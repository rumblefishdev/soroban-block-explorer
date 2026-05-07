---
id: '0199'
title: 'LP analytics: TVL + volume + fee_revenue (per-op extraction + USD)'
type: FEATURE
status: backlog
related_adr: ['0027', '0031', '0043']
related_tasks: ['0125', '0194', '0195']
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
    note: 'Spawned from 0194 §1d (snapshot-delta volume) — flaws: opposite-swap netting + asset_a units. Absorbed 0195 §2b (LP TVL); 0125 superseded. Spec: zero schema delta — `tvl/volume/fee_revenue` already exist (migration 0006), stay USD-denominated; `gross_volume_a` carried in SQS message body. All three columns owned by Lambda 2.'
---

# LP analytics: TVL + volume + fee_revenue

## Summary

Populate `liquidity_pool_snapshots.{tvl, volume, fee_revenue}` correctly. All three are existing schema columns (migration 0006); this task makes them **USD-denominated** per ADR 0027's forward-looking intent. Zero schema delta.

Two concerns:

1. **Per-op volume** — gross volume from PathPayment `claimedOffers[].amount_sold`, not snapshot-delta `ABS(reserve_post − reserve_pre)`. Snapshot-delta nets opposite swaps inside one ledger.
2. **USD denomination** — multiply on-chain values (reserves for TVL, gross_volume_a for volume) by per-asset USD price from team-built price API (Oskar). All three columns end as USD.

This task **consumes** the price API; it does not build it.

Subsumes 0125 (original LP analytics) and absorbs 0195 §2b (LP TVL).

## Per-lambda ownership

USD denomination → off-chain prices → ADR 0043 forces all three column writes to Lambda 2. Indexer contributes on-chain inputs only (reserves already written; `gross_volume_a` passed in SQS message body, no staging table).

| Column        | Inputs                                                                      | Written by   | Formula                                           |
| ------------- | --------------------------------------------------------------------------- | ------------ | ------------------------------------------------- |
| `tvl`         | `reserve_a, reserve_b` (Indexer, on-chain) × USD prices (oracle, off-chain) | **Lambda 2** | `tvl = reserve_a × price_a + reserve_b × price_b` |
| `volume`      | `gross_volume_a` (Indexer, in SQS msg) × `price_a` (oracle)                 | **Lambda 2** | `volume = gross_volume_a × price_a`               |
| `fee_revenue` | `volume` × `fee_bps` (Indexer, `liquidity_pools.fee_bps`, on-chain)         | **Lambda 2** | `fee_revenue = volume × fee_bps / 10000`          |

## Plan

### Phase 1 — Indexer-side extraction

- XDR parser: extract `claimedOffers[]` from PathPayment ops (types 2 + 13). Sum `amount_sold` per `(pool_id, ledger_sequence)`. Naturally excludes `LiquidityPoolDeposit` / `LiquidityPoolWithdraw` (capital flow, not volume — different op types, no claimed_offers).
- Producer hook in `enrichment_publish.rs`: insert-hook on each new `liquidity_pool_snapshots` row. Emit `EnrichmentMessage::LpAnalytics { pool_id: [u8; 32], snapshot_id: i64, gross_volume_a: Option<NumericString> }`. `None` for ledgers without swap activity on this pool. Exactly-once per snapshot insert.
- No DB write to `liquidity_pool_snapshots.volume` from indexer side. Phase 1 is parse + emit only.

### Phase 2 — Lambda 2 consumer

- New variant in `EnrichmentMessage` (defined above with `gross_volume_a` field).
- New module `crates/enrichment-shared/src/enrich_and_persist/lp_analytics.rs` exposing `enrich_pool_analytics(pool, msg, oracle)`.
- Worker reads `reserve_a, reserve_b, fee_bps` from DB (already populated by indexer), fetches USD prices from price API, computes:
  - `tvl = reserve_a × price_a + reserve_b × price_b`
  - `volume = gross_volume_a × price_a` (skip when `gross_volume_a IS NONE` → leave `volume = NULL`)
  - `fee_revenue = volume × fee_bps / 10000` (skip when volume NULL)
- UPDATE the snapshot row with all three values atomically.
- Sentinel (insert-hook → exactly-once → no dedup risk): permanent oracle fail writes `tvl = 0` + WARN log carrying pool_id, snapshot_id, per-leg oracle errors. `liquidity_pools.tvl` (latest, if column exists) NOT overwritten with sentinel `0`.
- Transient (price-API 5xx, network, rate limit) → `EnrichError::Transient` → SQS retry → DLQ.
- Backfill of pre-existing snapshots (NULL because they predate this task) → owned by 0196.

### Phase 3 — Soroban DEX adapters

Soroswap, Phoenix, etc. emit `swap` events with explicit per-swap amounts; no PathPayment. Per-DEX adapter scaffolding. Out of Phases 1+2; gates on Phase 1+2 landing.

## Acceptance Criteria

**Phase 1 (Indexer):**

- [ ] PathPayment ops yield `gross_volume_a` extraction during XDR parse (unit + integration tests with mocked claimed_offers).
- [ ] Insert-hook emits exactly one `LpAnalytics` SQS message per new `liquidity_pool_snapshots` row (test).
- [ ] `gross_volume_a = None` on no-swap ledgers; deposit/withdraw ops do not contribute to the sum.

**Phase 2 (Lambda 2):**

- [ ] `tvl`, `volume`, `fee_revenue` populated by Lambda 2 from `reserves + gross_volume_a + fee_bps + oracle prices`.
- [ ] Permanent oracle fail → `tvl = 0` + WARN log; transient → SQS retry.
- [ ] `liquidity_pools.tvl` (if column exists) not overwritten by sentinel `0`.
- [ ] Sample query: non-NULL `tvl` and `volume` on production-region pools with valid oracle data.
- [ ] Sample query: `volume / price_a ≈ Horizon /liquidity_pools/{id}/trades` gross volume (verifies extraction correctness end-to-end).
- [ ] `GET /liquidity-pools/:id/chart` returns non-null time series (originally 0125 AC).

**Phase 3:**

- [ ] At least Soroswap adapter scaffolding.

**Common:**

- [ ] Permanent / transient `EnrichError` mapping documented + unit-tested.
- [ ] Integration test (mock price API).
- [ ] CDK DepthAlarm thresholds reviewed.
- [ ] Docs: ADR 0043 per-kind matrix amendment + schema doc + indexing-pipeline doc + endpoint-queries comments.
- [ ] API types regenerated if any DTO surfaces these fields.
- [ ] 0196 backlog updated to capture backfill dedup ownership for the three columns.

## Notes

- **Required consultation with Oskar** — owns the price API. Phase 2 designed jointly: endpoint schema, freshness/cache contract, no-price-for-asset behavior, latency budget.
- **Blocked on price API.** Phases 1+2 ship atomically — Phase 1 alone produces no DB writes (SQS messages with no consumer until Phase 2 live).
- **USD rationale** — single comparable metric across mixed-asset pools; on-chain LP-graph traversal (peg-rooted hops) considered + rejected as too complex vs team-built price API.
