---
id: '0125'
title: 'LP analytics: price oracle, TVL, volume, and fee revenue'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0052', '0077']
tags: [priority-low, effort-large, layer-indexer, layer-backend, audit-gap]
milestone: 2
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-04-10'
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from pipeline audit — LP tvl/volume/fee_revenue columns exist but are likely always NULL without external pricing.'
---

# LP analytics: price oracle, TVL, volume, and fee revenue

## Summary

`liquidity_pools.tvl`, `liquidity_pool_snapshots.tvl`, `.volume`, and `.fee_revenue`
columns exist in the schema but are effectively always NULL. Computing real values requires:

- **TVL**: USD price oracle to convert reserves to dollar value.
- **Volume**: Tracking individual swap operations per pool per time window.
- **Fee revenue**: Derived from volume \* fee_bps.

## Important: Classic AMM vs Soroban DEX Pools

This task covers **two fundamentally different pool types** that require separate
implementation paths:

- **Classic AMM pools**: Native `LiquidityPoolEntry` ledger entries. Reserves, total
  shares, and fee_bps are directly available from XDR (`LiquidityPoolConstantProduct`).
  Swaps are `PathPayment` operations, not Soroban events. Fee is fixed at pool creation.
- **Soroban DEX pools** (Soroswap, Phoenix, etc.): Smart contracts storing state in
  `ContractData` entries. Reserves require contract-specific parsing. Swap events are
  **not standardized** across DEXes — each has its own event format. Fees may be dynamic.

The current codebase only extracts classic AMM pools via `ledger_entry_changes.rs`.
Soroban DEX pool support requires per-DEX event parsing adapters.

## Implementation

### Classic AMM Pools (Phase 1)

1. **Price oracle**: Integrate external price feed (CoinGecko, StellarExpert API, or
   Horizon aggregation endpoint) to get USD prices for pool assets.
2. **TVL computation**: reserve_a _ price_a + reserve_b _ price_b, updated per snapshot.
3. **Volume tracking**: Identify `PathPayment` operations that modify pool reserves.
4. **Fee revenue**: volume \* (fee_bps / 10000). Fee is immutable (set at pool creation).
5. Decide: inline during indexing or separate scheduled enrichment job?

### Soroban DEX Pools (Phase 2 — separate task recommended)

1. Build per-DEX event parsing adapters (Soroswap, Phoenix, etc.).
2. Extract reserves from contract storage (`ContractData` entries).
3. Track swaps via contract-specific event patterns in `soroban_events`.
4. Handle dynamic fee structures per DEX.

## Acceptance Criteria

- [ ] `liquidity_pools.tvl` populated with USD-denominated value
- [ ] `liquidity_pool_snapshots.tvl` populated per snapshot
- [ ] `liquidity_pool_snapshots.volume` populated from trade activity
- [ ] `liquidity_pool_snapshots.fee_revenue` computed from volume
- [ ] Chart endpoint (`GET /liquidity-pools/:id/chart`) returns non-null time series
