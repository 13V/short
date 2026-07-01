//! Market listing gate and the liquidity-gated open-interest cap.
//!
//! Two independent safety layers live here (feasibility report §6):
//!
//! * **Listing gate** — a token is only tradable if it has graduated to the
//!   PumpSwap AMM, its pool TVL is above a floor, and it has existed long enough.
//!   Pre-graduation bonding-curve tokens are refused outright in Phase 0.
//! * **Liquidity-gated OI cap** — total open short notional per token is capped
//!   at a small fraction of live pool depth. This is what makes the oracle
//!   attack uneconomic: you cannot open enough notional for a manipulation to
//!   pay for itself. It is the single most important tunable.

use crate::errors::ShortError;
use crate::math::apply_bps;

/// Static, per-market risk parameters. Set at listing, adjustable by governance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarketParams {
    /// Minimum PumpSwap pool TVL (in quote units, e.g. USDC atomic) to list.
    pub min_pool_tvl: u128,
    /// Minimum token age (in slots) before it may be listed.
    pub min_age_slots: u64,
    /// Max total open short notional as a fraction of live pool TVL, in bps.
    /// e.g. 500 = shorts may never exceed 5% of pool depth.
    pub max_oi_bps_of_tvl: u16,
    /// Protocol fee charged on a position's realized PnL transfer, in bps.
    pub fee_bps: u16,
}

impl MarketParams {
    /// Conservative Phase-0 defaults. These are deliberately strict; loosen only
    /// after the empirical manipulation-cost calibration in the build checklist.
    pub fn phase0_defaults() -> Self {
        Self {
            min_pool_tvl: 100_000_000_000, // $100k at 1e6 USDC scale
            min_age_slots: 216_000,        // ~24h at ~400ms/slot
            max_oi_bps_of_tvl: 500,        // 5% of pool depth
            fee_bps: 30,                   // 0.30%
        }
    }
}

/// Whether a token may be listed given its current on-chain facts.
///
/// `graduated` must come from the pump.fun `BondingCurve.complete` flag (or an
/// equivalent PumpSwap-pool-exists check). Phase 0 lists graduated tokens ONLY.
pub fn is_listable(
    params: &MarketParams,
    graduated: bool,
    pool_tvl: u128,
    token_age_slots: u64,
) -> Result<(), ShortError> {
    if graduated && pool_tvl >= params.min_pool_tvl && token_age_slots >= params.min_age_slots {
        Ok(())
    } else {
        Err(ShortError::NotListable)
    }
}

/// Maximum total short notional allowed for a market at the given live TVL.
pub fn max_open_interest(params: &MarketParams, pool_tvl: u128) -> Result<u128, ShortError> {
    apply_bps(pool_tvl, params.max_oi_bps_of_tvl)
}

/// Can `additional_notional` be opened without exceeding the OI cap?
///
/// `current_oi` is the market's existing open short notional; `pool_tvl` is the
/// *current* live pool depth (re-read every time — depth shrinks as the pool is
/// drained, and the cap must shrink with it).
pub fn can_open(
    params: &MarketParams,
    current_oi: u128,
    additional_notional: u128,
    pool_tvl: u128,
) -> Result<(), ShortError> {
    let cap = max_open_interest(params, pool_tvl)?;
    let projected = current_oi
        .checked_add(additional_notional)
        .ok_or(ShortError::MathOverflow)?;
    if projected <= cap {
        Ok(())
    } else {
        Err(ShortError::OiCapExceeded)
    }
}
