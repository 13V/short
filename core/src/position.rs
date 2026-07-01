//! Fully-collateralized, capped-payout, matched synthetic short.
//!
//! This is the Phase-0 product (feasibility report §3, §6). Two users escrow
//! collateral into a vault keyed to a token mint:
//!
//! * the **short** profits when the mark price falls,
//! * the **long** (counterparty / yield-seeker) profits when it rises.
//!
//! The settlement transfer between them is **clamped to the escrowed
//! collateral**, so:
//!
//! * neither side can lose more than it put in ⇒ **no bad debt, ever**,
//! * there is no shared LP pool to drain ⇒ the JELLY attack class cannot
//!   socialize a loss onto the protocol,
//! * there is no leverage and no liquidation auction — settlement is a pure
//!   arithmetic split a permissionless crank can run.
//!
//! The invariants `short_payout + long_payout + protocol_fee == total escrow`
//! (conservation) and `short_payout >= 0 && long_payout >= 0` (no bad debt) are
//! proven exhaustively in `tests/settlement.rs`.

use crate::errors::ShortError;
use crate::math::{apply_bps, mul_div, BPS_DENOM};

/// The result of settling a matched short. All fields are non-negative and sum
/// to the total escrowed collateral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settlement {
    /// Amount returned to the short side.
    pub short_payout: u128,
    /// Amount returned to the long side.
    pub long_payout: u128,
    /// Amount taken by the protocol (fee on the winner's realized gain only).
    pub protocol_fee: u128,
}

/// A matched short position. Both legs are pre-funded; `short_collateral` and
/// `long_collateral` define the capped payout in each direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchedShort {
    /// Entry price (quote per base, scaled by `PRICE_SCALE`).
    pub entry_price: u128,
    /// Position size in quote units at entry (the "notional").
    pub notional: u128,
    /// Collateral escrowed by the short (bounds the short's max loss).
    pub short_collateral: u128,
    /// Collateral escrowed by the long (bounds the long's max loss).
    pub long_collateral: u128,
}

impl MatchedShort {
    /// Collateral required for a *symmetric* fully-collateralized short.
    ///
    /// * The long's max loss is the price going to zero: `short_pnl = notional`,
    ///   so the long must escrow the full `notional`.
    /// * The short's max loss is the price rising by `cap_up_bps`:
    ///   `short_pnl = -notional * cap_up_bps/10_000`, so the short escrows that.
    ///
    /// Returns `(short_collateral, long_collateral)`. A `cap_up_bps` of 10_000
    /// (100%) means the short is protected up to a 2x move; beyond that the long
    /// simply cannot gain more (the short is fully liquidated at the cap).
    pub fn required_collateral(notional: u128, cap_up_bps: u16) -> Result<(u128, u128), ShortError> {
        let short_collateral = apply_bps(notional, cap_up_bps)?;
        Ok((short_collateral, notional))
    }

    /// Build a symmetric matched short, computing both collateral legs.
    pub fn new_symmetric(
        entry_price: u128,
        notional: u128,
        cap_up_bps: u16,
    ) -> Result<Self, ShortError> {
        if entry_price == 0 {
            return Err(ShortError::ZeroPrice);
        }
        let (short_collateral, long_collateral) = Self::required_collateral(notional, cap_up_bps)?;
        Ok(Self {
            entry_price,
            notional,
            short_collateral,
            long_collateral,
        })
    }

    /// Signed PnL of the SHORT leg at `mark_price`, in quote units.
    ///
    /// `short_pnl = notional * (entry - mark) / entry`.
    /// Positive ⇒ short is winning (price fell). Negative ⇒ short is losing.
    /// This is the *uncapped* economic PnL; [`settle`](Self::settle) applies the
    /// collateral cap.
    pub fn short_pnl(&self, mark_price: u128) -> Result<i128, ShortError> {
        if self.entry_price == 0 {
            return Err(ShortError::ZeroPrice);
        }
        let entry = self.entry_price as i128;
        let mark = mark_price as i128;
        let notional = self.notional as i128;
        let diff = entry.checked_sub(mark).ok_or(ShortError::MathOverflow)?;
        let scaled = notional.checked_mul(diff).ok_or(ShortError::MathOverflow)?;
        Ok(scaled / entry)
    }

    /// Is the position at (or beyond) either payout cap, so a keeper should
    /// force-settle it? True when the short is fully liquidated (mark high) or
    /// the long is fully liquidated (mark at/near zero).
    pub fn is_settleable(&self, mark_price: u128) -> Result<bool, ShortError> {
        let pnl = self.short_pnl(mark_price)?;
        let cs = self.short_collateral as i128;
        let cl = self.long_collateral as i128;
        Ok(pnl <= -cs || pnl >= cl)
    }

    /// Settle the position at `mark_price`, applying the collateral caps and the
    /// protocol fee.
    ///
    /// The transfer from long→short is `clamp(short_pnl, -short_collateral,
    /// +long_collateral)`. The fee is charged only on the winner's realized
    /// gain, so the loser is never charged beyond its escrow and no path can
    /// produce a negative payout. `fee_bps` must be <= 10_000.
    pub fn settle(&self, mark_price: u128, fee_bps: u16) -> Result<Settlement, ShortError> {
        if fee_bps as u128 > BPS_DENOM {
            return Err(ShortError::InvalidBps);
        }
        let pnl = self.short_pnl(mark_price)?;
        let cs = self.short_collateral as i128;
        let cl = self.long_collateral as i128;

        // Transfer from long to short, bounded by each side's escrow.
        let transfer = pnl.clamp(-cs, cl);

        // Gross payouts before fee. Both are >= 0 by construction:
        //   short_gross = cs + transfer, and transfer >= -cs  ⇒ >= 0
        //   long_gross  = cl - transfer, and transfer <= cl   ⇒ >= 0
        let short_gross = cs + transfer;
        let long_gross = cl - transfer;

        // Fee on the winner's realized gain only.
        let (short_payout, long_payout, protocol_fee) = if transfer > 0 {
            // short won `transfer` from the long.
            let fee = apply_bps(transfer as u128, fee_bps)?;
            ((short_gross as u128) - fee, long_gross as u128, fee)
        } else if transfer < 0 {
            // long won `-transfer` from the short.
            let fee = apply_bps((-transfer) as u128, fee_bps)?;
            (short_gross as u128, (long_gross as u128) - fee, fee)
        } else {
            (short_gross as u128, long_gross as u128, 0)
        };

        Ok(Settlement {
            short_payout,
            long_payout,
            protocol_fee,
        })
    }

    /// Total escrow held by the vault for this position.
    pub fn total_escrow(&self) -> Result<u128, ShortError> {
        self.short_collateral
            .checked_add(self.long_collateral)
            .ok_or(ShortError::MathOverflow)
    }
}

/// Convert a desired leverage-1 notional and entry price into a base-token
/// amount, for display / UX. `base_amount = notional * PRICE_SCALE / entry`.
pub fn notional_to_base(notional: u128, entry_price: u128) -> Result<u128, ShortError> {
    if entry_price == 0 {
        return Err(ShortError::ZeroPrice);
    }
    mul_div(notional, crate::math::PRICE_SCALE, entry_price)
}
