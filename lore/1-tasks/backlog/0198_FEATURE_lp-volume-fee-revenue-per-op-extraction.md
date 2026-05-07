---
id: '0198'
title: 'LP volume + fee_revenue: per-op extraction with USD-denominated values'
type: FEATURE
status: backlog
related_adr: ['0043']
related_tasks: ['0194', '0195']
tags:
  [
    priority-medium,
    effort-large,
    layer-indexer,
    layer-enrichment,
    blocked-on-oracle,
  ]
milestone: 2
links: []
history:
  - date: '2026-05-07'
    status: backlog
    who: karolkow
    note: 'Spawned from 0194 — sub-block 1d (snapshot-delta volume) pulled entirely. Two correctness problems made the snapshot-delta approach unfit for shipping: (a) reserve delta nets opposite swaps inside one ledger (gross volume lost), (b) volume in asset_a units without USD reference is a weak metric. Proper fix needs per-op extraction + price oracle, both out of 0194 scope.'
---

# LP volume + fee_revenue: per-op extraction with USD-denominated values

## Summary

Populate `liquidity_pool_snapshots.volume` and `liquidity_pool_snapshots.fee_revenue` correctly:

1. **Per-op extraction** — sum `claimedOffers[].amount_sold` per pool per ledger from PathPayment XDR effects, instead of `ABS(reserve_a_post − reserve_a_pre)`. Snapshot-delta nets opposite swaps inside one ledger (gross volume lost) and ignores deposit/withdraw distinction.
2. **USD denomination** — multiply by per-asset USD price (Reflector / StellarExpert oracle) so `volume` and `fee_revenue` are dollar-denominated, comparable across pools.

## Context

Task 0194 originally included sub-block 1d which computed `volume = ABS(reserve_a_post − reserve_a_pre)` per ledger and `fee_revenue = volume × fee_bps / 10000`. Two flaws:

**Flaw 1 — netting cancels opposite swaps.** Within one ~5s ledger a pool can see swap+50 and swap-30 (opposite directions). Reserve delta = 20. Gross volume = 80. The exchange convention is gross. Snapshot-delta loses the cancelled-out half.

**Flaw 2 — single-leg unit, no USD.** Volume in asset_a (e.g. XLM) is unitless without a price reference. Comparing pool XLM/USDC to pool XLM/AQUA — the XLM-leg numbers look comparable, the USD reality may differ 10×. Block-explorer UI showing "volume: 1.5M" in raw XLM is confusing; users expect USD.

These flaws compound. After review (2026-05-07 session) the approach was deemed unfit for the MVP — better to ship NULLs than wrong numbers — and the sub-block was pulled from 0194 entirely.

## Implementation Plan

### Phase 1 — Per-op extraction

- **XDR parser**: extract `claimedOffers[]` from PathPayment ops (types 2 + 13). Each claimed offer that touched a pool has `amount_sold` (in pool's asset_a units). Sum per (pool_id, ledger_sequence).
- **New staging row** type `LpSwapAggregate { pool_id, ledger_sequence, gross_volume_a }` populated from the op walk.
- **Persist write step** (in `upsert_pools_and_snapshots` after the snapshot INSERT): UPDATE `liquidity_pool_snapshots.volume` from `LpSwapAggregate` rows for matching (pool, ledger). Pools without a swap op in this ledger keep `volume = NULL`.
- Per-op extraction also handles **LiquidityPoolDeposit / LiquidityPoolWithdraw** — those ops are NOT volume; they're capital flow. The extractor only sums claimed-offer rows from PathPayment ops, so deposit/withdraw is naturally excluded without a NOT EXISTS filter.

### Phase 2 — USD denomination

- **Price oracle**: reuse 0195 §2b infrastructure (Reflector primary + StellarExpert fallback + USDC/USDT pegged direct). Per-asset USD price keyed by (code, issuer).
- **Two storage choices** (decide in design):
  1. Store `volume` in asset_a units (current schema) + add `volume_usd NUMERIC(28,7)` column written by Lambda 2 after oracle lookup. Keeps indexer write-path on-chain only.
  2. Convert in indexer at write-time using a cached price table populated by Lambda 2. Adds runtime price dependency to the indexer.
  - **Lean toward 1** per ADR 0043 (off-chain → Lambda 2). Matches the 0195 LP TVL pattern.
- **fee_revenue_usd = volume_usd × fee_bps / 10000** computed alongside.

### Phase 3 — Soroban DEX adapters (Phase 2 of 0194's original scope)

- Soroswap and Phoenix have their own pool contracts emitting `swap` events with explicit per-swap amounts. No PathPayment involvement; per-op aggregation needs adapters that read those event shapes.
- Out of Phase 1; gates on Phase 1 landing.

## Acceptance Criteria

- [ ] PathPayment ops emit `LpSwapAggregate` rows during XDR parse (unit + integration tests with mocked claimed_offers)
- [ ] `liquidity_pool_snapshots.volume` populated from per-op aggregation, NULL only on no-swap ledgers
- [ ] Sample query on production-region pool shows volume reflects gross swap activity (verifiable against Horizon `/liquidity_pools/{id}/trades`)
- [ ] `volume_usd` (or equivalent) column populated by Lambda 2 oracle path; NULL fallback when oracle has no price for the asset
- [ ] `fee_revenue_usd` derived from `volume_usd × fee_bps / 10000`
- [ ] Docs updated: `docs/architecture/database-schema/database-schema-overview.md` (new column attribution + populated-by note) + `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` (per-op step description) + endpoint-queries comments + ADR 0043 per-kind matrix amendment if `volume_usd` column added
- [ ] API types regenerated if any DTO surfaces these fields
- [ ] Migration if `volume_usd` / `fee_revenue_usd` columns added

## Notes

- **Blocked on 0195 §2b** for the USD denomination phase. Phase 1 (per-op extraction in asset_a units) can land independently and gives correct gross volume even before the oracle is wired.
- **MVP option**: ship Phase 1 first, leave `volume` in asset_a units with a clear UI disclaimer. Phase 2 (USD) follows once 0195 §2b is in production. This preserves "correctness" (no opposite-swap netting) while deferring "completeness" (USD).
- **Schema cost**: adding `volume_usd` + `fee_revenue_usd` columns adds 16 bytes per snapshot row (NUMERIC(28,7) ≈ 8B each). At ~17k ledgers/day × N pools, the storage delta is small compared to the existing `reserve_a/b/total_shares` triple.
