# Shorting pump.fun Memecoins On-Chain: Feasibility & Build Report

*Prepared for: founder exploring an on-chain platform to short pump.fun memecoins live*
*Date: 2026-07-01*

> **Research provenance / confidence note.** This report was produced by a fan-out research
> workflow (5 parallel researchers → adversarial fact-checkers → synthesis). Independently
> **verified** during the run: the JELLY exploit details, PumpSwap's program ID and constant-product
> mechanics, and the claim that no mainstream venue shorts arbitrary launchpad tokens. **Flagged as
> not-yet-verified and needing confirmation before code:** exact on-chain byte layouts / IDLs,
> empirical oracle-safety thresholds, competitor mainnet status (Percolator/Pumpr), and *all* legal
> claims (Section 7 is a posture, not legal advice — retain crypto-derivatives counsel). Treat
> program IDs and numeric parameters as "verify against mainnet" rather than gospel.

---

## 1. TL;DR

**It is feasible, but not in the form most people imagine, and the naive version is a guaranteed loss.** You cannot "short a pump.fun coin the moment it launches" in any safe, capital-efficient way — the token has no borrowable inventory, no manipulation-resistant price feed, and a single thin AMM pool that anyone with a few thousand dollars can move hundreds of percent. Every mature venue that offers memecoin shorting (Drift, Jupiter, Adrena, Hyperliquid) refuses arbitrary launchpad tokens for exactly this reason, and the graveyard of thin-market perp exploits (Hyperliquid's JELLY, ~$13.5M peak vault loss; POPCAT, ~$63M longs liquidated) is the empirical proof of what happens when you don't. The honest verdict: **build a fully-collateralized, capped-payout synthetic short on *graduated* tokens only, gated hard by liquidity and age, with a manipulation-hardened composite oracle.** That product is provably safe (no bad debt, no socialized losses), genuinely novel, and shippable. The dream of one-click shorting a 3-minute-old bonding-curve coin is the hardest possible version of this problem, sits at maximum manipulation risk, and should be the *last* thing you build, if ever. A permissionless-perp-launchpad category is already emerging (Percolator, Pumpr), but it is mostly devnet/beta and inherits every risk the incumbents deliberately avoid — that's your competition and your cautionary tale simultaneously.

---

## 2. How pump.fun works (the parts that matter for shorting)

A pump.fun token lives in exactly two states, and each has radically different implications for whether — and how — you can short it.

### State A: Pre-graduation (on the bonding curve)
- Token is created on the pump.fun program (`6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`).
- A `BondingCurve` PDA (seeds `["bonding-curve", mint]`) holds `virtual_token_reserves`, `virtual_sol_reserves`, `real_token_reserves`, `real_sol_reserves`, `token_total_supply`, a `complete` flag, and `creator`.
- **Price is purely mechanical:** `price = virtual_sol_reserves / virtual_token_reserves`. There is no order book, no external market, no LPs — just a deterministic curve. The "market cap" is a function of how much SOL has been deposited into the curve.
- This is the **most manipulable price surface in crypto.** A single buyer's flow *is* the price. There is no external anchor to compare against.

### State B: Post-graduation (PumpSwap AMM)
- When the curve fills (`complete` flips true / `real_token_reserves` exhausts), liquidity migrates to PumpSwap, pump.fun's in-house AMM (`pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`).
- PumpSwap is a **constant-product (x·y=k, Uniswap-v2 style)** DEX. Pool accounts expose `pool_base_token_account` and `pool_quote_token_account` as the reserve holders. Fee is 0.25% (0.20% LP / 0.05% protocol).
- PumpSwap is enormous in aggregate (record ~$1.28B/24h in Jan 2026; ~$16B *monthly* volume by Feb 2026) — but that volume is **spread across a long tail of individual pools**, most of which are shallow. A graduated token might have a $30k pool or a $2M pool; the aggregate number tells you nothing about any single token's depth.

### The liquidity reality that frames everything
- **There is no borrowable spot inventory.** No lending market lists a 3-day-old memecoin as borrowable. Kamino and MarginFi main pools are curated (stables/LSTs/wBTC/JUP/RWAs). This kills classic borrow-and-sell shorting for fresh tokens outright.
- **Both venues are single-pool, thin-liquidity, and trivially manipulable** — the bonding curve by design, and most PumpSwap pools by circumstance.
- **Note:** pump.fun has been *reported* to be considering native perps/lending, but as of mid-2026 has launched **no** derivatives product — PumpSwap is spot-only. So there is no incumbent "official" short primitive to compete against, but also no in-house infrastructure to lean on.

Every architectural decision downstream flows from one sentence: **the underlying is manipulable and there is no borrow market.** That pushes you away from physical shorts and toward *synthetic, fully-collateralized, hard-capped* exposure.

---

## 3. What "shorting" can even mean here — the menu of mechanisms

There are five ways to express bearish exposure. Only some survive contact with an illiquid launchpad token.

| Mechanism | How it works | Viable for launchpad tokens? |
|---|---|---|
| **Borrow-and-sell (physical short)** | Borrow the token from a lender, sell spot, rebuy later. | **No.** No lending inventory exists for fresh memecoins. A loan on an asset that can go to zero in hours is uninvestable. (MarginFi's *The Arena* and Save's permissionless isolated pools *technically* allow this for mid-tail tokens, but require a seeded pool, lenders, and an oracle — none of which a new launch has.) |
| **Orderbook perp (Drift-style DLOB)** | Central-limit book + JIT auctions; pro market makers quote two-sided. | **No for cold start.** Drift fills long-tail *majors* only because whitelisted MMs quote on top of the AMM. No MM will quote two-sided on a random memecoin; the book would be empty. Good long-term target, wrong for launch. |
| **Pooled-LP / vAMM perp (Jupiter JLP, GMX-style)** | Trade at oracle price against a shared LP pool; LPs are the counterparty. | **No for v1; maybe later, graduated-only.** This is the *exact* JELLY attack surface: a shared pool becomes the forced counterparty to a self-manipulated position. Only safe with a manipulation-resistant oracle AND per-market OI as a tiny fraction of pool NAV. Jupiter itself refuses all memecoins for this reason. |
| **Options (fully-collateralized puts)** | Writer escrows 100% of max payout; buyer pays premium. | **Partially — niche only.** Full collateralization (Stryke/Hegic/Premia model) is capital-safe by construction. But pricing implied volatility on a 3-day-old token is intractable, and there's no liquidity. Useful as a specialty product, not the core. |
| **Fully-collateralized P2P / matched synthetic short** | Two users each escrow collateral into a vault keyed to a mint; PnL is capped so neither side can lose more than escrow. | **YES — this is the viable v1.** No leverage, no bad debt, no shared pool to drain, no ADL, and the JELLY class of attack *cannot socialize losses*. Tradeoff: capital-inefficient and you must source a counterparty. |

**The winner for v1 is the fully-collateralized matched synthetic short**, because it is the smallest thing that is *provably* safe. Both legs are pre-funded, PnL is bounded, and the worst manipulation outcome is that the two matched parties trade collateral between themselves — the protocol never eats a shortfall.

---

## 4. Why this is hard — the five failure modes

### 4.1 Oracle manipulation (the primary killer)
A launchpad token's "price" is a single thin AMM read or a bonding-curve ratio. Both are movable by a single actor. The lesson from **JELLY (Mar 26, 2025)** is subtle and important: Hyperliquid *did* have an external oracle — a stake-weighted median of seven CEXs. It failed anyway because JELLY was thinly listed *everywhere*, so the attacker moved the aggregated median by manipulating spot across all venues at once. This is the **"median trap": an external anchor is worthless if the underlying is thin on every source you aggregate.** For a pump.fun token, there *is* no CEX source at all — the only price is the very pool you'd be marking against. TWAP helps against single-block spikes but **does not make a shallow market safe**; sustained multi-block pressure defeats it when the pool is thin.

### 4.2 Liquidity / counterparty risk
Someone must take the long side of every short. Memecoin flow is structurally *one-sided* — everyone wants to short; almost no one wants to be the long counterparty. In a pooled model, that "someone" is your LP vault, which becomes the forced counterparty and the thing an attacker drains (JELLY: ~$13.5M peak unrealized loss, ~27% of HLP NAV; POPCAT, Nov 2025: ~$63M longs liquidated, ~$5M HLP loss). In a P2P model, the problem *disappears by construction* — no short opens without a pre-funded long.

### 4.3 Unbounded loss / convexity
A short has **capped gain (token → 0) and theoretically unbounded loss (token → ∞).** Memecoins routinely do 5–50x in hours. A leveraged short on a coin that 10x's is a catastrophic, fast liquidation. This is why v1 must be **fully collateralized (0 leverage / 100% initial margin)** with a *capped payout*: the short's max loss is explicitly bounded to its escrow, and the long's max gain is bounded to the short's escrow. You trade away leverage to eliminate blow-up risk.

### 4.4 One-sided flow / funding
In a perp model, when OI is ~95% short, standard funding must go steeply negative (shorts pay longs) to price the imbalance and lure counterparties — but even aggressive funding won't conjure long-side capital for a coin everyone believes is going to zero. You'd need OI caps that simply forbid shorts from exceeding available long capacity, plus a hard-capped backstop vault. **In the fully-collateralized P2P v1, funding imbalance is a non-issue** — matching enforces balance.

### 4.5 Manipulation economics
The attack is cheap and the payoff is large precisely because the pool is thin. Your only defense is to **make the attack more expensive than the prize**: liquidity-gated OI caps (total short notional per token ≤ a small fraction of pool depth), truncated/clamped oracles (cap per-update price moves, Uniswap-v4 style), and volatility circuit breakers that freeze new positions and pause liquidations during abnormal moves. Note: the "5% of circulating mcap" figure sometimes cited as Hyperliquid's fix is not a published parameter — Hyperliquid revised OI caps to account for real market cap *and order-book depth*, without disclosing a specific percentage. Treat any such number as a tunable you must calibrate against historical PumpSwap manipulation-cost data, not a magic constant.

---

## 5. Existing venues & competitors — the gap

### What exists today (mid-2026)
- **Solana perp DEXes** — Drift (largest, orderbook+JIT+vAMM, lists only large-cap memes: 1MBONK-PERP, 1MPEPE-PERP, WIF-PERP in a "Speculative" tier, ~5x vs 20x for majors, Pyth+Switchboard oracles, 10% circuit-breaker band). Jupiter Perps (JLP pool; **SOL/ETH/wBTC only, zero memecoins**). Adrena (adds **BONK/USDC** — one large-cap meme). Flash Trade, Bullet (ex-Zeta), GooseFX — majors-focused, no long-tail memes.
- **Solana lending (borrow-to-short)** — Kamino/MarginFi main pools curated. **MarginFi's The Arena** and **Save's permissionless isolated pools** allow permissionless long/short on arbitrary tokens *in principle*, but need a seeded pool + lenders + oracle. (Note: The Arena's live mid-2026 status is ambiguous — beta ended Aug 2025, geoblocked, revamp pending.)
- **Hyperliquid + HIP-3** — Core book lists large-cap Solana meme perps via CEX-median oracle (structurally excludes non-CEX tokens). HIP-3 builder-deployed perps (mainnet Oct 2025) *could* host anything, but require ~500k HYPE (~$25M) stake + up-to-100% slashing + manipulation-resistant oracle — used for RWAs/equities, not memes.
- **The emerging permissionless-perp-launchpad category (your real competition):**
  - **Percolator** — open-source, attributed to Solana co-founder Anatoly Yakovenko; "deploy a perp market for *any* Solana token in one click," long *and* short, coin-margined, sourcing price from DEX pools (PumpSwap/Raydium/Meteora) + Jupiter + DexScreener. **Devnet/closed beta, Q3 2026 audited-mainnet target — not yet live for public trading.**
  - **Pumpr** ("PumpFun for Perps") — permissionless, "launch a perp market on anything in 30 seconds, long memes short KOLs." Claims a 2026 mainnet; independent live-volume confirmation is thin.
  - **Perps.fun / Kinetiq Launch** — HIP-3 launchpads; Perps.fun "coming soon" and explicitly *not* permissionless (retains approval control).

### The gap you'd fill
Nobody has shipped a **safe, live, production** venue for shorting arbitrary graduated pump.fun tokens. The incumbents refuse the assets; the launchpad-perp challengers chase permissionless-everything but are pre-mainnet and inherit exactly the oracle/liquidity risks that produced JELLY. **Your differentiated position: not "short anything instantly," but "the venue where shorting a real graduated memecoin is actually safe" — solvency-guaranteed by full collateralization and hard liquidity gating, shipped and audited while competitors are still in devnet.** Safety *is* the product wedge here, not a constraint on it.

---

## 6. Recommended architecture

### The one model
**Fully-collateralized, capped-payout, matched synthetic short on graduated PumpSwap tokens only**, evolving to oracle-anchored low-leverage perps later. Two participants (short-seeker + counterparty/yield-seeker) each escrow into an Anchor-managed vault keyed to a mint. PnL = `notional × (entryPrice − markPrice) / entryPrice`, **bounded so each side's max loss ≤ its escrow**. No shared LP pool, no bad debt, no ADL, no funding-imbalance risk.

### Oracle design (the heart of it)
Never mark to a single spot read. Build a **composite, manipulation-hardened index**:
- **Token/SOL price:** from PumpSwap pool reserves (`pool_quote_token_account / pool_base_token_account`); cross-check Raydium if a parallel pool exists.
- **SOL/USD leg:** Pyth + Chainlink with median/deviation fallback (mirror Jupiter's multi-source pattern for the *stable* leg — that leg is safe to trust).
- **TWAP:** on-chain time-weighted average (5–15 min window, per-slot samples via crank), *not* instantaneous price.
- **Truncated/clamped oracle:** cap per-update price move (e.g. max ±X% per tick) so no single actor can drag the TWAP arbitrarily (Uniswap-v4 style).
- **Liquidity-gated OI cap:** total open short notional per token tied to live on-chain pool depth — the single most important safety lever. Calibrate against real PumpSwap manipulation-cost data.
- **Circuit breakers:** freeze new positions + pause liquidations on abnormal price/volatility/staleness.

### Margin / liquidation / insurance
- **v1: fully collateralized (0 leverage, 100% initial margin), isolated margin per position/per mint.** Isolation is mandatory — one memecoin going to zero must never touch a user's other collateral or the protocol.
- **Liquidation in v1 is trivial and bad-debt-free:** when mark crosses the point where a side's escrow is exhausted, a permissionless crank settles the vault at the capped payout — no auction, no insurance draw.
- **Insurance fund + ADL: only introduced in later leveraged phases**, per-collateral, funded by fees, stakeable (Drift model). ADL strictly as last resort. **Never route a forced-closed manipulated position into a passive LP vault you can't exit** (the JELLY mistake).
- **Collateral:** USDC primary (sane PnL accounting under extreme vol); SOL accepted with a haircut.

### Solana program stack
- **Anchor** (as both pump.fun and Drift use).
- Core programs: `Exchange` (positions, config, collateral vaults), `Oracle` (TWAP accumulator + circuit breakers), `Vault` (escrow/backstop), `Liquidator` entrypoints.
- **Accounts:** PDA-per-market `["market", mint]`; PDA-per-position `["position", user, mint, subaccount]`; isolated collateral vault PDA per position. TWAP accumulator + last-sample-slot stored in the market account.
- **Reading the underlying:** deserialize the `BondingCurve` account (program `6EF8rr…`) and PumpSwap pool account (program `pAMMBay6…`) directly. Detect graduation via `complete` / `real_token_reserves == 0`, then switch price source to the PumpSwap pool. **Verify the live pool byte-layout against a current on-chain IDL before writing deserialization — a wrong offset silently breaks the oracle.**
- **Keepers/crank:** permissionless off-chain bots (Rust/TS), incentivized by fees/rebates (Drift model): push TWAP samples, run settlements/liquidations.
- **Matching:** off-chain matching service to pair short-seekers with counterparties; **custody and settlement stay on-chain** (Drift's off-chain-match / on-chain-settle pattern).
- **Indexing/frontend:** Yellowstone gRPC / Geyser (Helius LaserStream, Triton, QuickNode, Chainstack) for sub-100ms account streaming — essential for tracking reserve/graduation changes and keeper latency. Frontend: React + wallet adapter subscribing to the indexer.

### Concrete phased MVP
- **Phase 0 (ship first):** Graduated PumpSwap tokens **only**, with `≥ $100k pool TVL` and `≥ 24h age` gates. Fully-collateralized matched synthetic shorts, capped payout, hard per-token OI cap sized to pool depth, clamped TWAP index. Permissionless settlement crank. **No leverage, no insurance fund, no ADL, no funding risk.** This is the smallest provably-safe thing that works.
- **Phase 1:** SOL/USDC collateral choice; isolated-margin **≤2x** leverage on the same universe; per-collateral insurance fund; permissionless liquidators; funding with piecewise-linear clamp.
- **Phase 2:** oracle-anchored vAMM/pooled-LP counterparty (Jupiter-style) with strict per-market NAV-exposure caps + circuit breakers; ADL as last resort.
- **Phase 3 (hardest, maybe never):** pre-graduation bonding-curve tokens — only with truncated oracle, tiny notional caps, heavy funding carry. **This is where JELLY-class risk is maximal. Do not lead with it.**

---

## 7. Legal / regulatory reality

*This section is a product/engineering posture, not legal advice — retain crypto-derivatives counsel before launch. The research did not include verified legal-jurisdiction analysis, so treat the below as flags to validate, not conclusions.*

- **You are building a derivatives venue.** A synthetic short with capped payout is, functionally, a derivative/swap. In the US this implicates CFTC (and potentially SEC) jurisdiction; offering leveraged retail derivatives is heavily restricted. Perps on memecoins are drawing explicit "perps-as-gambling" regulatory scrutiny (referenced in 2026 coverage).
- **Honest posture:** the leveraged phases (1–3) materially increase regulatory exposure vs the fully-collateralized, capped-payout v1 (which resembles a bilateral prediction/escrow more than a leveraged derivative — though this distinction must be confirmed with counsel, not assumed).
- **Mitigations to evaluate:** geofencing / jurisdiction gating at the frontend and RPC layer; no US persons; a decentralized, non-custodial protocol posture (contracts hold collateral, not the company); clear disclaimers; publishing that the protocol is non-custodial and permissionlessly operable; avoiding acting as a matched-book intermediary yourself. Note that frontend geofencing is weak protection if the contracts are openly callable — the legal reality of "the protocol is just code" is contested and jurisdiction-specific.
- **Token/marketing risk:** marketing "short any memecoin with leverage" to retail is the highest-scrutiny framing. "Solvency-guaranteed, fully-collateralized bearish exposure on liquid tokens" is a defensibly narrower posture.

---

## 8. Concrete next steps

### Ordered buildable checklist (MVP = Phase 0)
1. **Confirm on-chain layouts.** Pull the *current* live IDLs for pump.fun (`6EF8rr…`) and PumpSwap (`pAMMBay6…`); verify pool/bonding-curve byte offsets against real mainnet accounts. Confirm whether a SOL/USDC quote-pairing option changes the quote token (affects your token/USD path).
2. **Build the oracle module first, in isolation.** TWAP accumulator + clamp + circuit breakers + liquidity-gated OI cap. Backtest against historical PumpSwap pools: simulate manipulation attacks and measure the cost to move your index vs the profit an attacker could extract. This is your core IP and your survival.
3. **Write the vault/escrow + matched-short settlement program** (Anchor). Fully-collateralized, capped payout, isolated per-mint. Unit + property tests asserting *no path produces bad debt*.
4. **Build the permissionless settlement crank** and the off-chain matching service.
5. **Indexing layer** (Yellowstone gRPC) tracking graduation events, pool reserves, position state.
6. **Frontend** (React + wallet adapter) — restricted universe (graduated, ≥$100k TVL, ≥24h old), clear risk disclosures, geofencing.
7. **Audit + adversarial testing** before any real capital. Simulate JELLY/POPCAT-style attacks against your own oracle and caps on devnet.
8. **Only then** consider Phase 1 (leverage).

### Key decisions the founder must make (with recommendation)
| Decision | Options | Recommendation |
|---|---|---|
| Short mechanism for v1 | Physical / perp / options / **P2P synthetic** | **Fully-collateralized matched synthetic short.** Only provably-safe cold-start option. |
| Token universe at launch | Pre-graduation / **graduated-only** | **Graduated-only, hard-gated by TVL + age.** Pre-graduation is Phase 3 or never. |
| Leverage at launch | 0x / low / high | **0x (fully collateralized).** Leverage is Phase 1+, ≤2x, only after the oracle is battle-tested. |
| Counterparty model | Shared LP pool / **matched P2P** | **Matched P2P.** Eliminates the JELLY attack surface entirely. |
| Oracle | Single spot / TWAP / **clamped composite** | **Clamped composite TWAP + liquidity-gated OI cap.** Non-negotiable. |
| Collateral | SOL / **USDC** / both | **USDC primary**, SOL with haircut later. |
| Positioning | "Short anything instant" / **"safe shorts on liquid memes"** | **The safe framing.** It's your wedge *and* your regulatory shield. |
| Speed vs safety | Race Percolator/Pumpr to permissionless / **ship safe first** | **Ship the safe, audited, live product** while they're in devnet. |

---

## 9. Open questions worth validating before writing code

1. **Live PumpSwap/bonding-curve byte layout** — confirm against a current IDL/on-chain account; a wrong offset breaks the oracle silently. Also confirm whether the SOL/USDC pairing option (reported ~mid-2026) changes which quote token pools hold.
2. **Empirical safety thresholds** — what minimum pool TVL + token age *actually* make a graduated token safe to mark? Needs historical manipulation-cost analysis on real PumpSwap pools, not a heuristic OI %.
3. **Counterparty bootstrapping** — will yield-seekers voluntarily take the long side of memecoin shorts at any funding rate? If not, do you need a hard-capped, hedged protocol backstop vault — and can it be sized so it never itself becomes a JELLY-style forced counterparty?
4. **Regulatory posture in target jurisdictions** — is a capped-payout, fully-collateralized synthetic legally distinct from a leveraged derivative? Does "perps-as-gambling" scrutiny reach the v1 design? (Requires counsel.)
5. **Competitor state** — is Percolator's Q3 2026 mainnet real and audited? Does Pumpr have verifiable live volume, or is it vaporware? Their live/failure data is your cheapest source of truth on what breaks.
6. **The Arena / Save live status** — can a mid-tail token actually be borrow-shorted there today? If so, that's a partial substitute you're competing against for the *mid*-tail (not the fresh-launch) segment.
7. **Drift's real listing gate** — is "permissionless market listing" genuinely open, or is a Pyth/Switchboard feed the de facto gate? Determines whether Drift could suddenly enter your niche.

---

**Bottom line for the founder:** the safe, novel, shippable product is *not* the one in the pitch. It's a fully-collateralized, capped-payout short on graduated, liquid memecoins, defended by a manipulation-hardened composite oracle and hard liquidity gating — launched and audited while the permissionless-everything competitors are still in devnet learning the JELLY lesson the expensive way. Build that. Earn the right to loosen the gates later.
