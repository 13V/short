//! Integer-only fixed-point helpers.
//!
//! On Solana there are no floats, so everything is `u128`/`i128` with an
//! explicit scale. Prices are expressed as *quote atomic units per base atomic
//! unit*, multiplied by [`PRICE_SCALE`]. Because every PnL computation uses a
//! *ratio* of prices, the exact scale cancels out — it only needs to be
//! consistent between entry price and mark price.

use crate::errors::ShortError;

/// Fixed-point scale for prices (1e12). A price of `1.0` quote/base is stored
/// as `1_000_000_000_000`.
pub const PRICE_SCALE: u128 = 1_000_000_000_000;

/// Basis-point denominator (100% = 10_000 bps).
pub const BPS_DENOM: u128 = 10_000;

/// `a * b / d` computed in u128 with overflow + divide-by-zero guards.
///
/// The intermediate `a * b` is done in u128; callers must ensure it fits. For
/// the amounts we handle (token/USDC atomic units, prices ~1e12) this is safe,
/// and `checked_mul` catches the pathological cases instead of wrapping.
pub fn mul_div(a: u128, b: u128, d: u128) -> Result<u128, ShortError> {
    if d == 0 {
        return Err(ShortError::MathOverflow);
    }
    a.checked_mul(b)
        .ok_or(ShortError::MathOverflow)
        .map(|p| p / d)
}

/// Derive a price (scaled by [`PRICE_SCALE`]) from AMM reserves.
///
/// `price = quote_reserve * PRICE_SCALE / base_reserve`.
///
/// For a PumpSwap constant-product pool, `base` is the memecoin and `quote` is
/// SOL (or USDC). The result is "quote per base" — i.e. how much quote one base
/// token is worth. A larger memecoin balance / smaller quote balance ⇒ lower
/// price, exactly as expected.
pub fn price_from_reserves(base_reserve: u128, quote_reserve: u128) -> Result<u128, ShortError> {
    if base_reserve == 0 {
        return Err(ShortError::ZeroReserve);
    }
    mul_div(quote_reserve, PRICE_SCALE, base_reserve)
}

/// Apply `value * bps / 10_000` with guards. `bps` must be <= 10_000.
pub fn apply_bps(value: u128, bps: u16) -> Result<u128, ShortError> {
    if bps as u128 > BPS_DENOM {
        return Err(ShortError::InvalidBps);
    }
    mul_div(value, bps as u128, BPS_DENOM)
}

/// Absolute difference between two prices as a fraction of `reference`, in bps.
/// Used by the clamp and circuit breaker. Returns bps capped at u128 (can exceed
/// 10_000 for large moves — that's the point).
pub fn abs_move_bps(reference: u128, other: u128) -> Result<u128, ShortError> {
    if reference == 0 {
        return Err(ShortError::ZeroPrice);
    }
    let diff = reference.abs_diff(other);
    mul_div(diff, BPS_DENOM, reference)
}
