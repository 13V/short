//! # short-perp (Anchor program) — SCAFFOLD
//!
//! On-chain venue for **fully-collateralized, capped-payout matched shorts** on
//! *graduated* pump.fun tokens. All risk/oracle/settlement math is delegated to
//! the host-tested [`short_core`] crate so the exact logic proven in
//! `core/tests/` runs on-chain.
//!
//! ## Status
//! This is a reviewable scaffold, NOT audited/deployable code. It compiles under
//! `anchor build` with the Anchor 0.31 toolchain (not built by the root
//! workspace). The two things that MUST be completed against mainnet before this
//! is real are marked `TODO(verify)`:
//!   1. Deserializing the pump.fun `BondingCurve` and PumpSwap pool accounts to
//!      read reserves — the byte layout must be confirmed against a live IDL.
//!   2. The SPL-token custody transfers in `settle` (wiring `anchor_spl`).
//!
//! ## Instruction flow
//! * `initialize_market` — list a graduated token after the TVL/age gate.
//! * `crank_oracle`      — permissionless: sample the pool, clamp, accumulate TWAP.
//! * `open_matched_short`— a short and a long escrow collateral into a position.
//! * `settle`            — permissionless: pay out both legs at the capped split.

use anchor_lang::prelude::*;
use short_core::market::{can_open, is_listable, MarketParams};
use short_core::oracle::Oracle;
use short_core::position::MatchedShort;

declare_id!("Short111111111111111111111111111111111111111"); // TODO(verify): replace with the deployed program id

#[program]
pub mod short_perp {
    use super::*;

    /// List a token. Fails unless it is graduated, deep enough, and old enough.
    pub fn initialize_market(
        ctx: Context<InitializeMarket>,
        params: MarketParamsArg,
        graduated: bool,
        pool_tvl: u128,
        token_age_slots: u64,
        initial_price: u128,
    ) -> Result<()> {
        let mp: MarketParams = params.into();
        is_listable(&mp, graduated, pool_tvl, token_age_slots).map_err(err)?;

        let clock = Clock::get()?;
        let oracle = Oracle::new(
            initial_price,
            clock.slot,
            mp.max_oi_bps_of_tvl, // NOTE: choose distinct clamp/breaker params in prod
            5_000,
            25,
        )
        .map_err(err)?;

        let market = &mut ctx.accounts.market;
        market.mint = ctx.accounts.mint.key();
        market.authority = ctx.accounts.authority.key();
        market.params = params;
        market.open_interest = 0;
        market.oracle = oracle.into();
        market.bump = ctx.bumps.market;
        Ok(())
    }

    /// Permissionless crank: feed a fresh pool price into the oracle.
    ///
    /// TODO(verify): derive `raw_price` from the PumpSwap pool reserves via CPI /
    /// account deserialization instead of trusting a passed-in value.
    pub fn crank_oracle(ctx: Context<CrankOracle>, raw_price: u128) -> Result<()> {
        let clock = Clock::get()?;
        let market = &mut ctx.accounts.market;
        let mut oracle: Oracle = market.oracle.into();
        oracle.observe(raw_price, clock.slot).map_err(err)?;
        market.oracle = oracle.into();
        Ok(())
    }

    /// Open a matched short: the short and the long escrow collateral, and a
    /// position account is created. Rejected if the oracle breaker is tripped or
    /// the liquidity-gated OI cap would be exceeded.
    ///
    /// TODO(verify): move the escrow SPL transfers here (anchor_spl::token) into
    /// a per-position vault PDA.
    pub fn open_matched_short(
        ctx: Context<OpenMatchedShort>,
        notional: u128,
        cap_up_bps: u16,
        pool_tvl: u128,
    ) -> Result<()> {
        let market = &mut ctx.accounts.market;
        let oracle: Oracle = market.oracle.into();
        require!(!oracle.tripped, ShortPerpError::CircuitBreakerTripped);

        let mp: MarketParams = market.params.into();
        can_open(&mp, market.open_interest, notional, pool_tvl).map_err(err)?;

        let pos = MatchedShort::new_symmetric(oracle.accepted_price, notional, cap_up_bps)
            .map_err(err)?;

        // TODO(verify): transfer `pos.short_collateral` from the short and
        // `pos.long_collateral` from the long into the position vault here.

        let position = &mut ctx.accounts.position;
        position.market = market.key();
        position.short_owner = ctx.accounts.short_owner.key();
        position.long_owner = ctx.accounts.long_owner.key();
        position.entry_price = pos.entry_price;
        position.notional = pos.notional;
        position.short_collateral = pos.short_collateral;
        position.long_collateral = pos.long_collateral;
        position.settled = false;
        position.bump = ctx.bumps.position;

        market.open_interest = market
            .open_interest
            .checked_add(notional)
            .ok_or_else(|| error!(ShortPerpError::MathOverflow))?;
        Ok(())
    }

    /// Permissionless settle at the current TWAP mark. Pays both legs the capped
    /// split; the protocol fee is skimmed from the winner's gain only.
    ///
    /// TODO(verify): use a TWAP over a window (two cumulative snapshots) as the
    /// mark, and perform the SPL payouts from the vault.
    pub fn settle(ctx: Context<Settle>) -> Result<()> {
        let market = &ctx.accounts.market;
        let position = &mut ctx.accounts.position;
        require!(!position.settled, ShortPerpError::AlreadySettled);

        let oracle: Oracle = market.oracle.into();
        let mp: MarketParams = market.params.into();

        let pos = MatchedShort {
            entry_price: position.entry_price,
            notional: position.notional,
            short_collateral: position.short_collateral,
            long_collateral: position.long_collateral,
        };
        // TODO(verify): replace accepted_price with a windowed TWAP mark.
        let settlement = pos.settle(oracle.accepted_price, mp.fee_bps).map_err(err)?;

        // TODO(verify): pay `settlement.short_payout` / `.long_payout` /
        // `.protocol_fee` from the vault via anchor_spl transfers.
        let _ = settlement;

        position.settled = true;
        Ok(())
    }
}

/// Map a `short_core::ShortError` onto the Anchor error space.
fn err(_e: short_core::ShortError) -> Error {
    error!(ShortPerpError::CoreRejected)
}

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

#[account]
pub struct Market {
    pub mint: Pubkey,
    pub authority: Pubkey,
    pub params: MarketParamsArg,
    pub open_interest: u128,
    pub oracle: OracleState,
    pub bump: u8,
}

#[account]
pub struct Position {
    pub market: Pubkey,
    pub short_owner: Pubkey,
    pub long_owner: Pubkey,
    pub entry_price: u128,
    pub notional: u128,
    pub short_collateral: u128,
    pub long_collateral: u128,
    pub settled: bool,
    pub bump: u8,
}

/// Anchor-serializable mirror of `short_core::MarketParams`.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default)]
pub struct MarketParamsArg {
    pub min_pool_tvl: u128,
    pub min_age_slots: u64,
    pub max_oi_bps_of_tvl: u16,
    pub fee_bps: u16,
}

impl From<MarketParamsArg> for MarketParams {
    fn from(a: MarketParamsArg) -> Self {
        MarketParams {
            min_pool_tvl: a.min_pool_tvl,
            min_age_slots: a.min_age_slots,
            max_oi_bps_of_tvl: a.max_oi_bps_of_tvl,
            fee_bps: a.fee_bps,
        }
    }
}

/// Anchor-serializable mirror of `short_core::oracle::Oracle`.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default)]
pub struct OracleState {
    pub accepted_price: u128,
    pub last_slot: u64,
    pub cumulative: u128,
    pub max_move_bps: u16,
    pub breaker_bps: u16,
    pub staleness_slots: u64,
    pub tripped: bool,
}

impl From<OracleState> for Oracle {
    fn from(s: OracleState) -> Self {
        Oracle {
            accepted_price: s.accepted_price,
            last_slot: s.last_slot,
            cumulative: s.cumulative,
            max_move_bps: s.max_move_bps,
            breaker_bps: s.breaker_bps,
            staleness_slots: s.staleness_slots,
            tripped: s.tripped,
        }
    }
}

impl From<Oracle> for OracleState {
    fn from(o: Oracle) -> Self {
        OracleState {
            accepted_price: o.accepted_price,
            last_slot: o.last_slot,
            cumulative: o.cumulative,
            max_move_bps: o.max_move_bps,
            breaker_bps: o.breaker_bps,
            staleness_slots: o.staleness_slots,
            tripped: o.tripped,
        }
    }
}

// ---------------------------------------------------------------------------
// Contexts (space sizing is illustrative; use `InitSpace` in prod)
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeMarket<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK: token mint being listed.
    pub mint: UncheckedAccount<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + 256,
        seeds = [b"market", mint.key().as_ref()],
        bump
    )]
    pub market: Account<'info, Market>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CrankOracle<'info> {
    #[account(mut, seeds = [b"market", market.mint.as_ref()], bump = market.bump)]
    pub market: Account<'info, Market>,
    // TODO(verify): pass the PumpSwap pool accounts here to read reserves.
}

#[derive(Accounts)]
pub struct OpenMatchedShort<'info> {
    #[account(mut, seeds = [b"market", market.mint.as_ref()], bump = market.bump)]
    pub market: Account<'info, Market>,
    #[account(mut)]
    pub short_owner: Signer<'info>,
    #[account(mut)]
    pub long_owner: Signer<'info>,
    #[account(
        init,
        payer = short_owner,
        space = 8 + 256,
        seeds = [b"position", market.key().as_ref(), short_owner.key().as_ref(), long_owner.key().as_ref()],
        bump
    )]
    pub position: Account<'info, Position>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Settle<'info> {
    #[account(seeds = [b"market", market.mint.as_ref()], bump = market.bump)]
    pub market: Account<'info, Market>,
    #[account(mut, has_one = market)]
    pub position: Account<'info, Position>,
}

#[error_code]
pub enum ShortPerpError {
    #[msg("core risk/oracle/settlement math rejected the operation")]
    CoreRejected,
    #[msg("oracle circuit breaker is tripped; new positions frozen")]
    CircuitBreakerTripped,
    #[msg("position already settled")]
    AlreadySettled,
    #[msg("math overflow")]
    MathOverflow,
}
