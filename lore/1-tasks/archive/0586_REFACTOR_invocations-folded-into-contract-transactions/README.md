---
id: '0586'
title: 'REFACTOR: soroban_invocations_appearances folded into contract_transactions — the last surrogate-keyed contract table'
type: REFACTOR
status: completed
related_adr: ['0059', '0060']
related_tasks: ['0538', '0541', '0372', '0487', '0575']
tags: [clickhouse, storage, effort-medium, priority-high]
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/api/src/contracts/queries.rs
  - crates/api/src/transactions/queries.rs
history:
  - date: '2026-09-25'
    status: active
    who: karolkow
    note: >
      Step 5 of epic 0538 for the invocations table (decided 2026-09-23: fold
      into contract_transactions). Chosen next (thread 246 A) as the smallest
      of the three tables still blocking the drop of transactions.id.
  - date: '2026-09-28'
    status: completed
    who: karolkow
    note: >
      Shipped in 3 steps (PRs #506/#507, #512/#513/#515/#516, #519; task 0487
      as #517). Deployed 2026-09-28 14:26 UTC; whole-row comparison 0 / 0 over
      50,450,000–64,663,963; both old tables dropped, +23.40 GiB free disk.
      contract_activity 15.41 GiB, 3.0 bn rows.
---

# Invocations folded into `contract_transactions`

## Summary

`soroban_invocations_appearances` (14.76 GiB, 1.13 bn rows) locates the
transaction by the `transaction_id` surrogate (8.45 GiB at ratio 1.0). Its
presence is a subset of `contract_transactions` (epic 0538, 0 missing pairs on
10,000 ledgers); what only it holds is the caller (`caller_id`,
`caller_contract_id`) and the fold count `amount`. Fold those into a
position-keyed contract table and drop the invocations table. It is one of the
three tables still joining `transactions.id` (with `nft_ownership` and
`nft_ownership_pending`), and it carries the last surrogate cursor of the API
(the contract's Invocations tab, `ChSurrogate`).

## Context (measured 2026-09-25, read-only)

- **Sizes:** `soroban_invocations_appearances` 14.76 GiB: `transaction_id`
  8.45 (ratio 1.0), `caller_id` 4.63, `caller_contract_id` 0.63, `amount`
  0.58, `ledger_sequence` 0.46, `contract_id` 0.04.
  `contract_transactions` 9.32 GiB, 3.0 bn rows, **no codecs**:
  `ledger_sequence` 5.50 (ratio 4.06), `application_order` 3.68 (1.52) —
  task 0575 measured Delta / T64 at 0.77 and 0.97 B/row on the same shape.
- **Readers of the invocations table (API, 196 reads in 14 days):**
  contract list invocation stats (`contracts/queries.rs:368`), contract detail
  stats (`:674`), the Invocations tab driver (`:811`, cursor `ChSurrogate`),
  the transaction page's invocations (`transactions/queries.rs:437`, the
  frontend does not read the field — epic 0538). `contract_transactions`:
  the `/transactions` contract filter (`list_transactions.rs:472`).
- **`caller_contract_id` is not dead:** task 0487 needs it — every read uses
  `caller_id` only, so a contract called by contracts shows 0 unique callers.
- **prices-api:** no `prices_*` user read either table in 14 days.

## Plan

Decided (karolkow, 2026-09-25):

- **Parallel change** (thread 247 A): a new table, filled in ClickHouse, then
  both old tables dropped — not `ADD COLUMN` on `contract_transactions`.
- **No codecs this time** (karolkow): the gain on the two key columns is
  ~4 GiB (_estimate_), not worth it here.
- **Name `contract_activity`** (thread 248 A): `transaction_contracts` was
  one letter-swap from `contract_transactions`, and one of the two ends in a
  `DROP`.
- **Task 0487 rides a separate PR** right after the readers (thread 249 A),
  so the reader PR stays behaviour-preserving and comparable to the old API.
- **No invoked flag:** every invocation row names exactly one caller (0 rows
  without one in the whole table, 0 with both on 64,000,000–64,050,000), so
  "invoked" = a caller is present.
- **The fold count stays, as `invocation_count`** (thread 255, karolkow):
  how many times the transaction called the contract — the execution trace's
  `fn_call`s merged with the auth tree, not the operation count; 0 on a
  touched-only row. Nothing reads it yet, but no other table holds it
  (diagnostic events are not stored), and after the drop it would come back
  only from an S3 re-parse. 64,000,000–64,010,000: 294,841 of 1,978,709
  invoked pairs were called more than once; the fill's counts sum to
  3,835,802 = `sum(amount)` of the invocations table.

Target shape:

```sql
CREATE TABLE contract_activity (
    contract_id        Int64,
    ledger_sequence    Int64,
    application_order  Int16,             -- transaction position
    caller_id          Nullable(Int64),   -- invoked by an account
    caller_contract_id Nullable(Int64),   -- invoked by a contract
    invocation_count   Int32              -- calls in the transaction; 0 = touched only
) ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (contract_id, ledger_sequence, application_order);
```

| PR                                                                                                                    | Attention  | Deploy | Operator                                                                                                              |
| --------------------------------------------------------------------------------------------------------------------- | ---------- | ------ | --------------------------------------------------------------------------------------------------------------------- |
| 1. New table (position key, two caller columns; no codecs, no fold count); the indexer writes it beside both old ones | write path | yes    | before: `CREATE`; after: fill per 10k-ledger slice from `contract_transactions` ⟕ invocations ⋈ `transactions`, gated |
| 2. Readers on the new table; Invocations tab cursor on the position (old cursors 400; `ChSurrogate` leaves the API)   | read path  | yes    | —                                                                                                                     |
| 3. Old tables no longer written                                                                                       | small      | yes    | after: `DROP` ×2                                                                                                      |

## Progress

- **PR 1 (write both)** — branch `feat/0586-contract-activity-dual-write`,
  local: `0b743946` moves the invocation fold and the `contract_transactions`
  derivation out of `stage.rs` (3,036 → 2,949 lines) into
  `stage/contract_activity.rs`; the next commit adds `contract_activity` (row,
  staging, writer, `init.sql` under `contract_transactions`). Checks: unit
  test of the staging (red with the two callers swapped), column order, the
  events e2e writes and reads both callers set and unset on a fresh
  ClickHouse 26.3; `db-clickhouse` 176 tests pass; clippy clean.
- **Fill runbook** — [`fill_contract_activity.zsh`](notes/fill_contract_activity.zsh)
  with [`fill_contract_activity.sql`](notes/fill_contract_activity.sql),
  [`gate_contract_activity.sql`](notes/gate_contract_activity.sql),
  [`check_fill_matches_live.sql`](notes/check_fill_matches_live.sql). Slices
  of **10,000** ledgers: the fill SELECT over 50,000 exceeded the 3.73 GiB
  memory cap. Read-only on production, 64,000,000–64,010,000: 2,915,824 rows
  = `contract_transactions` keys; 1,978,709 with a caller = invocation keys;
  0 with both callers; 17.4 M rows read, 0.74 s, 1.1 GiB. Loop dry-run with
  stubbed `chw` / `chq`: pass, gate mismatch, bad resume point, low disk.

- **Review of PR 1** (standards + spec, 2026-09-25): no struct / DDL
  mismatch, the move is pure, fill = live writer for the same ledgers.
  Fixed: the fill takes the caller pair with one `any()` (two unmerged copies
  with different callers could otherwise combine into a row with both set),
  the gate checks that no row has both callers, the e2e now writes a
  contract caller too, a test pins that a second invocation's caller does not
  replace the first, `contract_rows` → `rows`. For the PR that stops the old
  writes: `scripts/merge-*.sh` list neither contract table — add
  `contract_activity` there, as 0372 did.

- **PRs opened** (2026-09-25): the move alone in
  [#506](https://github.com/rumblefishdev/soroban-block-explorer/pull/506)
  (`refactor/0586-move-contract-staging`, 95 lines moved, 34 of glue), the
  table and dual write in
  [#507](https://github.com/rumblefishdev/soroban-block-explorer/pull/507),
  stacked on it (draft; retargets to `develop` after #506 merges). Split
  because a PR that moves code and changes logic reads all green in GitHub's
  diff (global rule, `move-split-guard`).

- **PR 1 deployed** (2026-09-25): `contract_activity` created by hand, then
  Compute (indexer and API Lambdas 16:06:03 UTC). The indexer writes it from
  ledger **64,613,318**; 0 writer or API errors. Whole-row check of the fill
  SELECT against the live rows on 64,613,318–64,613,348 (`check_fill_matches_live.sql`):
  0 / 0; gate: 4,943 = 4,943 pairs, 2,607 = 2,607 invoked, 0 both, 0 count
  mismatches (768 pairs called more than once, 7,788 calls). The same deploy
  shipped task 0497's writer (the `minted_at_ledger` defaults were already
  on production).

- **Fill complete** (2026-09-25, 16:10–16:50 UTC): every slice gated;
  coverage over all 30 partitions — 2,997,455,423 pairs = the
  `contract_transactions` keys, 1,109,006,747 invoked = the invocation keys.

- **PR 2 (readers) opened** (2026-09-25): the move alone in
  [#512](https://github.com/rumblefishdev/soroban-block-explorer/pull/512)
  (`fetch_invocation_appearances` into `contracts/queries/list_invocations.rs`,
  `contracts/queries.rs` 1165 → 995 lines, 172 moved), the readers in
  [#513](https://github.com/rumblefishdev/soroban-block-explorer/pull/513),
  stacked (draft). Local API on production ClickHouse against the deployed
  API: detail stats same for 6 contracts, list counts 50/50, Invocations tab
  same rows for 4 contracts over whole ledgers, old cursor 400, transaction
  page 18/18, `/transactions` contract filter same for 6. The tab's first
  driver put `LIMIT 1 BY` beside `LIMIT`, which disables the read-in-order
  early stop (21.7 M rows vs 0.59 M); `LIMIT` moved into a subquery →
  4.37 M rows / 78 ms vs 0.54 M / 32 ms, the rest from 199 unmerged parts
  after the fill (6.6 per partition vs 1.9), left to background merges.

- **Review of PR 2** (standards + spec, 2026-09-28): #512 merged, #513
  retargeted to `develop`. No defect in the readers: every API read moved,
  `invocation_count > 0` on the four invocation readers and not on the
  `/transactions` filter, dedup kept, unique callers still `caller_id` only
  (249 A). Fixed: `contracts/queries.rs` had grown 995 → 999 (now 993),
  a comment pointed at deleted SQL, the schema overview / pilot / pipeline
  docs still named the old readers. Behaviour change to state in the PR:
  within one ledger the Invocations tab now lists by execution order, not by
  the hash surrogate. Open: `TxListCursor::ChSurrogate` still decodes only
  to be refused — dropping it makes the three list guards dead; names
  `fetch_invocation_appearances` / `InvocationAppearanceRow` describe the
  retired table.

- **#513 merged** (2026-09-28, 07:17 UTC) before the review fixes were
  pushed; they follow in
  [#515](https://github.com/rumblefishdev/soroban-block-explorer/pull/515)
  with the `ChSurrogate` variant removed (thread 273 B): a surrogate cursor
  now fails to decode (400 `invalid_cursor` from the extractor) and the four
  per-list guards go. The rename of the "appearances" names rides its own
  PR before task 0487 (thread 274 A).

- **#515, #516 merged** (2026-09-28); the rename landed as
  [#516](https://github.com/rumblefishdev/soroban-block-explorer/pull/516).

- **PR 3 (stop the old writes)** — branch `feat/0586-stop-old-contract-writes`,
  local: `311d9435` — the writer, staging and `init.sql` drop
  `soroban_invocations_appearances` and `contract_transactions`; the fold keys
  its rows by position straight from the ledger's own order
  (`app_order_by_hash`), so staging maps no surrogate; allowlist without the
  invocations table; `contract_activity` in `scripts/merge-*.sh`. `2a6b24d8` —
  schema overview §4.5.6 rewritten for `contract_activity` alone, pilot,
  pipeline, README, backfills ("not repeatable after step 3"), deployment
  step 3 with the drops, crash-recovery and cutover runbooks, backups.
  Checks: workspace clippy clean; `db-clickhouse` all tests pass on the local
  ClickHouse 26.3 (smoke and the events e2e write `contract_activity`);
  indexer, backfill-runner, xdr-parser, domain 645 tests pass. PR only after
  the step-2 deploy is verified in `query_log`.
- **prices-api check** (2026-09-28, read-only, `system.query_log`, 14 days):
  no `prices_*` user read either table. Readers were `api_reader` (last
  2026-09-26, before the step-2 deploy), `dev_read` / `dev_shared` (this
  task's checks), `default` (3, 2026-09-21) and `ingestion_writer` (writes).

- **PR 3 opened and merged** (2026-09-28):
  [#519](https://github.com/rumblefishdev/soroban-block-explorer/pull/519),
  merged 08:48 UTC. It ships in the same deploy as #513 (thread 288 A):
  rollback only to a commit that includes #513.

- **Whole-row comparison before the drops** (2026-09-28, read-only,
  `check_fill_matches_live.sql` over every 10k-ledger slice,
  50,450,000–64,659,000): 1,421 slices, **0 rows only in the old tables, 0
  only in `contract_activity`**, 65 min. Holds until the deploy of #519; the
  ledgers from 64,659,000 to that deploy are compared the same way right after
  it, before the drops.

- **Steps 2 and 3 deployed** (2026-09-28): Compute from `develop` `f1e53c45`
  (#513, #515, #516, #517, #519), indexer and API Lambdas 14:26:33 UTC, SPA
  14:29:16 UTC. The old writer's last insert into both old tables was
  14:26:34 UTC; their head stays at ledger **64,663,962**. After the deploy:
  0 query exceptions, the three DLQs empty, `api_reader` ran 498 queries and
  none read either old table (last reads 08:46 UTC and 2026-09-25).
  KALE SAC detail stats from the deployed API: 70 unique callers = 70 by the
  new formula in ClickHouse (62 by the old, `caller_id` only); the Invocations
  tab lists the contract caller `CDL74RF5…`.
- **Tail comparison** (read-only, `check_fill_matches_live.sql` on
  64,659,000–64,663,963): **0 / 0** on 1,414,449 rows each side; 0
  `contract_transactions` rows past 64,663,962.
- **prices-api check before the drops** (2026-09-28 14:35 UTC,
  `system.query_log`, 14 days): no `prices_*` user read either table. Sizes:
  `soroban_invocations_appearances` 14.87 GiB, `contract_transactions`
  9.36 GiB (drop saving 24.23 GiB); `contract_activity` 15.41 GiB.
- **Both old tables dropped** by the operator (2026-09-28, ~14:36 UTC);
  `system.tables` 0, ingest kept pace (0 write or read exceptions). The
  server removes the files 480 s after the drop
  (`database_atomic_delay_before_drop_table_sec`): free disk 822.08 GiB
  before, 845.48 GiB at 14:44:58 UTC — **+23.40 GiB** (other tables grow
  meanwhile; the parts measured 24.23 GiB).

## Acceptance Criteria

- [x] New table filled and gated in every partition; whole rows compared
      before each drop (1,421 slices + the tail, 0 / 0)
- [x] API reads only the new table (`query_log`: 0 `api_reader` reads of
      either old table after the 2026-09-28 deploy)
- [x] `soroban_invocations_appearances` and the old `contract_transactions`
      dropped; saving measured (+23.40 GiB free disk)
- [x] `schema_conventions` allowlist without `soroban_invocations_appearances`
      (#519)
- [x] prices-api check recorded before each drop
- [x] **Docs updated** — schema overview, pilot, pipeline, deployment,
      backfills, runbooks (#513, #519). Canonical SQL: N/A — the set was
      retired by ADR 0060 / task 0588 before the readers merged.

## Design Decisions

### From Plan

1. **Parallel change, new table name `contract_activity`** (threads 247 A,
   248 A): no swap window, every step an ordinary deploy.
2. **No codecs this time**; ~4 GiB (_estimate_) not worth the extra step.
3. **Task 0487 in its own PR after the readers** (249 A), so the reader PR
   stayed comparable to the old API.

### Emerged

4. **Fold count kept as `invocation_count`** (thread 255): nothing reads it,
   but no other table holds it and only an S3 re-parse could restore it.
5. **Surrogate cursor removed, not tolerated** (273 B): an old
   `ch_surrogate` cursor fails to decode (400) instead of being refused per
   list, which removed four guards.
6. **Invocations tab driver puts `LIMIT` in a subquery**: `LIMIT 1 BY`
   beside `LIMIT` disables the read-in-order early stop (21.7 M rows vs
   0.59 M).

## Issues Encountered

- **#513 merged before its review fixes were pushed** — they followed in
  #515. Not a regression.
- **Fill SELECT over 50k ledgers exceeded the 3.73 GiB cap** — slices of
  10k.
- **Drop saving shows only after 480 s** (`database_atomic_delay_before_drop_table_sec`).
