//! Error type shared by every module. Kept small and `Copy` so it can be
//! mapped 1:1 onto an Anchor `#[error_code]` enum in the on-chain program.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortError {
    /// A price was zero where a positive price is required (division guard).
    ZeroPrice,
    /// A reserve was zero where a positive reserve is required.
    ZeroReserve,
    /// Integer overflow/underflow in checked arithmetic.
    MathOverflow,
    /// basis-point argument exceeded 10_000 (100%).
    InvalidBps,
    /// Oracle observation arrived with a slot <= the last recorded slot.
    NonMonotonicSlot,
    /// TWAP window requested spans zero slots (cannot average).
    EmptyWindow,
    /// The token is not eligible to be listed (fails TVL/age/graduation gate).
    NotListable,
    /// Opening this position would exceed the liquidity-gated open-interest cap.
    OiCapExceeded,
    /// The oracle circuit breaker is tripped; new positions are frozen.
    CircuitBreakerTripped,
    /// The two legs of a matched short are not balanced / not fully funded.
    UnbalancedMatch,
}
