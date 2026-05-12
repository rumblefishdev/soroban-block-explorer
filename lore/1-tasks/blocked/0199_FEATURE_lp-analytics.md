---
id: '0199'
title: 'LP analytics: TVL + volume + fee_revenue (per-op extraction + USD)'
type: FEATURE
status: blocked
related_adr: ['0027', '0031', '0043']
related_tasks: ['0125', '0191', '0194', '0195', '0197']
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
  - date: '2026-05-12'
    status: blocked
    who: stkrolikiewicz
    note: >
      Promoting `blocked-on-oracle` from tag to lifecycle status. Phases 1+2
      ship atomically and depend on the team-built price API (owned by Oskar)
      being ready for consumption — without it Lambda 2 cannot USD-denominate
      `tvl`, `volume`, `fee_revenue` and Phase 1 (indexer per-op extraction
      to SQS) produces no observable DB writes on its own. Reusable
      enrichment scaffolding from 0191 (completed 2026-05-06,
      `crates/enrichment-shared/`, `crates/enrichment-worker/`, EventBridge
      + SQS + DLQ) is in place; gating dependency is solely the price API.
      Move back to active once Oskar's API contract (endpoint shape,
      freshness, no-price-for-asset behavior, latency budget) is finalized.
  - date: '2026-05-12'
    status: blocked
    who: karolkow
    note: >
      Scope extended to include `assets.usd_price` +
      `assets.usd_price_updated_at` columns (Phase 2b). Both populated by
      the same price-API consumer per ADR 0043 (off-chain, list-endpoint
      → Lambda 2). Originally surfaced as 0191 / 0194 Future Work; rolled
      in here rather than spawned as a separate task because (a) shared
      price-API consumer + cache, (b) markets-list product trigger
      materialised, (c) single ADR 0043 allocation decision covers both
      LP analytics and asset prices. Surfaced during 0197 audit-prep
      punch-list review.
---

# LP analytics: TVL + volume + fee_revenue

## Summary

Populate `liquidity_pool_snapshots.{tvl, volume, fee_revenue}` correctly. All three are existing schema columns (migration 0006); this task makes them **USD-denominated** per ADR 0027's forward-looking intent. Zero schema delta.

Two concerns:

1. **Per-op volume** — gross volume from PathPayment `claimedOffers[].amount_sold`, not snapshot-delta `ABS(reserve_post − reserve_pre)`. Snapshot-delta nets opposite swaps inside one ledger.
2. **USD denomination** — multiply on-chain values (reserves for TVL, gross_volume_a for volume) by per-asset USD price from team-built price API (Oskar). All three columns end as USD.

This task **consumes** the price API; it does not build it.

Subsumes 0125 (original LP analytics) and absorbs 0195 §2b (LP TVL).

Also adds two new columns on `assets` populated by the same price-API
consumer per ADR 0043 (off-chain, list-endpoint → Lambda 2):

- `assets.usd_price NUMERIC(28,7)` — latest USD price per asset unit.
- `assets.usd_price_updated_at TIMESTAMPTZ` — when the price was last
  refreshed from the API.

Bundled into 0199 (rather than its own task) because (a) the only
consumer of asset USD price today is LP analytics + the markets list
view; (b) building two price-API clients is wasteful — single
`enrichment-shared::price` module serves both LP and asset
columns; (c) ADR 0043 cleanly assigns these columns to Lambda 2,
not type-2 runtime fetch.

Originally surfaced as Future Work in 0191 + 0194; re-evaluated in
the 0197 audit-prep pass (2026-05-12) and rolled into this task
since a product trigger (markets list parity with stellarchain.io)
materialised alongside the LP work.

## Per-lambda ownership

USD denomination → off-chain prices → ADR 0043 forces all three column writes to Lambda 2. Indexer contributes on-chain inputs only (reserves already written; `gross_volume_a` passed in SQS message body, no staging table).

| Column                                      | Inputs                                                                      | Written by   | Formula                                                                       |
| ------------------------------------------- | --------------------------------------------------------------------------- | ------------ | ----------------------------------------------------------------------------- |
| `tvl`                                       | `reserve_a, reserve_b` (Indexer, on-chain) × USD prices (oracle, off-chain) | **Lambda 2** | `tvl = reserve_a × price_a + reserve_b × price_b`                             |
| `volume`                                    | `gross_volume_a` (Indexer, in SQS msg) × `price_a` (oracle)                 | **Lambda 2** | `volume = gross_volume_a × price_a`                                           |
| `fee_revenue`                               | `volume` × `fee_bps` (Indexer, `liquidity_pools.fee_bps`, on-chain)         | **Lambda 2** | `fee_revenue = volume × fee_bps / 10000`                                      |
| `assets.usd_price` + `usd_price_updated_at` | per-asset USD price (oracle, off-chain)                                     | **Lambda 2** | direct write of API response; `updated_at = now()` on each successful refresh |

## Plan

### Phase 1 — Indexer-side extraction

- XDR parser: extract `claimedOffers[]` from PathPayment ops (types 2 + 13). Sum `amount_sold` per `(pool_id, ledger_sequence)`. Naturally excludes `LiquidityPoolDeposit` / `LiquidityPoolWithdraw` (capital flow, not volume — different op types, no claimed_offers).
- **Bidirectional swap normalization.** `claimedOffers[].amount_sold` is denominated in the asset that was sold, so an A→B swap reports in A while a B→A swap reports in B. Both directions count toward gross pool volume; the parser must convert B-denominated amounts into the A-denominated accumulator `gross_volume_a` before summing. The conversion uses the trade's effective price (`amount_bought_a / amount_sold_b`), not a snapshot reserve ratio (which would suffer the same opposite-swap netting flaw the snapshot-delta approach hit). Verification queries (see Acceptance Criteria below) compare USD-denominated totals **after** this normalization, so any direction mismatch surfaces as a Horizon-vs-explorer drift on a known mixed-direction pool.
- Producer hook in `enrichment_publish.rs`: insert-hook on each new `liquidity_pool_snapshots` row. Emit `EnrichmentMessage::LpAnalytics { pool_id: [u8; 32], snapshot_id: i64, gross_volume_a: Option<NumericString> }`. `None` for ledgers without swap activity on this pool. Exactly-once per snapshot insert.
- No DB write to `liquidity_pool_snapshots.volume` from indexer side. Phase 1 is parse + emit only.

### Phase 2 — Lambda 2 consumer

- New variant in `EnrichmentMessage` (defined above with `gross_volume_a` field).
- New module `crates/enrichment-shared/src/enrich_and_persist/lp_analytics.rs` exposing `enrich_pool_analytics(pool, msg, oracle)`.
- Worker reads `reserve_a, reserve_b, fee_bps` from DB (already populated by indexer), fetches USD prices from price API, computes:
  - `tvl = reserve_a × price_a + reserve_b × price_b`
  - `volume = gross_volume_a × price_a` (skip when `gross_volume_a IS NONE` → leave `volume = NULL`)
  - `fee_revenue = volume × fee_bps / 10000` (skip when volume NULL)
- **Atomicity model.** A single UPDATE statement carries all three column writes (transactional atomicity). **Semantic atomicity is per-column, not all-or-nothing**: a partial-oracle outcome can leave one column populated while another stays NULL — the column's input is the discriminator, not a global "all or none" gate. Decision matrix:

  | Inputs available                                           | `tvl`                                             | `volume`            | `fee_revenue` |
  | ---------------------------------------------------------- | ------------------------------------------------- | ------------------- | ------------- |
  | `price_a` + `price_b` + `gross_volume_a`                   | computed                                          | computed            | computed      |
  | `price_a` + `price_b`, `gross_volume_a IS NONE` (no swaps) | computed                                          | NULL                | NULL          |
  | `price_a` only (price_b permanent fail)                    | NULL (need both legs)                             | computed            | computed      |
  | `price_b` only (price_a permanent fail)                    | NULL (need both legs)                             | NULL (no `price_a`) | NULL          |
  | both prices permanent fail                                 | NULL                                              | NULL                | NULL          |
  | any input transient                                        | (no write — `EnrichError::Transient` → SQS retry) |                     |               |

- **Sentinel.** Permanent oracle failure writes `NULL` (not `0`) for any column whose required inputs are unavailable, and emits a WARN log carrying `pool_id`, `snapshot_id`, per-leg oracle error. NULL preserves the "fetch attempted, no value" semantics without conflating with legitimate zero-volume snapshots (a pool with no swaps in a ledger genuinely has `volume = 0`). `liquidity_pools.tvl` (latest, if column exists) is NEVER overwritten by Lambda 2 — only by indexer reserves recompute. If operational distinction between "permanent fail" and "pending" becomes valuable, surface via metrics / log filters or a future `oracle_status` enum, not via numeric sentinels.
- Transient (price-API 5xx, network, rate limit) → `EnrichError::Transient` → SQS retry → DLQ.
- Backfill of pre-existing snapshots (NULL because they predate this task) → owned by 0196.

### Phase 2b — Assets USD price columns

- Migration: add `assets.usd_price NUMERIC(28,7) NULL` +
  `assets.usd_price_updated_at TIMESTAMPTZ NULL` + partial DESC index
  on `usd_price` for markets-style sortable list endpoint.
- Producer: insert/update hook on `assets` table — new asset row or
  asset that hasn't been priced in N hours → emit
  `EnrichmentMessage::AssetUsdPrice { asset_id }`. Refresh policy:
  periodic janitor sweep (cron-driven SQS publish) for stale rows,
  same pattern as 0191's icon refresh janitor.
- Consumer: extend `enrichment-shared::price` module (built for LP)
  with a per-asset price-API call. Write both columns atomically.
- Failure modes: permanent fail → leave both NULL (do not retry until
  next janitor sweep); transient → SQS retry → DLQ. Same matrix
  shape as Phase 2.
- Backfill of existing `assets` rows → owned by 0196 (add a new
  subcommand to the `enrich` binary if not already covered by the
  janitor model).

### Phase 3 — Soroban DEX adapters

Soroswap, Phoenix, etc. emit `swap` events with explicit per-swap amounts; no PathPayment. Per-DEX adapter scaffolding. Out of Phases 1+2; gates on Phase 1+2 landing.

## Acceptance Criteria

**Phase 1 (Indexer):**

- [ ] PathPayment ops yield `gross_volume_a` extraction during XDR parse (unit + integration tests with mocked claimed_offers).
- [ ] Insert-hook emits exactly one `LpAnalytics` SQS message per new `liquidity_pool_snapshots` row (test).
- [ ] `gross_volume_a = None` on no-swap ledgers; deposit/withdraw ops do not contribute to the sum.

**Phase 2 (Lambda 2):**

- [ ] `tvl`, `volume`, `fee_revenue` populated by Lambda 2 from `reserves + gross_volume_a + fee_bps + oracle prices`.
- [ ] Permanent oracle fail → per-column NULL (matrix in Phase 2 §Atomicity) + WARN log carrying pool_id, snapshot_id, per-leg oracle errors; transient → SQS retry.
- [ ] `liquidity_pools.tvl` (if column exists) is not overwritten by Lambda 2 under any oracle outcome (only by indexer reserves recompute).
- [ ] Sample query: non-NULL `tvl` and `volume` on production-region pools with valid oracle data.
- [ ] Sample query: `volume / price_a` agrees with Horizon `/liquidity_pools/{id}/trades` gross volume **within 1% tolerance** on a known mixed-direction pool. The "≈" comparison is intentional — Horizon and the explorer both use per-operation extraction (Horizon parses `claimedOffers[]` the same way), so the only sources of drift are (a) USD price-snapshot timing (Horizon uses asset units, we multiply by `price_a` then divide back), (b) rounding in `NUMERIC(28,7)` arithmetic, (c) Soroban DEX swap events not yet covered in Phase 1 (Phase 3 scope). Drift > 1% on a classic-only pool indicates an extraction bug and blocks the AC.
- [ ] `GET /liquidity-pools/:id/chart` returns non-null time series (originally 0125 AC).

**Phase 2b (Assets USD price):**

- [ ] Migration adds `assets.usd_price NUMERIC(28,7)` + `usd_price_updated_at TIMESTAMPTZ` + partial DESC index on `usd_price`.
- [ ] Producer hook + janitor sweep emit `AssetUsdPrice` SQS messages (test).
- [ ] Lambda 2 writes both columns atomically from price-API response; permanent fail → both NULL; transient → SQS retry.
- [ ] Sample query: non-NULL `usd_price` on production-region tradeable assets after Lambda 2 drains the initial backlog.
- [ ] 0196 backlog updated to capture asset-price backfill subcommand if janitor model is insufficient.

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
