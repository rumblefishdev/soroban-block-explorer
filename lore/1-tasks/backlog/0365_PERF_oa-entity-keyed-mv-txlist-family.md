---
id: '0365'
title: 'PERF: operations_appearances entity-keyed MV — prefix-seek for the tx-list family (asttxs / lptxs / acctxs)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0357', '0281', '0354']
tags:
  [priority-medium, effort-large, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links:
  - crates/api/src/assets/queries.rs
  - crates/api/src/liquidity_pools/queries_ch.rs
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
      floor — the real fix is a schema-level entity-keyed structure, shared across
      the family.
---

# PERF: operations_appearances entity-keyed MV — tx-list prefix-seek

## Summary

`asttxs`, `lptxs`, and `acctxs` all page `operations_appearances` by a filter
that is **not the table's leading key** (`ORDER BY (ledger_sequence,
transaction_id, application_order)`), so each leans on `optimize_read_in_order`
early-termination — which is **entity-density-dependent**: dense entities fill a
page instantly, sparse ones scan deep from the tip (up to **70.96M rows**
measured). The read-in-order driver (0281-C / #315) is already at its floor. The
real fix is a schema-level structure keyed by the filter entity so the driver
does a **prefix-seek** (~tens of k rows) instead of a density-dependent scan —
one MV design that serves the whole tx-list family.

## Context

From the 0357 load test (2026-07-07) + `system.query_log`:

| endpoint | filter on `operations_appearances`            | @10-VU p95 | @100-VU                |
| -------- | --------------------------------------------- | ---------- | ---------------------- |
| asttxs   | `asset_code+asset_issuer_id` OR `contract_id` | 2915 ms    | **20.7% 504-timeout**  |
| lptxs    | `has(pool_ids, <pool>)`                       | 2671 ms    | **26.7% 504-timeout**  |
| acctxs   | account/participant surrogate                 | 1148 ms    | 6459 ms (slow, 0% err) |

- oa "other" group (asttxs/acctxs/txlist): avg **735k**, **max 70.96M** rows.
- lptxs pool driver: avg **3.56M**, max 27.98M rows.
- Root cause is structural, not a bloom miss — the lptxs spike showed
  `idx_oa_pool_ids` already prunes 99.85% of granules; the residual is the
  entity's real granule footprint spread across ~1000+ granules of history.
  `optimize_read_in_order` early-terminates only for DENSE entities and cannot be
  globally disabled (dense entities would then explode) — so there is **no
  query-only lever left**.

Prior art: task **0281** added a prod-only `oa_pool_seek` projection (not in
`init.sql`) for lptxs; it prunes but does not give a contiguous per-pool seek. An
entity-keyed MV supersedes it for the pool case and extends the idea to assets +
accounts.

## Implementation Plan

### Step 1: MV design (the key decision)

An `operations_appearances`-derived MV whose sort key **leads with the filter
entity**, so the driver prefix-seeks. Two shapes to weigh:

- **One unified MV** — `arrayJoin` a computed `(entity_type, entity_id)` array
  per op (pool_ids already an array; asset = issuer_id+code / contract_id;
  account = source + participants) → `ORDER BY (entity_type, entity_id,
ledger_sequence, transaction_id)`. One structure, all three endpoints seek it.
- **Per-entity MVs** — separate pool / asset / account MVs. Simpler each, but 3×
  the schema + backfill.

Decide based on fan-out (row multiplication from `arrayJoin` on multi-entity ops)
and write cost. Carries only the driver columns (`ledger_sequence`,
`transaction_id`) — hydration stays on the existing id-IN page fetch.

### Step 2: driver swap

Point `assets::fetch_transactions` + `liquidity_pools::fetch_pool_transactions`
(+ the acctxs driver) at the MV: `WHERE entity = ? ORDER BY (ledger, tx) DESC
LIMIT/keyset`. Drop the over-fetch+dedup dance where the MV makes it a clean seek.

### Step 3: prod rollout

MV + backfill of existing history + `init.sql` + coordinated deploy (writer-
coupled → the 0281 batch window). Validate byte-identical vs the current driver.

## Acceptance Criteria

- [ ] asttxs / lptxs / acctxs drivers read ~page-size (prefix-seek), not the
      density-dependent scan — read_rows bounded even for sparse + mega entities;
      verified via `system.query_log` on the worst-case entities.
- [ ] Outputs byte-identical to the current drivers (prod before/after) for all
      three endpoints, across sparse / dense / mega entities.
- [ ] MV backfilled over full history; live ingestion keeps it current
      (idempotent, matches the RMT re-ingest safety of the base table).
- [ ] Supersede or retire the prod-only `oa_pool_seek` projection (0281) if the
      pool case moves to the MV.
- [ ] **Docs updated** — REQUIRED (new schema object): update the schema +
      ingestion pages under `docs/architecture/**` per
      [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — N/A (query-internal; no API surface change).

## Notes

- Sibling of 0364 (astlist/astdetail `assets FINAL`) — **different table, root
  cause, and fix**: 0364 is the `assets` FINAL scan; this is the
  `operations_appearances` non-leading-key filter.
- Post-launch — the driver (#315 / 0281-C) already bounds the common case; this
  removes the sparse/mega-entity tail that shows up under concurrency.
- `acctxs` is the least acute (survives @100 VU, just slow) but shares the exact
  pattern, so it comes along for free with a unified MV.
