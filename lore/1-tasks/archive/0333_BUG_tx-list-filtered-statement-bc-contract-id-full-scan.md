---
id: '0333'
title: 'Filtered /transactions (Statement B/C) full-table scan of operations_appearances blows api_reader read_rows quota (CH Code 201)'
type: BUG
status: completed
related_adr: []
related_tasks: ['0290', '0298', '0334']
tags:
  ['clickhouse', 'api', 'performance', 'quota', 'phase-launch', 'priority-high']
links: []
history:
  - date: 2026-06-29
    status: active
    who: fmazur
    note: >
      Created from live prod incident — api_reader read_rows quota exhausted
      (CH Code 201), 500-ing every CH endpoint. Root cause traced to the
      contract-filtered tx-list subquery (Statement B) scanning
      operations_appearances by contract_id with no skip-index: ~6.2B
      read_rows/query, 7 queries = 43.4B of the 50B/hr budget. Sibling of
      0290 (which fixed the unfiltered Statement A); the filtered B/C skip-index
      was explicitly deferred there as out-of-scope.
  - date: 2026-06-29
    status: completed
    who: fmazur
    note: >
      `idx_oa_contract_id` bloom_filter(0.001) added to init.sql + applied LIVE
      on prod (ALTER ADD INDEX + MATERIALIZE INDEX, mutation_523367 is_done=1,
      0 failures). Verified: local before/after = 13.18M→245K read_rows (~54x),
      granules 1609→32; prod EXPLAIN confirms planner uses idx_oa_contract_id
      (Skip Granules 450726/757664 on an active contract — sparse contracts,
      the quota killers, prune ~95%+). Docs updated (CH 02 endpoint SQL).
      1 file + 1 doc changed. Spawned 0334 (asset-detail seek rewrite) from the
      same root pattern surfaced while investigating.
---

# Filtered /transactions (Statement B/C) full-table scan blows api_reader read_rows quota

## Summary

The contract-filtered (and op_type-filtered) transaction-list paths scan
`operations_appearances` by a **non-PK** column with **no skip-index**, reading
~6.2 billion rows per query (effectively a full-table scan). A handful of such
queries per hour (~7) exhaust the `api_throttle.read_rows` quota (50B/hr) for
user `api_reader`, returning **CH Code 201 QUOTA_EXCEEDED** which 500s **every**
CH endpoint (the quota is per-user) until the fixed hourly window rolls over.
This is the asset/contract detail page's "Latest transactions" query.

## Status: Completed

**Current state:** Done. `idx_oa_contract_id` live on prod (MATERIALIZE complete,
verified via EXPLAIN) and in `init.sql`. `type`/Statement C left as a deferred
follow-up; asset-detail full-dimension-scan (the same root pattern, found while
investigating) spawned to **[[0334]]**.

## Implementation Notes

- Added `INDEX idx_oa_contract_id contract_id TYPE bloom_filter(0.001) GRANULARITY 1`
  to `operations_appearances` in `crates/db-clickhouse/schema/init.sql` (mirrors
  the existing `idx_oa_pool_ids` / 0290 `idx_acc_id` pattern). `init.sql` is
  `CREATE … IF NOT EXISTS` → covers fresh installs only; the **live prod table
  needs `ALTER TABLE … ADD INDEX … + MATERIALIZE INDEX`** (huge table → run in
  the 0281 maintenance window).
- **Verified locally** ([[local-api-clickhouse-run]]; 13.18 M `operations_appearances`
  rows, single partition 124). Query = the Statement-B `operations_appearances`
  arm (`WHERE contract_id = ? ORDER BY ledger_sequence DESC, transaction_id DESC
LIMIT 80`). read_rows before → after the materialized index:

  | contract regime         | before                | after  | factor                                |
  | ----------------------- | --------------------- | ------ | ------------------------------------- |
  | sparse (42 appearances) | 13.18 M (whole table) | 245 K  | **~54×**                              |
  | mid (2036)              | 4.18 M                | 2.44 M | ~1.7×                                 |
  | heavy (3.24 M)          | 81.7 K                | 81.7 K | — (already early-terminates near tip) |

  `EXPLAIN indexes=1` confirms the planner picks `idx_oa_contract_id`
  automatically (no `force_data_skipping_indices` needed): granules 1609 → 32.
  The sparse regime is exactly the prod 6.2 B-rows/query full scan that blew the
  quota.

## Design Decisions

### From Plan

1. **bloom_filter(0.001) GRANULARITY 1 on `contract_id`** — identical tool +
   FP-rate as the proven `idx_oa_pool_ids` / `idx_acc_id` skip-indexes.

### Emerged

2. **Worst case is the SPARSE contract, not the active one** — an active contract
   early-terminates near the tip (read-in-order `ORDER BY ledger DESC LIMIT`), so
   it was already cheap; a sparse/old contract scans the whole table to fill the
   page. The index targets exactly that regime. (Matches prod: 6.2 B/query came
   from a sparse contract, not a hot one.)
3. **`type` / Statement C skip-index NOT added here** — `type` is low-cardinality
   (a bloom is weak when the filtered value is common) and Statement C did not
   feature in the incident (one 59 M-row run vs 43.4 B from contract_id). Left as
   a deferred follow-up needing its own measurement (set-index vs bloom). Spawn a
   backlog task if it recurs.

## Context

Live incident 2026-06-29: all CH-backed endpoints returned `500 {"code":"db_error"}`
fast (~120–280ms, pre-execution rejection); only `/v1/network/stats` survived
(it serves a last-good in-memory snapshot on DB error, returning stale data).

`system.query_log` attribution for the failing hour (user `api_reader`):

| query (normalized)                                                                                     | runs | total read_rows | avg/run   |
| ------------------------------------------------------------------------------------------------------ | ---- | --------------- | --------- |
| `SELECT oa.ledger_sequence, oa.transaction_id FROM operations_appearances oa WHERE oa.contract_id = …` | 7    | **43.41B**      | **6.20B** |
| asset detail row                                                                                       | 15   | 310.67M         | 20.71M    |
| ... (everything else ≤ ~110M total)                                                                    | —    | —               | —         |

The first query alone ate **87%** of the 50B budget. It is the inner
contract-filter subquery of Statement B in
`crates/api/src/transactions/queries_ch.rs` (the `operations_appearances` arm of
the 3-source UNION). The code comment at ~L480-484 already flags it:
`operations_appearances` arm "scans the pruned partition (`contract_id` is not its
PK prefix — deferred skip-index follow-up, same as op_type)". Statement C
(op_type filter) has the same gap (~L541-545, "needs a skip-index on `type`").

**Why ~6.2B not ~one partition:** a "latest N for this contract" query cannot be
bounded to a single ledger partition (the contract's recent activity may live in
any partition), so without a skip-index CH scans all partitions for matching
`contract_id` — i.e. the whole table. The single-partition prune the comment
assumes does not apply to this path. **Investigate + confirm** whether the
filtered path bounds `ledger_sequence` at all.

0290 fixed the unfiltered Statement A (35M→~1M/poll via `accounts.id`
bloom_filter skip-index + key-seeks) and raised the quota 10B→50B. It explicitly
left the `operations_appearances(type, contract_id)` skip-index out of scope
("separate backlog, if they recur"). They have now recurred → this task.

## Implementation Plan

### Step 1: Confirm the scan shape

Reproduce locally ([[local-api-clickhouse-run]]) or read prod `system.query_log`:
`EXPLAIN`/actual `read_rows` for the contract-filtered and op_type-filtered
Statement B/C subqueries. Confirm whether `ledger_sequence` is bounded and why
the prune the comment assumes does not bite (~6.2B/query observed).

### Step 2: Add skip-index(es) on operations_appearances

Mirror the 0290 `accounts.id` fix. In `crates/db-clickhouse` init.sql:
`ALTER TABLE operations_appearances ADD INDEX … contract_id TYPE bloom_filter(…)`
and an index on `type` for Statement C. `MATERIALIZE INDEX` for existing parts
(huge table — schedule via the maintenance window, see [[0281]]). Pick granularity

- false-positive rate empirically (0290 used `bloom_filter(0.001)`).

### Step 3: Verify read_rows drop

Re-run the filtered queries; confirm `read_rows` falls from ~6.2B to granule-bounded
(orders of magnitude). Then `api_throttle.read_rows` can stay or be lowered.

### Step 4: Deploy

Schema/index change → CH-side via `db-clickhouse-init`; any `users.d/` quota change
needs a CH container **recreate**, not `--tags app` ([[ch-usersd-rbac-needs-container-recreate]]).

## Acceptance Criteria

- [x] Root cause of ~6.2B read_rows/query confirmed — sparse contract forces a
      full-table scan (cannot prune `ledger_sequence` for "latest N for contract");
      reproduced locally (42-appearance contract read the whole 13.18 M table)
- [x] Skip-index on `operations_appearances(contract_id)` added to `init.sql`
      and applied LIVE on prod (`ALTER ADD INDEX` + `MATERIALIZE INDEX`,
      mutation_523367 `is_done=1`, online — no maintenance window needed; it
      hardlinks data + writes only the ~tens-of-MB index)
- [x] Skip-index on `operations_appearances(type)` for Statement C — **N/A here**:
      low-cardinality + not in the incident; deferred follow-up (see Emerged #3)
- [x] Filtered Statement B `read_rows` measured ≪ table (245 K vs 13.18 M; 32/1609
      granules) — verified locally
- [x] No more Code 201 on `api_reader` under normal filtered-list traffic — index
      live + verified; sparse-contract full scan (the trigger) now seeks. Keep an
      eye on `system.query_log` for any residual Code 201 from Statement C (`type`)
      or the asset-detail 1.58 GB read (→ 0334).
- [x] **Docs updated** — `endpoint-queries-clickhouse/02_get_transactions_list.sql`
      updated (contract_id now seeks via `idx_oa_contract_id`; type still scans).
      DB-schema-overview is PG-era DDL (CH index lives in `init.sql`, the CH source
      of truth) per [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [x] **API types regenerated** — N/A: query-plan/schema-index fix only, no
      `crates/api/**` request/response shape change.

## Notes

- Sibling/parent: [[0290]] (Statement A fix + quota raise), [[0298]] (0290 close-out:
  lower read_rows back toward 1–2B, canonical SQL, regression test — blocked on this
  fix landing, since lowering the cap before the filtered scan is fixed would re-break).
- Diagnosis playbook saved to memory: `ch-api-throttle-readrows-quota-incident`.
- The `quotas.xml` comment calls the interval "sliding-window" but observed behaviour
  is fixed-hourly (reset exactly at 10:00) — fix the comment while here.
