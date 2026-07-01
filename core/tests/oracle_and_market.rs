//! Tests for the oracle (clamp / TWAP / circuit breaker) and the market gate.

use short_core::market::{can_open, is_listable, max_open_interest, MarketParams};
use short_core::oracle::{twap_between, Oracle};
use short_core::{price_from_reserves, PRICE_SCALE};

fn oracle_1() -> Oracle {
    // accepted price 1.0, max move 2%/update, breaker at 50% raw deviation.
    Oracle::new(PRICE_SCALE, 100, 200, 5_000, 25).unwrap()
}

#[test]
fn clamp_bounds_a_single_spike() {
    let o = oracle_1();
    // A 400% spike is clamped to +2%.
    let clamped = o.clamp(5 * PRICE_SCALE).unwrap();
    assert_eq!(clamped, PRICE_SCALE + PRICE_SCALE * 200 / 10_000);
    // A crash to zero is clamped to -2%.
    let clamped = o.clamp(0).unwrap();
    assert_eq!(clamped, PRICE_SCALE - PRICE_SCALE * 200 / 10_000);
    // A small move within the band passes through.
    let small = PRICE_SCALE + PRICE_SCALE * 100 / 10_000; // +1%
    assert_eq!(o.clamp(small).unwrap(), small);
}

#[test]
fn breaker_trips_on_large_raw_move() {
    let mut o = oracle_1();
    // +1% raw move: fine.
    o.observe(PRICE_SCALE + PRICE_SCALE / 100, 110).unwrap();
    assert!(!o.tripped);
    // +400% raw move: breaker trips (even though accepted price only clamps up).
    o.observe(5 * PRICE_SCALE, 120).unwrap();
    assert!(o.tripped);
}

#[test]
fn twap_averages_over_window() {
    // Hold at 1.0 for 20 slots, step up (clamped to +2%), then hold the new
    // price for a further 10 slots so it actually accrues time in the average.
    let mut o = oracle_1();
    let slot_start = o.last_slot;
    let cum_start = o.cumulative_at(slot_start).unwrap();

    // 20 slots at 1.0 (the accumulator records the price in force each interval).
    o.observe(PRICE_SCALE, slot_start + 20).unwrap();
    // Step up: raw 2.0 clamps to +2%. This price takes effect from this slot on.
    let up = o.observe(2 * PRICE_SCALE, slot_start + 21).unwrap();

    // Let the new (higher) price accrue for 10 more slots before measuring.
    let slot_end = slot_start + 31;
    let cum_end = o.cumulative_at(slot_end).unwrap();

    let twap = twap_between(cum_start, slot_start, cum_end, slot_end).unwrap();
    // TWAP sits strictly between the initial 1.0 (dominant, held longest) and the
    // clamped `up` — proving it both rose and lagged the latest price.
    assert!(twap > PRICE_SCALE, "twap should have risen above 1.0");
    assert!(twap < up, "twap should lag the latest clamped price");
}

#[test]
fn stale_and_monotonic_guards() {
    let mut o = oracle_1();
    assert!(!o.is_stale(120)); // last_slot 100, staleness 25 -> 120 ok
    assert!(o.is_stale(200)); // way past
    // Non-monotonic slot is rejected.
    assert!(o.observe(PRICE_SCALE, 100).is_err());
    assert!(o.observe(PRICE_SCALE, 99).is_err());
}

#[test]
fn price_from_reserves_matches_expectation() {
    // 1_000 quote / 1_000 base = price 1.0
    assert_eq!(price_from_reserves(1_000, 1_000).unwrap(), PRICE_SCALE);
    // Twice as much quote per base -> price 2.0
    assert_eq!(price_from_reserves(1_000, 2_000).unwrap(), 2 * PRICE_SCALE);
    // Draining base (memecoin bought up) raises price.
    assert!(price_from_reserves(500, 1_000).unwrap() > PRICE_SCALE);
    // Zero base reserve is an error, not a panic.
    assert!(price_from_reserves(0, 1_000).is_err());
}

#[test]
fn listing_gate_rejects_ungraduated_and_thin() {
    let p = MarketParams::phase0_defaults();
    // Graduated, deep, old enough -> listable.
    assert!(is_listable(&p, true, p.min_pool_tvl, p.min_age_slots).is_ok());
    // Not graduated -> rejected regardless of depth.
    assert!(is_listable(&p, false, p.min_pool_tvl * 10, p.min_age_slots * 10).is_err());
    // Too thin -> rejected.
    assert!(is_listable(&p, true, p.min_pool_tvl - 1, p.min_age_slots).is_err());
    // Too young -> rejected.
    assert!(is_listable(&p, true, p.min_pool_tvl, p.min_age_slots - 1).is_err());
}

#[test]
fn oi_cap_scales_with_pool_depth() {
    let p = MarketParams::phase0_defaults(); // 5% of TVL
    let tvl = 1_000_000_000_000u128;
    assert_eq!(max_open_interest(&p, tvl).unwrap(), tvl * 500 / 10_000);

    // Opening within the cap is fine.
    assert!(can_open(&p, 0, tvl * 400 / 10_000, tvl).is_ok());
    // Opening past the cap fails.
    assert!(can_open(&p, 0, tvl * 600 / 10_000, tvl).is_err());
    // As pool depth shrinks, the same OI can breach the (now smaller) cap.
    let existing = tvl * 400 / 10_000;
    assert!(can_open(&p, existing, 1, tvl).is_ok());
    assert!(can_open(&p, existing, 1, tvl / 2).is_err());
}
