---
id: '0386'
title: 'PERF: main txlist — drop FINAL in fetch_tx_list_aggregates (operations_appearances + soroban_contracts whole-table merge)'
type: PERF
status: active
related_adr: []
related_tasks: ['0357', '0364', '0365']
tags: [perf, clickhouse, read-path, priority-high, effort-medium]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Task created — split out of 0357 cluster after Stanisław flagged main /transactions list still reads ~2M rows/request'
  - date: 2026-07-13
    status: active
    who: karolkow
    note: 'Promoted to active'
  - date: 2026-07-14
    status: active
    who: karolkow
    note: >
      Implemented. Root cause of the soroban_contracts whole-table FINAL was a
      DEAD field: `contract_ids` (PG-parity scaffolding rendered by NO frontend
      — grep confirmed only a test fixture). Resolved by DELETING `contract_ids`
      from TransactionListItem + the shared fetch_tx_list_aggregates helper +
      ledger-embedded rows, not by optimising the query. Helper 210,334 → 8,192
      read_rows/page (prod chq, -96%); soroban_contracts gone from the list
      path. op_sql FINAL kept (seek-bounded, zero read cost). 210 api + 117 web
      tests green; API types regenerated; docs updated. Remaining ~2M is
      accounts (785k) + Statement A — out of scope, tracked under 0357.
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

> **Reframed (2026-07-14).** Investigation found the `soroban_contracts FINAL`
> existed only to populate a **dead field** (`contract_ids`), so it was resolved
> by DELETING the field, not optimising the query. The original "list stops
> reading ~2M" criterion was mis-attributed — the ~2M is dominated by `accounts`
> resolution, outside this helper (see Design Decisions / Issues).

- [x] No whole-dimension read remains in `fetch_tx_list_aggregates` — the
      `soroban_contracts FINAL` join is GONE (field deleted); the contract-arm
      `operations_appearances FINAL` went with it. Prod `chq`: helper
      210,334 → 8,192 read_rows/page (−96%), `soroban_contracts` absent.
- [x] `op_sql` (operation_types) `FINAL` KEPT deliberately — seek-bounded, zero
      read cost, explicit RMT collapse (measured identical read_rows vs no-FINAL).
- [x] `contract_ids` removed from `TransactionListItem` + the shared helper +
      ledger-embedded rows; our frontend is the only consumer and renders it
      nowhere (grep: sole reference is a test fixture).
- [x] Remaining fields byte-identical (`operation_types` unchanged) on
      `/transactions` + ledger detail.
- [x] **Docs updated** — canonical SQL `02`/`05`/`07`/`20` + `clickhouse-pilot.md`
      annotated (API-shape change).
- [x] **API types regenerated** — `contract_ids` gone from `openapi.json` +
      generated TS (API surface changed; not the original N/A).
- [ ] **Out of scope (→ 0357):** `/transactions` still reads ~2M, dominated by
      the `accounts` id-seek (~785k/page; 22M-row churny RMT) + Statement A
      candidate scan (~200k). Neither lives in `fetch_tx_list_aggregates`.
- [ ] acctxs / asttxs / lptxs op_types read_rows re-verified post-backfill
      (Step 3) — contract cost is now zero there too.

## Implementation Notes

Files touched:

- `crates/api/src/common/ch.rs` — deleted `ctr_sql` + the `JOIN soroban_contracts
FINAL`, `ContractIdsRow`, the `TxListAggregates.contract_ids` field, and the
  second `tokio::join!` arm. Helper is now a single `op_sql` (FINAL kept).
- `transactions/{dto,queries,handlers}.rs`, `ledgers/queries.rs` — removed the
  `contract_ids` field + every mapping; updated doc comments.
- `accounts/queries.rs`, `assets/queries.rs`, `accounts/dto.rs` — stale
  "contract_ids intentionally unused / list on /v1/transactions only" comments.
- `libs/api-types/src/{openapi.json,generated/*}` — regenerated.
- `web/src/pages/TransactionsListPage.test.tsx` — dropped the fixture line.
- docs: `02/05/07/20_*.sql` + `clickhouse-pilot.md` — API-shape notes.

Verification: `cargo test -p api --lib` 210 passed; web `typecheck` + `test`
117 passed; prod `chq` helper read_rows 210,334 → 8,192.

## Issues Encountered

- **The "~2M" was mis-attributed.** Task premised the win on the contract
  `FINAL`, but prod measurement showed the contract aggregate was ~202k while
  the dominant per-page cost is the `accounts` surrogate→StrKey id-seek (~785k
  on a 22M-row churny RMT, 9 parts) + Statement A candidate scan (~200k). Both
  are outside `fetch_tx_list_aggregates`. This task cannot bring `/transactions`
  to "page working set" alone — that needs the accounts lever (0357).
- **Worktree package resolution.** `@rumblefish/api-types` resolved to the MAIN
  checkout's stale build (still had `contract_ids`) → false `tsc` failure.
  Fixed by symlinking `node_modules/@rumblefish/api-types` → worktree libs
  (gitignored; CI on a clean branch checkout is unaffected).

## Design Decisions

### From Plan

1. **Drop the whole-table `soroban_contracts FINAL`.** Delivered — but by
   deletion (below), which is strictly better than the planned id-IN seek.

### Emerged

2. **DELETE `contract_ids` instead of optimising it.** The field was fetched by
   NO frontend component (grep: sole reference a test fixture) — PG-parity
   scaffolding for a "touched contracts" column that never shipped (confirmed by
   the 2026-04-28 audit report + canonical SQL header). The contract _filter_
   runs server-side off a UNION driver, not this per-row array. Confirmed with
   the user that our frontend is the only API consumer. So the cheapest fix is
   no query. Removes the cost for all 5 tx-list callers at once.
3. **Keep `FINAL` on `op_sql` (operation_types).** Measured identical read_rows
   with/without FINAL (seek-bounded; the merge is over the matched rows only),
   so there is no perf reason to remove it; kept per user preference to leave
   the RMT collapse explicit rather than lean on `groupUniqArray` set-dedup.
4. **API-shape change accepted.** Removing a response field is technically
   breaking, but the field is unconsumed and the API has no external consumer
   (user-confirmed) — so removed outright rather than left as a lying `[]`.

## Future Work

- **`accounts` resolution ~785k/page** (the real `/transactions` monster) —
  belongs to the read-path cluster **0357** (txlist row), not a new task. Lever
  is merge cadence / `accounts_recent`, not a FINAL/seek change.

## Notes

- Depends on nothing to start, BUT Step 3 verification is gated on Stanisław's
  accounts_recent backfill (in-flight 2026-07-13).
- Statement A itself is already fixed (queries.rs:39) — do **not** touch it;
  the win here is purely the aggregate-hydration path.
