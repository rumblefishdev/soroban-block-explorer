---
id: '0365'
title: 'PERF: operation_pools — indexer-written pool-keyed companion for lptxs prefix-seek (was: entity-keyed MV for the tx-list family)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0357', '0281', '0354', '0364', '0268', '0266', '0359']
tags:
  [priority-medium, effort-large, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links:
  - crates/api/src/liquidity_pools/queries.rs
  - crates/db-clickhouse/schema/init.sql
  - crates/db-clickhouse/src/persist/writer.rs
  - crates/db-clickhouse/src/persist/stage.rs
  - crates/backfill-runner/src/bin/pool-ids-backfill.rs
  - crates/api/src/assets/queries.rs
history:
  - date: 2026-07-07
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from the 0357 read-path perf cluster (group A). The 2026-07-07 load
      test confirmed the tx-list drivers are density-bound on the non-leading-key
      `operations_appearances` filter: asttxs 20.7% and lptxs 26.7% 504-timeouts
      @100 VU (both ~2.7-2.9 s p95 @10 VU), the oa driver reading up to 70.96M
      rows for sparse entities. The read-in-order driver (0281-C / #315) is at its
      floor — the real fix is a schema-level entity-keyed structure.
  - date: 2026-07-09
    status: backlog
    who: stkrolikiewicz
    note: >
      Rewrote after a design pass + a local ClickHouse 26.3 bake-off. Three
      corrections to the original "one unified entity-keyed MV" plan: (1) only
      LPTXS structurally needs a new structure — its filter `has(pool_ids, X)` is
      array-membership, which cannot be a sort-key prefix and cannot be a
      projection (projections cannot `arrayJoin`); (2) ASTTXS is scalar
      (`asset_issuer_id` / `contract_id`) and can use a projection — split to a
      sibling, out of scope here; (3) ACCTXS already seeks `transaction_participants`
      on its leading PK `account_id` — it does NOT share the pattern (the original
      "comes along for free" note was wrong) and needs no change. Deliverable is
      an INDEXER-WRITTEN table `operation_pools` (not an MV): MVs on the RMT source
      fire pre-dedup and, crucially, do NOT see source DELETEs (repair tooling uses
      `ALTER … DELETE` on companion tables — `repair_tier1.rs:409`), so the writer
      must own both writes. Precedent: `transaction_participants`. Prod-measured
      size: `sum(length(pool_ids))` = 377.63M (ceiling), `uniq` deduped = 363.17M
      rows → ~2 GB compressed (~1.2 GB with Delta codecs); oa itself is 6.41B rows.
---

# PERF: operation_pools — pool-keyed companion for lptxs prefix-seek

## Summary

`lptxs` (`GET /v1/liquidity-pools/:id/transactions`) pages `operations_appearances`
by `has(pool_ids, X)` — **array membership on a non-leading key**
(`ORDER BY (ledger_sequence, transaction_id, application_order)`). The read-in-order
driver (0281-C / #315) early-terminates only for DENSE pools; a broad/sparse pool
scans deep from the tip. Array membership **cannot** be a sort-key prefix and
**cannot** be a projection (projections cannot `arrayJoin`), so the only fix is a
**pool-keyed derived table** whose sort key leads with `pool_id`:

```sql
CREATE TABLE operation_pools (
    pool_id          FixedString(32),
    ledger_sequence  Int64 CODEC(Delta, ZSTD(1)),
    transaction_id   Int64 CODEC(Delta, ZSTD(1))
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (pool_id, ledger_sequence, transaction_id);
```

Populated by `arrayJoin(pool_ids)` (one row per pool a tx crossed), it turns the
lptxs driver into a `WHERE pool_id = ? ORDER BY (ledger, tx) DESC` prefix-seek and
lets it drop the over-fetch+Rust-dedup dance. Shape mirrors `transaction_participants`.

## Context

From the 0357 load test (2026-07-07) + `system.query_log`:

| endpoint | filter on `operations_appearances`                 | @10-VU p95 | @100-VU                |
| -------- | -------------------------------------------------- | ---------- | ---------------------- |
| asttxs   | `asset_code+asset_issuer_id` OR `contract_id`      | 2915 ms    | 20.7% 504-timeout      |
| lptxs    | `has(pool_ids, <pool>)`                            | 2671 ms    | **26.7% 504-timeout**  |
| acctxs   | `transaction_participants.account_id` (leading PK) | 1148 ms    | 6459 ms (slow, 0% err) |

- lptxs pool driver: avg **3.56M**, max 27.98M rows read. Not a bloom miss —
  `idx_oa_pool_ids` prunes ~99.85% of granules; the residual is the pool's real
  footprint spread across the history. A broad pool sits in ~every granule, so no
  prune. `optimize_read_in_order` cannot be globally disabled (dense pools would
  explode) → **no query-only lever left**.
- **Local bake-off (CH 26.3, 5M synthetic rows)** confirmed the mechanism: for a
  broad pool the primary key prunes `614/614` (zero — pool not the key) and the
  bloom `610/614` (useless); read = ~410 granules ≈ **3.36M rows**. On a
  `(pool_id, ledger, tx)`-keyed copy the primary key seeks `1/63` → **2 288 rows**
  (a 500k-row pool: **10 480** — the seek is pool-size-independent).

Prior art: task **0281** added a prod-only `oa_pool_seek` projection (not in
`init.sql`); it prunes but gives no contiguous per-pool seek (it predates the
`pool_id`→`pool_ids` array move of 0261/0268 — array membership killed the
projection-seek). `operation_pools` supersedes it.

## Design decision — indexer-written table, not an MV

Both an incremental MV `TO operation_pools` and an indexer-written table produce
the identical structure. Choose the **indexer-written table**:

- MVs fire on the inserted block **pre-RMT-dedup**, and — decisive — **do NOT see
  source DELETEs/mutations**. Repair tooling uses `ALTER … DELETE` on companion
  tables (`repair_tier1.rs:409`); an MV would silently drift. The writer owning
  both writes keeps oa and `operation_pools` consistent, including on re-ingest.
- Precedent is exact: `transaction_participants` is an entity-keyed, indexer-written,
  RMT companion with DELETE-by-ledger re-ingest (`writer.rs:190`). The only in-repo
  MV (`balance_aggregates_mv`) is a refresh-recompute MV — a different pattern.

## Implementation Plan

1. **Schema** — add `operation_pools` (above) to `init.sql`.
2. **Indexer** — `stage::prepare` emits `operation_pool_rows` (`arrayJoin` of
   `pool_ids` per op-appearance → `(pool_id, ledger, tx)`); `writer` writes them
   beside `transaction_participants`; re-ingest DELETE-by-ledger parity.
3. **Backfill** — one-shot `INSERT INTO operation_pools SELECT arrayJoin(pool_ids)
AS pool_id, ledger_sequence, transaction_id FROM operations_appearances`, or
   extend the `pool-ids-backfill` worker (already re-parses these ledgers). RMT on
   `(pool_id, ledger, tx)` makes an overlapping backfill idempotent.
4. **Driver swap** — `liquidity_pools::fetch_pool_transactions` → `WHERE pool_id = ?
… ORDER BY ledger DESC, tx DESC LIMIT 1 BY (ledger, tx) LIMIT ?` (mirrors the
   acctxs driver on `transaction_participants`). Drop the over-fetch×4 / re-fetch×128
   / Rust-dedup dance. Validate byte-identical vs the current driver.
5. **Retire** the prod-only `oa_pool_seek` projection (0281).

## Sizing (prod-measured 2026-07-09)

- `sum(length(pool_ids))` = **377.63M** (ceiling); `uniq((arrayJoin(pool_ids),
ledger, tx))` = **363.17M** deduped rows.
- ~48 B/row raw → **~2 GB compressed** (default), **~1.2 GB** with the Delta codecs
  above. ~6% of oa's 6.41B rows; a small fraction of its disk.

## Acceptance Criteria

- [ ] lptxs driver reads ~page-size (prefix-seek), not the density scan —
      `read_rows` bounded even for sparse + mega pools; verified via
      `system.query_log` on worst-case pools.
- [ ] Output byte-identical to the current driver (prod before/after), across
      sparse / dense / mega pools; E20 (`/liquidity-pools/:id/transactions` vs
      Horizon) green.
- [ ] `operation_pools` backfilled over full history; the indexer keeps it current,
      re-ingest-safe (DELETE-by-ledger parity with `transaction_participants`).
- [ ] Retire the prod-only `oa_pool_seek` projection (0281).
- [ ] **Docs updated** — REQUIRED (new schema object + ingestion step): schema +
      ingestion pages under `docs/architecture/**` per
      [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — N/A (query-internal; no API surface change).

## Notes / out of scope

- **asttxs (perf) — subsumed by 0359, NOT a new task.** 0359
  (`asset-participation index re-model`, active) builds `operation_asset_appearances`
  (per-(op, asset, role) fan-out, asset-leading seek) and already rewrites
  `/assets/:id/transactions` onto it — the same asset-keyed structure the perf fix
  needs. Perf findings fed to 0359 (2026-07-09): the density-scan is also a PERF
  driver (asttxs 20.7% @100-VU 504s); a projection is the wrong tool (a normal
  projection re-copies all ~2.8B asset-op rows, ×2 for the OR arms → ~60-90 GB +
  rebuild-on-merge); mega-asset concentration (417 assets ≥1M rows = 71.6% of asset
  traffic) → no cheap "mega-only" subset. Once asttxs seeks the new index,
  `idx_oa_asset_issuer_id` (97 MiB) is droppable — analog to `idx_oa_pool_ids`
  (298 MiB) in 0372.
- **acctxs** — already seeks `transaction_participants` on leading PK `account_id`
  (`accounts/queries.rs:504`); its @100-VU slowness is hydration/volume, not a
  non-leading-key scan. No change here.
- **Optional follow-up — drop `pool_ids` from `operations_appearances`.** The array's
  op→pool direction (returned as `pool_ids` in the transactions response,
  `transactions/queries.rs:870` / `dto.rs:182`) is **not consumed by the `web/`
  frontend** (verified: stubbed to `[]`, never read). If no external API consumer
  needs it, dropping the column frees more disk than `operation_pools` costs (net
  smaller). Gate on confirming external consumers; regen API types if the response
  field is also dropped. Separate task — do not couple to the perf fix.
- Sibling of 0364 (astlist/astdetail `assets FINAL`) — different table, root cause,
  and fix.
