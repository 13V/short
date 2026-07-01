# short-perp (on-chain program)

Anchor program that wraps the host-tested [`short-core`](../../core) math into a
Solana venue for **fully-collateralized, capped-payout matched shorts** on
graduated pump.fun tokens.

> **This is a reviewable scaffold, not audited or deployable code.** It is
> intentionally excluded from the root `cargo` workspace (which only builds and
> tests the pure-Rust `core` crate offline). Build this program separately with
> the Anchor toolchain.

## Build (requires the Solana + Anchor toolchain)

```bash
# One-time toolchain install (not present in every sandbox):
#   sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"   # solana
#   cargo install --git https://github.com/coral-xyz/anchor avm --locked
#   avm install 0.31.0 && avm use 0.31.0

anchor build          # compiles the BPF program + generates the IDL
anchor test           # spins up a local validator and runs tests/
```

## What still needs mainnet verification (`TODO(verify)` in `src/lib.rs`)

1. **Pool/curve deserialization.** `crank_oracle` currently trusts a passed-in
   `raw_price`. In production it must read PumpSwap pool reserves (and detect
   graduation via the pump.fun `BondingCurve.complete` flag) by deserializing
   those accounts — whose exact byte layout must be confirmed against a live IDL
   before trusting an offset.
2. **SPL custody.** `open_matched_short` / `settle` do not yet move tokens. Wire
   `anchor_spl::token` transfers into a per-position vault PDA, and pay out
   `short_payout` / `long_payout` / `protocol_fee` on settle.
3. **Windowed TWAP mark.** `settle` currently marks at `accepted_price`; switch
   to a TWAP computed from two cumulative snapshots (`short_core::twap_between`).
4. **Account sizing.** Space is illustrative; use `#[derive(InitSpace)]`.

The financial invariants (conservation, no-bad-debt, OI caps, oracle clamp) live
in `short-core` and are already proven by `cargo test` in that crate — this
program is the thin, verifiable on-chain shell around them.
