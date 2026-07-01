# app — frontend (placeholder)

Not built yet. Phase-0 plan (see `../docs/feasibility-and-architecture.md` §6):

- React + Solana wallet adapter.
- Subscribes to a Yellowstone gRPC / Geyser indexer (Helius LaserStream, Triton,
  QuickNode, or Chainstack) for sub-100ms streaming of pool reserves, graduation
  events, and position state.
- **Restricted universe by design:** only shows graduated tokens that pass the
  pool-TVL and age gates, with prominent risk disclosures and jurisdiction
  gating.
- Two flows: open a short (posts `short_collateral`), or take the long/yield
  side (posts `long_collateral`); a settle/claim view driven by the oracle mark.
