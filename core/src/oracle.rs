//! Manipulation-hardened price oracle.
//!
//! This is the single most important safety component (see the feasibility
//! report, §4.1 and §6). A launchpad token's price is a single thin AMM read
//! that one actor can move hundreds of percent. We never mark to a raw spot
//! read. Instead we layer three defenses:
//!
//! 1. **Per-update clamp** — a single observation can move the accepted price by
//!    at most `max_move_bps`. An attacker who spikes the pool 400% in one block
//!    only moves our accepted price by, say, 2%.
//! 2. **Cumulative TWAP** — we accumulate `accepted_price * slots` (Uniswap-v2
//!    style). Marking uses a time-weighted average over a window, so sustaining
//!    a manipulated price costs the attacker for the *entire* window, not one
//!    block.
//! 3. **Circuit breaker** — if a *raw* observation deviates from the accepted
//!    price by more than `breaker_bps`, the oracle trips: new positions freeze
//!    and liquidations pause until it is manually/rule-based reset.
//!
//! The clamp bounds *how fast* the mark can move; the TWAP window bounds *how
//! long* a manipulation must be sustained; together they set the attacker's
//! minimum cost, which §2 of the build checklist calibrates against pool depth.

use crate::errors::ShortError;
use crate::math::{abs_move_bps, mul_div};

/// Rolling cumulative-price oracle for one market.
///
/// Store this in the on-chain market account. `observe()` is called by the
/// permissionless crank each time it samples the pool. To read a TWAP, snapshot
/// [`Oracle::cumulative`] at two times and divide the difference by the elapsed
/// slots (see [`twap_between`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Oracle {
    /// The last *accepted* (clamped) price, scaled by `PRICE_SCALE`.
    pub accepted_price: u128,
    /// Slot of the last accepted observation.
    pub last_slot: u64,
    /// Sum over time of `accepted_price * slots_elapsed`. Diff two snapshots to
    /// get a time-weighted average.
    pub cumulative: u128,
    /// Max bps a single observation may move `accepted_price`.
    pub max_move_bps: u16,
    /// Raw deviation (bps) beyond which the circuit breaker trips.
    pub breaker_bps: u16,
    /// Max slots an observation may be stale before it's rejected as too old.
    pub staleness_slots: u64,
    /// Whether the breaker is currently tripped (new positions frozen).
    pub tripped: bool,
}

impl Oracle {
    /// Initialize from a first trusted price (e.g. the pool price at listing
    /// time, after the TVL/age gate has already been passed).
    pub fn new(
        initial_price: u128,
        slot: u64,
        max_move_bps: u16,
        breaker_bps: u16,
        staleness_slots: u64,
    ) -> Result<Self, ShortError> {
        if initial_price == 0 {
            return Err(ShortError::ZeroPrice);
        }
        Ok(Self {
            accepted_price: initial_price,
            last_slot: slot,
            cumulative: 0,
            max_move_bps,
            breaker_bps,
            staleness_slots,
            tripped: false,
        })
    }

    /// Clamp `raw_price` to within `max_move_bps` of the current accepted price.
    pub fn clamp(&self, raw_price: u128) -> Result<u128, ShortError> {
        let max_delta = mul_div(self.accepted_price, self.max_move_bps as u128, crate::math::BPS_DENOM)?;
        let upper = self.accepted_price.saturating_add(max_delta);
        let lower = self.accepted_price.saturating_sub(max_delta);
        Ok(raw_price.clamp(lower, upper))
    }

    /// Ingest a new raw pool price observed at `slot`.
    ///
    /// Steps: (a) reject non-monotonic slots, (b) advance the cumulative
    /// accumulator with the *previous* accepted price over the elapsed slots,
    /// (c) trip the breaker if the raw price deviates too far, (d) move the
    /// accepted price by at most the clamp.
    ///
    /// Returns the new accepted price. Even when the breaker trips we still
    /// clamp+record so the TWAP keeps advancing conservatively.
    pub fn observe(&mut self, raw_price: u128, slot: u64) -> Result<u128, ShortError> {
        if raw_price == 0 {
            return Err(ShortError::ZeroPrice);
        }
        if slot <= self.last_slot {
            return Err(ShortError::NonMonotonicSlot);
        }

        // (b) advance cumulative with the price that was in force until now.
        let dt = (slot - self.last_slot) as u128;
        let contribution = self
            .accepted_price
            .checked_mul(dt)
            .ok_or(ShortError::MathOverflow)?;
        self.cumulative = self
            .cumulative
            .checked_add(contribution)
            .ok_or(ShortError::MathOverflow)?;

        // (c) circuit breaker on the RAW deviation (pre-clamp), so we can detect
        // an attack even though the clamp would have absorbed it.
        if abs_move_bps(self.accepted_price, raw_price)? > self.breaker_bps as u128 {
            self.tripped = true;
        }

        // (d) clamp the accepted price move.
        self.accepted_price = self.clamp(raw_price)?;
        self.last_slot = slot;
        Ok(self.accepted_price)
    }

    /// Is the last observation older than `staleness_slots` as of `now_slot`?
    pub fn is_stale(&self, now_slot: u64) -> bool {
        now_slot.saturating_sub(self.last_slot) > self.staleness_slots
    }

    /// Cumulative value as of `now_slot`, extrapolating the current accepted
    /// price forward. Snapshot this at two times to compute a TWAP.
    pub fn cumulative_at(&self, now_slot: u64) -> Result<u128, ShortError> {
        if now_slot < self.last_slot {
            return Err(ShortError::NonMonotonicSlot);
        }
        let dt = (now_slot - self.last_slot) as u128;
        let extra = self
            .accepted_price
            .checked_mul(dt)
            .ok_or(ShortError::MathOverflow)?;
        self.cumulative.checked_add(extra).ok_or(ShortError::MathOverflow)
    }

    /// Manually reset the breaker (e.g. after governance/rule review, once the
    /// pool has been calm for N slots). Rebases the accepted price to `price`.
    pub fn reset_breaker(&mut self, price: u128, slot: u64) -> Result<(), ShortError> {
        if price == 0 {
            return Err(ShortError::ZeroPrice);
        }
        if slot < self.last_slot {
            return Err(ShortError::NonMonotonicSlot);
        }
        self.tripped = false;
        self.accepted_price = price;
        self.last_slot = slot;
        Ok(())
    }
}

/// Time-weighted average price between two cumulative snapshots.
///
/// `twap = (cum_end - cum_start) / (slot_end - slot_start)`.
pub fn twap_between(
    cum_start: u128,
    slot_start: u64,
    cum_end: u128,
    slot_end: u64,
) -> Result<u128, ShortError> {
    if slot_end <= slot_start {
        return Err(ShortError::EmptyWindow);
    }
    let span = (slot_end - slot_start) as u128;
    let delta = cum_end.checked_sub(cum_start).ok_or(ShortError::MathOverflow)?;
    Ok(delta / span)
}
