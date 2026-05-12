---
id: '0125'
title: 'LP analytics: price oracle, TVL, volume, and fee revenue'
type: FEATURE
status: superseded
related_adr: ['0027', '0031', '0043']
related_tasks: ['0052', '0077', '0191', '0194', '0195', '0199']
tags: [priority-low, effort-large, layer-indexer, layer-backend, audit-gap]
milestone: 1
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-04-10'
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from pipeline audit — LP tvl/volume/fee_revenue columns exist but are likely always NULL without external pricing.'
  - date: '2026-05-12'
    status: superseded
    who: stkrolikiewicz
    by: ['0199']
    note: >
      Replaced by 0199 (LP analytics: TVL + volume + fee_revenue), spawned
      2026-05-07 by karolkow. 0199 is a strict superset: same target
      columns, same Classic AMM + Soroban DEX phasing, same fee_revenue
      formula, AC `GET /liquidity-pools/:id/chart returns non-null time
      series` carried forward verbatim. 0199 also corrects the
      snapshot-delta volume flaw (opposite-swap netting) by extracting
      gross volume from `PathPayment.claimedOffers[].amount_sold`, swaps
      cron 5min for an insert-hook on `liquidity_pool_snapshots`
      (sharing the SQS stream with 0195 SEP-1/NFT enrichment built by
      0191 framework, completed 2026-05-06), and adds bidirectional
      swap normalization, per-column semantic atomicity, NULL sentinel,
      and Horizon ≤1% tolerance verification. No residual unique scope
      remained in 0125. Functional gap (NULL columns) is **not closed**
      — it is now tracked under 0199, currently blocked on the
      team-built price API (Oskar). Pre-existing snapshot backfill
      ownership delegated to 0196.
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

**Architecture decision (resolved by audit Section 9.3):** This MUST be a **scheduled
enrichment job** (EventBridge cron every 5 min), NOT inline during indexer ingestion.
Price oracle calls and trade aggregation add latency/failure modes to the critical path.

1. **Enrichment Worker Lambda** (shared with task 0124 or separate): triggered by
   EventBridge cron (every 5 min for TVL/volume, reuses 0124 Lambda or creates new one).
2. **Price oracle**: Integrate external price feed (CoinGecko, StellarExpert API, or
   Horizon aggregation endpoint) to get USD prices for pool assets.
3. **TVL computation**: reserve*a * price*a + reserve_b * price_b, updated per snapshot.
4. **Volume tracking**: Identify `PathPayment` operations that modify pool reserves.
5. **Fee revenue**: volume \* (fee_bps / 10000). Fee is immutable (set at pool creation).

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
- [ ] **Scheduled Enrichment Worker Lambda deployed** with EventBridge cron (every 5 min)
- [ ] Enrichment runs independently of indexer ingestion (no inline price lookups)
