---
id: '0575'
title: 'REFACTOR: key transaction_participants and operation_asset_appearances by the transaction position — drop their transaction_id'
type: REFACTOR
status: active
related_adr: ['0059']
related_tasks: ['0538', '0541']
tags:
  [
    'clickhouse',
    'storage',
    'performance',
    'phase-future',
    'effort-medium',
    'priority-high',
  ]
links:
  - crates/db-clickhouse/schema/init.sql
history:
  - date: 2026-09-23
    status: backlog
    who: karolkow
    note: >
      Filed as step 5 of the 0538 programme, its first presence tables
      (decision karolkow, 2026-09-23). The two tables share one shape and hold
      170.13 GiB of `transaction_id` between them; the target shape already
      runs on production as `contract_transactions` (0541), where the
      position costs 1.31 B/row against the surrogate's 8.03.
  - date: 2026-09-23
    status: active
    who: karolkow
    note: >
      Promoted. Step 1 starts: one partition of each table rebuilt locally
      from production reads, codec variants side by side.
---

# Key `transaction_participants` and `operation_asset_appearances` by the transaction position

## Summary

Replace `transaction_id` (hash64 of the hash, ratio 1.0) with
`application_order` in the two largest presence indexes, so they locate a
transaction the way `transactions`, `asset_transfers`,
`contract_transactions` and `soroban_events` already do (ADR 0059). Saves
~180 GiB (_estimate_ from a measured partition, with two codecs) and lists account and asset transactions in execution
order inside a ledger instead of hash order.

## Context

Programme: [0538](../0538_EPIC_canonical-transaction-and-event-location/README.md),
step 5. Step 3 (`soroban_events`, task 0541) is done and left the reference
shape on production:

```
contract_transactions  ORDER BY (contract_id, ledger_sequence, application_order)
```

Measured on production 2026-09-23 (`system.parts_columns`, active parts):

| table                         | rows     | `transaction_id`               | `ledger_sequence`     | table                          |
| ----------------------------- | -------- | ------------------------------ | --------------------- | ------------------------------ |
| `operation_asset_appearances` | 11.77 bn | 88.06 GiB, 8.03 B/row          | 12.23 GiB, 1.12 B/row | 100.74 GiB                     |
| `transaction_participants`    | 10.97 bn | 82.07 GiB, 8.03 B/row          | 28.88 GiB, 2.83 B/row | 112.00 GiB                     |
| `contract_transactions` (ref) | 2.99 bn  | `application_order` 1.31 B/row | 1.97 B/row            | 9.29 GiB (203 parts, unmerged) |

Projected at 1.31 B/row (_estimate_, the reference table is unmerged, so an
upper bound): `operation_asset_appearances` −74 GiB, `transaction_participants`
−69 GiB. Free disk 482.58 GiB of 1.72 TiB (2026-09-23), so one rebuilt copy
beside the original fits; still one table at a time (0538 constraints).

## Target shape

```sql
CREATE TABLE transaction_participants (
    account_id        Int64,
    ledger_sequence   Int64 CODEC(Delta, ZSTD(1)),
    application_order Int16 CODEC(T64, ZSTD(1))
) ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (account_id, ledger_sequence, application_order);
-- operation_asset_appearances: the same with asset_id leading
```

Codecs chosen from the trial below (step 1): the position alone saves 58–73%
of the row, the two codecs another ~40 GiB across both tables. `UInt32` for
the ledger was measured and pays nothing.

## Implementation Plan

### Step 1: trial partition + read benchmark (read-only on prod, local build)

Rebuild one partition of each table with the target shape (codec variants
side by side), measure B/row per column. Benchmark the account and asset
transaction lists — today's `(ledger_sequence, id) IN` page fetch against the
full-PK `(ledger_sequence, application_order) IN` — on the hottest account,
native XLM and one mid asset. Record in `notes/`.

### Step 2: writer

`stage.rs` / `rows.rs`: rows carry `application_order` (staging already maps
hash → position: `app_order_by_hash`).

**Deploy order is strict — no `DEFAULT` escape here.** The driver checks the
row struct against `DESCRIBE` (task 0310): the new indexer cannot write to the
old table (no `application_order`), and the old indexer cannot write to the
new one (no `transaction_id`). Both a `DEFAULT` and an `ALTER ADD COLUMN`
would leave `transaction_id` in the sort key, so the swap is the only move:
fill the staging table, stop the indexer, fill the tail, `EXCHANGE TABLES`,
deploy the new indexer and API together, start the indexer. Same shape as the
0541 window.

### Step 3: readers, same PR

- `accounts/queries.rs` (account transaction list driver + page fetch)
- `assets/queries.rs` (`seek("operation_asset_appearances", …)` arm)
- `transactions/queries.rs` (participants of one transaction)
- cursor on `(ledger_sequence, application_order)`; an old cursor answers 400
  `invalid_cursor`, as 0541 did
- `backfill-runner`: `bootstrap.rs`, `repair_tier1.rs`, `rpc_snapshot.rs`,
  `run.rs`

### Step 4: fill + swap, one window per table

New table filled per partition in ClickHouse from the old one joined to
`transactions` on `id` (no S3); gate per partition: row counts equal, every
row has a position. `EXCHANGE TABLES` with the indexer stopped
(`docs/backfills.md`), readers deployed in the same window. Old table dropped
after the checks (`max_table_size_to_drop` override, as in 0541 phase 5).

## Acceptance Criteria

- [x] Trial partition measured, codec choice recorded with numbers (partition 128, 2026-09-23)
- [x] Read benchmark recorded; no list over the gate (< 1 s, < 1 GiB) —
      staging, 2026-09-23; native XLM 1451 → 516 MiB, hottest account
      639 MiB (re-measure after the swap)
- [x] Neither table carries `transaction_id`; saving re-measured per table —
      swapped 2026-09-23: 112.04 → 20.12 GiB, 100.76 → 12.39 GiB; old
      tables dropped the same day
- [x] Account and asset lists in execution order inside a ledger — checked on
      ledger 64 454 000 and on one account, one asset (after the swap, below)
- [x] `transaction_id` still has no new consumer added by this change — the
      PR's added lines name it only where `operation_types` is still keyed by
      it (`operations_appearances`, the same consumer as before, now keyed
      off the page rows), a moved `init.sql` comment and one test
- [x] **Docs updated** — `database-schema/**`: overview §4.5 / §4.5.1,
      ClickHouse pilot, endpoint queries 03, 07, 10 (02: N/A — the global list
      does not read either table); `indexing-pipeline/**`; `docs/backfills.md`;
      `docs/deployment.md`
- [x] **API types regenerated** — ran 2026-09-23, no diff: the cursor is an
      opaque string

## Progress

- **Step 1 — trial:** done; numbers in
  [notes/R-trial-partition-128.md](notes/R-trial-partition-128.md). Row
  11.19 → 1.95 B (`transaction_participants`), 9.09 → 1.12 B
  (`operation_asset_appearances`); 0 rows without a position.
- **Steps 2–3 — writer and readers:** done on branch
  `refactor/0575-presence-tables-canonical-position`, draft PR #483 — merged
  only right before the window (`docs/deployment.md`);
  [notes/S-implementation-log.md](notes/S-implementation-log.md). 881 tests
  green; decode smokes run against a real ClickHouse.
- **Step 4 — fill and swap (the operator's):** runbook ready —
  [`fill_presence.sql`](notes/fill_presence.sql),
  [`gate_presence.sql`](notes/gate_presence.sql),
  [`fill_presence.zsh`](notes/fill_presence.zsh); procedure in
  `docs/backfills.md` ("Presence tables by transaction position") and the
  window in `docs/deployment.md` ("Presence tables by position"). Fill
  statement and gate dry-run read-only on production (64,450,000–64,460,000:
  keys equal, 0 without a position); the loop dry-run with stubbed `chw`/`chq`.
  **Staging filled 2026-09-23** for partitions 100–128 of both tables, every
  50k slice gated; 112.03 → 19.97 GiB and 100.75 → 12.35 GiB before merges
  (−180 GiB). Head partition 129 is the window's. Numbers, gate history and
  the read benchmark:
  [notes/R-fill-and-read-benchmark.md](notes/R-fill-and-read-benchmark.md).

## The window (2026-09-23)

PR #483 merged (`cb7ea315`) right before it. Run from one local checkout,
`window-0575`: first at `9a89d35c` — the code production ran, deployed
2026-09-22 15:26 UTC, not the last `production-*` tag — then at `cb7ea315`.
Ingest stood still for ~51 minutes, most of it the first Lambda build.

| UTC      | step                                                                                                                               |
| -------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| 13:02    | Pause: Compute from `9a89d35c` with `indexerLambdaConcurrency: 0`. **H = 64,576,549** (closed 13:02:03); both old tables end at H. |
| 13:46    | Tail: `fill_presence.zsh … 64500000-64576550`, both tables, gated.                                                                 |
| 13:51    | New code, still paused: Compute from `cb7ea315`, concurrency 0.                                                                    |
| ~13:52   | Swap: `EXCHANGE TABLES` for both. Between 13:51 and the swap the account and asset transaction lists returned errors.              |
| 13:53:36 | Resume: Compute from `cb7ea315`, concurrency 1; the trigger enabled.                                                               |

**Checks before the swap** (read-only): partition 129 old = staging on rows
(54,074,792 and 34,295,186) and on distinct keys; both span 64,500,000–H,
all 76,550 ledgers present. Partitions 100–128: 281 of 281 slices present in
both tables, same first and last ledger; where the row counts differ the gap
is the old table's unmerged duplicates (checked exactly on the two worst
slices: distinct keys equal). The writer against the target DDL: the
production persist path (`persist_e2e`, two more e2e tests) on a local
ClickHouse 26.3 with the merged `init.sql`, plus one direct
`operation_asset_appearances` insert; the row structs equal the staging
tables' `system.columns` on production.

**After the swap:** `transaction_participants` 10.96 bn rows, 20.12 GiB
(was 112.04); `operation_asset_appearances` 11.82 bn, 12.39 GiB (was
100.76). The old tables stay under the `_staging_position` names until
step 7.

**Checks after the resume** (read-only, first ~50 ledgers after H): the
indexer and API Lambdas 0 errors, the DLQ empty; rows after H land only in
the new tables (0 in the old); 0 rows of either table without a transaction
at their position; every transaction's source account is a participant
(15,234 of 15,234); every asset of `asset_transfers` appears in
`operation_asset_appearances` (16,588 of 16,588, as before H: 35,001 of
35,001); the account and asset driver seeks return the new head.

**Old tables dropped the same day** (step 7, ~14:12 UTC, decision karolkow:
drop once the check is exact, no week-long rollback horizon). Before the
drop, `system.query_log` since 2026-09-22 00:00 showed two writers to either
table: `ingestion_writer` — 54,260 inserts against 27,131 ledgers closed in
that time (one per ledger per table), all ≥ 64,549,885, i.e. partition 129,
re-copied after the pause — and the operator's fill (`dev_shared`, reading
the old tables into the staging ones, 09:46–13:46). No backfill or repair
touched the old tables after their slices were copied. A full re-gate over
the old and new tables was stopped for the drop at 110 complete slices
(4.52 bn keys, 0 differences); the hash ↔ position map in `transactions` had
no conflict in the 81 slices checked.

**After the drop:** free disk 447 → 658.70 GiB of 1.72 TiB (+211.7 GiB, at
14:20 UTC).

**Read benchmark on the swapped tables** (first page of 26, best of 3,
driver plus page fetch; staging figures from
[notes/R-fill-and-read-benchmark.md](notes/R-fill-and-read-benchmark.md) in
brackets): hottest account 468.9 MiB, 0.118 s (639.4); mid account 22.8 MiB
(21.9); sparse account 29.4 MiB (30.0); native XLM 318.6 MiB, 0.098 s (516;
production before the change 1451); abUSDC 60.8 MiB (65); sparse asset
21.8 MiB (22). Every list under the gate.

**Execution order, ledger 64,454,000:** an account with four transactions in
the ledger lists them at positions 183, 177, 134, 58 — the old hash order
would have been 134, 183, 177, 58. Native XLM appears in 135 of the ledger's
transactions, each at a position `transactions` holds, listed 188, 186,
185, 184, 182, …

## Design Decisions

### Emerged

1. **The asset list has one source; its second arm is removed** (decision
   karolkow, 2026-09-23). The list was a union of `operation_asset_appearances`
   (arm A) and the activity of the asset's contract — its own contract or its
   SAC (arm B, `soroban_invocations_appearances`, still keyed by the
   surrogate). Measured on production, ledgers 64,400,000–64,401,000, against
   arm A: arm B added 295 of 646 rows (46%) for 37 type-3 tokens and 301 of
   16,733 (1.8%) for 11 SACs — 213 successful calls with no event at all
   (reads such as `balance`), 55 `REFLECTOR` oracle updates, 11 pair
   `swap,sync`, 254 failed SAC calls (a failed transaction emits no transfer
   event), 1 `approve`. Every transfer, mint, burn and clawback is already in
   arm A through the event-derived rows of task 0383, which is what first made
   arm B necessary. So the list is now "an operation names the asset or a
   token event moved it"; the contract's own activity is the contract page's.
   Known gap kept on purpose: failed Soroban calls and `approve` are no longer
   listed, while failed classic operations still are (arm A takes the asset
   from the operation body whatever the result). An interim version of this
   task read arm B from `contract_transactions` instead; it is gone with the
   arm. Side effect: `AssetRow.contract_surrogate_id` and
   `AssetListChRow.contract_id_key` lost their only reader and are removed.
2. **The aggregate follows the page** instead of running beside it: its keys
   are `t.id`, which only the page knows now.
