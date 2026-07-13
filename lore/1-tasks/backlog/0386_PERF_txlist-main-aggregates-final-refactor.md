---
id: '0386'
title: 'PERF: main txlist — drop FINAL in fetch_tx_list_aggregates (operations_appearances + soroban_contracts whole-table merge)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0357', '0364', '0365']
tags: [perf, clickhouse, read-path, priority-high, effort-medium]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Task created — split out of 0357 cluster after Stanisław flagged main /transactions list still reads ~2M rows/request'
---

# PERF: main txlist — drop FINAL in `fetch_tx_list_aggregates`

## Summary

The main (no-filter) `/transactions` list endpoint reads **~2M rows/request**
even though **Statement A** (the candidate scan in
`crates/api/src/transactions/queries.rs:39`) already drops `FINAL` and reads in
primary-key order (~2e5 rows). The remaining cost is in the **per-page
aggregate hydration** — [`common::ch::fetch_tx_list_aggregates`] — which keeps
three `FINAL` clauses. Same scan→seek class as the 0357 cluster (0364 assets,
0365 lptxs). Drop the whole-table merges; keep output byte-identical.

## Context

Flagged by Stanisław (2026-07-13). Sibling of the 0357 read-path perf cluster.
Distinct from 0364 (astlist/astdetail) and 0365 (lptxs companion) — this is the
**global** transaction list, not an entity-scoped tx-list.

Root-cause read (`crates/api/src/common/ch.rs`):

- `operations_appearances oa FINAL` — op-type aggregate (~L140). Bounded by
  `key_filter` (page `(ledger_sequence, transaction_id)` IN + partition prune),
  but `FINAL` still forces a part merge over the matched ranges.
- `operations_appearances oa FINAL` — contract-ids aggregate (~L157), same
  bound + `FINAL`.
- `soroban_contracts sc FINAL` (~L158) — **whole-table `FINAL` merge, ~159k
  rows**, no key bound. This is the un-pruned whole-dimension read (the 0364
  class: `soroban_contract_metadata FINAL`, issuer-seek fixes).

`operations_appearances` is append-only → `FINAL` is droppable with Rust
consecutive-dedup (mirrors Statement A's own comment at queries.rs:39 and the
`arm(...)` driver at queries.rs:450). `soroban_contracts` is a small
`ReplacingMergeTree` keyed on `id` → replace whole-table `FINAL` with a
`LIMIT 1 BY id` id-IN seek bounded to the page's `oa.contract_id` set (same
pattern as the accounts seek at queries.rs:273).

## Implementation Plan

### Step 1: op-type + contract aggregates — drop `FINAL` on `operations_appearances`

Append-only source; the `groupUniqArray` aggregate is order-independent and the
key columns are immutable across physical versions → drop `FINAL`, dedup is a
no-op on the aggregated set. Verify no duplicate op-rows leak into the
`groupUniqArray`.

### Step 2: `soroban_contracts` — whole-table `FINAL` → id-IN seek

Replace `JOIN soroban_contracts sc FINAL` with a bounded lookup on the page's
distinct `oa.contract_id` values (`WHERE sc.id IN (...) LIMIT 1 BY sc.id`).
`contract_id` is immutable per key → `LIMIT 1 BY id` is deterministic, no
`FINAL` needed.

### Step 3: post-backfill verification of the entity-scoped tx-lists

Per Stanisław: acctxs / asttxs / lptxs ("transakcje dla pooli, accounts,
assets") share this same `fetch_tx_list_aggregates` helper and are _probably_
fine, but **must be re-checked after the accounts_recent backfill** — the row
counts move. Confirm read_rows on each entity-scoped variant via
`system.query_log` once the backfill lands.

## Acceptance Criteria

- [ ] Main `/transactions` list read_rows bounded to the page working set (not
      ~2M); verified via `system.query_log`.
- [ ] No whole-dimension read remains in `fetch_tx_list_aggregates` —
      `soroban_contracts FINAL` and both `operations_appearances FINAL` gone.
- [ ] Output byte-identical to pre-change — op-type badges + `contract_ids`
      per list row (spot-check native, Soroban-invoke, and multi-op txs).
- [ ] acctxs / asttxs / lptxs read_rows re-verified post-backfill (Step 3).
- [ ] **Docs updated** — N/A (query-internal; no schema/index change).
- [ ] **API types regenerated** — N/A (no DTO/API surface change).

## Notes

- Depends on nothing to start, BUT Step 3 verification is gated on Stanisław's
  accounts_recent backfill (in-flight 2026-07-13).
- Statement A itself is already fixed (queries.rs:39) — do **not** touch it;
  the win here is purely the aggregate-hydration path.
