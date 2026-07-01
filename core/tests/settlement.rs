//! Invariant + fuzz tests for the capped-payout matched short.
//!
//! The two properties that make the whole product safe:
//!   * CONSERVATION: short_payout + long_payout + protocol_fee == total escrow
//!   * NO BAD DEBT:  short_payout >= 0 && long_payout >= 0   (always, all prices)
//!
//! We assert these on hand-picked edge cases AND on ~200k randomized inputs
//! using a tiny in-file PRNG (no external crates, so this runs offline).

use short_core::position::MatchedShort;
use short_core::PRICE_SCALE;

/// xorshift64* — deterministic, dependency-free PRNG for fuzzing.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    /// Uniform in [lo, hi].
    fn range(&mut self, lo: u128, hi: u128) -> u128 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() as u128) % (hi - lo + 1)
    }
}

/// Assert both core invariants for one settlement.
fn assert_invariants(pos: &MatchedShort, mark: u128, fee_bps: u16) {
    let total = pos.total_escrow().unwrap();
    let s = pos.settle(mark, fee_bps).unwrap();

    // NO BAD DEBT — payouts are u128 so >= 0 by type; assert they don't exceed
    // the total (which would imply the fee went negative / minting from nothing).
    assert!(s.short_payout <= total, "short payout exceeds escrow");
    assert!(s.long_payout <= total, "long payout exceeds escrow");

    // CONSERVATION — nothing created or destroyed.
    let sum = s.short_payout + s.long_payout + s.protocol_fee;
    assert_eq!(sum, total, "conservation violated: {sum} != {total}");

    // Winner never gains more than the counterparty's escrow.
    assert!(
        s.short_payout <= pos.short_collateral + pos.long_collateral,
        "short gained more than possible",
    );
    assert!(
        s.long_payout <= pos.short_collateral + pos.long_collateral,
        "long gained more than possible",
    );
}

#[test]
fn edge_cases() {
    // A symmetric short: notional 1_000, protected up to +100% (2x).
    let entry = PRICE_SCALE; // price 1.0
    let pos = MatchedShort::new_symmetric(entry, 1_000, 10_000).unwrap();
    assert_eq!(pos.short_collateral, 1_000);
    assert_eq!(pos.long_collateral, 1_000);

    // Price unchanged: both get their collateral back, no fee.
    let s = pos.settle(entry, 30).unwrap();
    assert_eq!(s.short_payout, 1_000);
    assert_eq!(s.long_payout, 1_000);
    assert_eq!(s.protocol_fee, 0);

    // Price to ZERO: short fully wins the long's escrow (minus fee).
    let s = pos.settle(0, 0).unwrap();
    assert_eq!(s.short_payout, 2_000);
    assert_eq!(s.long_payout, 0);

    // Price DOUBLES (+100%): short fully liquidated to the cap.
    let s = pos.settle(entry * 2, 0).unwrap();
    assert_eq!(s.short_payout, 0);
    assert_eq!(s.long_payout, 2_000);

    // Price 10x (way past the cap): still bounded — no bad debt.
    let s = pos.settle(entry * 10, 0).unwrap();
    assert_eq!(s.short_payout, 0);
    assert_eq!(s.long_payout, 2_000);

    // Halfway down (-50%): short gains half the long's escrow.
    let s = pos.settle(entry / 2, 0).unwrap();
    assert_eq!(s.short_payout, 1_500);
    assert_eq!(s.long_payout, 500);
}

#[test]
fn fee_only_on_winner_gain() {
    let entry = PRICE_SCALE;
    let pos = MatchedShort::new_symmetric(entry, 1_000_000, 10_000).unwrap();

    // Short wins 500_000 (price halves); fee is 1% of the GAIN, not notional.
    let s = pos.settle(entry / 2, 100).unwrap();
    assert_eq!(s.protocol_fee, 5_000); // 1% of 500_000
    assert_eq!(s.short_payout, 1_000_000 + 500_000 - 5_000);
    assert_eq!(s.long_payout, 500_000);
    assert_eq!(s.short_payout + s.long_payout + s.protocol_fee, 2_000_000);
}

#[test]
fn direction_is_monotonic() {
    // As price rises, the short's payout must be non-increasing.
    let entry = PRICE_SCALE;
    let pos = MatchedShort::new_symmetric(entry, 1_000_000, 10_000).unwrap();
    let mut last = u128::MAX;
    let mut mark = 0u128;
    while mark <= entry * 3 {
        let s = pos.settle(mark, 0).unwrap();
        assert!(s.short_payout <= last, "short payout increased as price rose");
        last = s.short_payout;
        mark += entry / 20;
    }
}

#[test]
fn is_settleable_at_caps() {
    let entry = PRICE_SCALE;
    let pos = MatchedShort::new_symmetric(entry, 1_000, 10_000).unwrap();
    assert!(!pos.is_settleable(entry).unwrap()); // at entry, not settleable
    assert!(pos.is_settleable(0).unwrap()); // long wiped
    assert!(pos.is_settleable(entry * 2).unwrap()); // short wiped
    assert!(pos.is_settleable(entry * 5).unwrap()); // past cap
}

#[test]
fn fuzz_invariants_hold_everywhere() {
    let mut rng = Rng(0x1234_5678_9abc_def0);
    for _ in 0..200_000 {
        // Random but realistic-ish magnitudes.
        let entry = rng.range(1, 5 * PRICE_SCALE).max(1);
        let notional = rng.range(1, 1_000_000_000);
        let cap_up_bps = rng.range(1, 10_000) as u16;
        let mark = rng.range(0, 20 * PRICE_SCALE);
        let fee_bps = rng.range(0, 10_000) as u16;

        let pos = MatchedShort::new_symmetric(entry, notional, cap_up_bps).unwrap();
        assert_invariants(&pos, mark, fee_bps);
    }
}

#[test]
fn fuzz_asymmetric_collateral() {
    // Even with arbitrary (mismatched) collateral legs, invariants must hold.
    let mut rng = Rng(0xdead_beef_cafe_babe);
    for _ in 0..100_000 {
        let entry = rng.range(1, 5 * PRICE_SCALE).max(1);
        let pos = MatchedShort {
            entry_price: entry,
            notional: rng.range(1, 1_000_000_000),
            short_collateral: rng.range(0, 1_000_000_000),
            long_collateral: rng.range(0, 1_000_000_000),
        };
        let mark = rng.range(0, 20 * PRICE_SCALE);
        let fee_bps = rng.range(0, 10_000) as u16;
        assert_invariants(&pos, mark, fee_bps);
    }
}
