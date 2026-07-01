# short — safe on-chain shorting of pump.fun memecoins

A Solana venue for taking **bearish positions on graduated pump.fun memecoins**,
designed so the protocol can *never* take on bad debt.

> **The honest thesis (read `docs/feasibility-and-architecture.md` first).**
> You cannot safely "short a coin the second it launches" — a fresh pump.fun
> token has no borrowable inventory and a single thin pool that one actor can
> move hundreds of percent (this is exactly how Hyperliquid's ~$13.5M JELLY loss
> happened). Every serious venue refuses these assets for that reason. So the
> product here is deliberately the *safe* version: **fully-collateralized,
> capped-payout, matched shorts on tokens that have already graduated to the
> PumpSwap AMM and cleared liquidity + age gates**, priced by a
> manipulation-hardened oracle. Safety is the wedge, not a limitation. Leverage
> and fresh-launch tokens are later phases (or never).

## How it works (Phase 0)

Two users escrow collateral into a vault keyed to a token mint:

- the **short** profits when the mark price falls,
- the **long** (counterparty / yield-seeker) profits when it rises.

Settlement is a **clamped split of the pre-funded collateral** — neither side
can lose more than it escrowed. Consequences:

- **No bad debt, ever.** No leverage, no shared LP pool to drain, no socialized
  losses, no liquidation auction. Settlement is pure arithmetic a permissionless
  crank runs.
- **Manipulation-hardened oracle.** Prices are never marked to a raw spot read.
  A per-update **clamp** bounds how fast the mark can move, a cumulative **TWAP**
  forces an attacker to sustain a manipulation for the whole window, and a
  **circuit breaker** freezes new positions on abnormal moves.
- **Liquidity-gated open interest.** Total short notional per token is capped at
  a small fraction of live pool depth, so a manipulation can't be sized big
  enough to pay for itself.

## Repository layout

```
short/
├─ docs/
│  └─ feasibility-and-architecture.md   # the research: feasibility, competitors, full design
├─ core/                                # pure-Rust math — the trust core (zero deps, no_std)
│  ├─ src/{math,oracle,market,position,errors}.rs
│  └─ tests/                            # invariant + 300k-iteration fuzz tests
├─ programs/short-perp/                 # Anchor program wrapping `core` (scaffold; build locally)
└─ app/                                 # frontend (placeholder)
```

The critical financial logic lives in **`core`** and is deliberately isolated:
zero third-party dependencies, `no_std`, and identical whether it runs on-chain
(inside the Anchor program) or off-chain (keepers, matchers, backtests).

## Run the tests (no toolchain beyond Rust needed)

```bash
cargo test           # builds + runs the core suite, incl. 300k fuzz iterations
```

These prove the two properties the whole design rests on, across randomized
inputs:

- **CONSERVATION** — `short_payout + long_payout + protocol_fee == total escrow`
- **NO BAD DEBT** — every payout is non-negative for every possible price

Building the on-chain program needs the Solana + Anchor toolchain — see
[`programs/short-perp/README.md`](programs/short-perp/README.md).

## Status & roadmap

| Phase | Scope | State |
|---|---|---|
| **0** | Fully-collateralized matched shorts, graduated tokens only, TVL+age gate, clamped-TWAP oracle, liquidity-gated OI cap | `core` implemented & tested; on-chain program scaffolded |
| 1 | ≤2x isolated-margin leverage, per-collateral insurance fund, permissionless liquidators | design only |
| 2 | Oracle-anchored pooled-LP counterparty with strict NAV caps + ADL | design only |
| 3 | Pre-graduation bonding-curve tokens (max risk — maybe never) | design only |

## Before this touches real money

1. **Verify on-chain layouts** — confirm the pump.fun `BondingCurve` and
   PumpSwap pool byte layouts against a live IDL (a wrong offset silently breaks
   the oracle).
2. **Calibrate the safety thresholds** — backtest the OI cap / clamp against
   historical PumpSwap pools: attacker cost to move the mark vs. profit available.
3. **Audit + adversarial testing** — simulate JELLY/POPCAT-style attacks on
   devnet before any capital.
4. **Legal counsel** — this is a derivatives venue; the report's §7 is a posture,
   not legal advice.

## Disclaimer

Research and engineering scaffold. Not audited. Not financial or legal advice.
Nothing here is deployed. Shorting memecoins is extremely high risk.
