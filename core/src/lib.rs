//! # short-core
//!
//! Pure-Rust risk, oracle, and capped-payout settlement math for a
//! fully-collateralized on-chain venue for shorting *graduated* pump.fun
//! memecoins. See `docs/feasibility-and-architecture.md` for the why.
//!
//! This crate has **zero third-party dependencies** and is `no_std`, so the
//! exact same audited logic runs:
//!   * inside the Solana/Anchor program (`programs/short-perp`), and
//!   * off-chain in keepers, matchers, and backtests.
//!
//! ## The one guarantee
//! Every settlement is a *conservative, capped* split of pre-funded collateral:
//! neither side can lose more than it escrowed, so the protocol can never take
//! on bad debt or socialize a loss (the JELLY failure mode). This is proven by
//! the exhaustive/fuzz tests in `tests/settlement.rs`.
//!
//! ## Module map
//! * [`math`] — integer fixed-point helpers (prices scaled by `PRICE_SCALE`).
//! * [`oracle`] — clamp + cumulative TWAP + circuit breaker.
//! * [`market`] — listing gate + liquidity-gated open-interest cap.
//! * [`position`] — the matched short and its capped-payout settlement.
//! * [`errors`] — the shared error type.

#![cfg_attr(not(test), no_std)]

pub mod errors;
pub mod market;
pub mod math;
pub mod oracle;
pub mod position;

pub use errors::ShortError;
pub use market::{can_open, is_listable, max_open_interest, MarketParams};
pub use math::{apply_bps, price_from_reserves, BPS_DENOM, PRICE_SCALE};
pub use oracle::{twap_between, Oracle};
pub use position::{MatchedShort, Settlement};
