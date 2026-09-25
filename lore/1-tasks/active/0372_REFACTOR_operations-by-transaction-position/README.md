---
id: '0372'
title: 'REFACTOR: operations and LP amounts located by transaction position — drop transaction_id, the dead operation_pools, and the unread fold count'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0538', '0575', '0580', '0365', '0491', '0268', '0261', '0281']
tags: [clickhouse, storage, effort-large, priority-high]
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/api/src/transactions/queries.rs
  - crates/api/src/transactions/dto.rs
  - crates/db-clickhouse/src/persist/stage.rs
history:
  - date: 2026-07-09
    status: active
    who: stkrolikiewicz
    note: >
      Spawned from 0365. Once lptxs seeks `operation_pools`, the `pool_ids` array on
      `operations_appearances` serves only the op→pool direction — the `pool_ids`
      field on the transactions response — which the `web/` frontend does NOT consume
      (verified: `operationEntries.ts:56` stubs `[]`; grep finds no `.pool_ids` read).
      Dropping the column reclaims more disk than `operation_pools` costs (net smaller)
      and removes dead ingestion work. Sequence POST-M3, strictly after 0365 ships.
  - date: '2026-07-22'
    status: active
    who: karolkow
    note: >
      **Unblocked — `blocked_by: ['0365']` removed, because 0365 shipped.**
      Verified in the code rather than from the task's archive location:
      `crates/api/src/liquidity_pools/queries.rs:592` now reads "STEP 1 —
      leading-key seek over `operation_pools` (task 0365)", and the surrounding
      doc comment records the prefix-seek replacing the old driver. So the
      condition this task waited on — the pool→op direction moving off
      `pool_ids` — is satisfied.
      Nothing else about the task changed: the remaining reader is still the
      op→pool direction on the transactions response, which the frontend does
      not consume. Still `milestone-4` / post-launch by choice, not by blocker.
  - date: '2026-09-25'
    status: active
    who: karolkow
    note: >
      Widened (decision karolkow, 2026-09-24/25) into step 5 of epic 0538 for
      the operation tables. The premise "the frontend does not read pool_ids"
      is stale: the transaction page renders "Pools crossed" from it and pool
      activity reads pools_crossed — pool_ids stays. operation_pools lost its
      last reader in task 0491 and is dropped instead. Shipped as a parallel
      change, no window.
---

# Operations and LP amounts located by transaction position

## Summary

`operations_appearances` (103.3 GiB) and `lp_operation_amounts` (11.6 GiB)
locate their transaction by `transaction_id`, a hash surrogate that
compresses at ~1.0 (33.3 + 4.5 GiB). Rebuild both keyed by the transaction
position `(ledger_sequence, application_order)` and the operation's
`operation_index` (ADR 0059), without `transaction_id`, and without the
unread fold count `amount` on the operations table. Drop `operation_pools`
(6.96 GiB), which nothing has read since task 0491. Saving ~46 GiB
(_estimate_); three of the six `transaction_id` holders in the
`schema_conventions` allowlist go.

## Context (measured 2026-09-24, read-only)

- **`operations_appearances`:** one row per _folded_ identity tuple (tx, type,
  source, destination, contract, asset, `pool_ids`), not per operation;
  `application_order` is the smallest 1-based operation index in the group;
  `amount` is how many operations folded (1,000 ledgers from 64,000,000:
  794,200 operations in 586,050 rows, 10.9% of rows folded, max 60). No code
  or doc SQL reads `amount`. The transaction position is at hand in the
  writer (`stage.rs` `app_order_by_hash`).
- **Readers (7 API + 1 ops):** the operation-type aggregates of the account,
  asset, ledger and `/transactions` lists (`common/ch.rs`); the `/transactions`
  op-type filter (statements B and C, cursor `ChSurrogate`); the transaction
  page's operations (`fetch_operations`, reads `pool_ids`); pool activity
  (`list_pool_activity.rs`, reads OA and `lp_operation_amounts`, joins
  `t.id`); `repair-tier1` (`arrayJoin(pool_ids)`).
- **`pool_ids` stays** (decision karolkow): "Pools crossed" on the transaction
  page (`OperationCard.tsx`) and `pools_crossed` in pool activity read it,
  and no other table maps an operation to its pools.
- **`operation_pools`:** no API reader since 0491 (pool activity drives from
  `lp_operation_amounts`); `query_log` 14 days: backups, ad-hoc research and
  the writer only.
- **Review of PR 2** (standards + spec): no struct/DDL mismatch; live
  writer and fill produce the same rows. Fixed: a 0 operation position now
  fails the ledger (`checked_sub`), false comments about `operation_pools`,
  `stage.rs` growth, the pipeline and `--only` docs. The spec review noted the
  gate counts keys only, so a uniform shift would pass:
  [`check_fill_matches_live.sql`](notes/check_fill_matches_live.sql) compares
  whole rows of the fill against the live-written ones on the first
  dual-written slice, before the fill. Deferred: moving the 4,891-line
  sibling `tests_cross.rs` (task 0525).
- **prices-api:** no `prices_*` user read any of the three tables in 14 days.

## Target shape

```sql
CREATE TABLE transaction_operations (
    ledger_sequence   Int64,
    application_order Int16,          -- transaction position
    operation_index   Int16,          -- 0-based; smallest of the folded group
    type              Int16,
    source_id         Nullable(Int64),
    destination_id    Nullable(Int64),
    contract_id       Nullable(Int64),
    asset_code        LowCardinality(String),
    asset_issuer_id   Nullable(Int64),
    pool_ids          Array(FixedString(32))
) ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (ledger_sequence, application_order, operation_index);
```

`lp_operation_amounts` gets the same treatment under a new name:
`(pool_id, ledger_sequence, application_order, operation_index, asset_id)`.
Codecs and skip indexes are settled in PR 2 (the pool bloom is dead; the
contract bloom stays only if a reader still filters by contract).

## Plan — parallel change (agreed 2026-09-25)

| PR                                                                                                                      | Attention                   | Deploy | Operator                                                                                                                               |
| ----------------------------------------------------------------------------------------------------------------------- | --------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Moves: the operations block out of `stage.rs` (3,355 lines), the list query out of `transactions/queries.rs` (1,103) | mechanical, `--color-moved` | no     | merge                                                                                                                                  |
| 2. Both new tables, the indexer writes old and new; `operation_pools` no longer written                                 | write path                  | yes    | before: `CREATE` ×2; after: `DROP operation_pools`, history fill per 50k-ledger slice (join `transactions` on `(ledger_sequence, id)`) |
| 3. Readers on the new tables; op-type cursor on the position (old cursors 400)                                          | read path                   | yes    | —                                                                                                                                      |
| 4. Old tables no longer written; `pool_operation_amounts` joins the `--only` list in place of `lp_operation_amounts`    | small                       | yes    | after: `DROP` ×2                                                                                                                       |

PR 3 is written while the fill runs.

## Progress

- **PR 1 (moves)** — [#498](https://github.com/rumblefishdev/soroban-block-explorer/pull/498), merged. `transactions/queries.rs` 1,103 → 482 lines (list →
  `queries/list_transactions.rs`, 635); `stage.rs` 3,355 → 3,210 (operation
  staging → `stage/operations.rs`, 180). Only glue is new: imports, module
  lines, the wrapping signature. Checks: fmt, workspace clippy
  `-D warnings`, `api` + `db-clickhouse` 824 tests, CH-gated
  `db-clickhouse` 174 on a fresh ClickHouse 26.3; `api` decode smoke 21/25 —
  the 4 pool tests need pool rows an empty database lacks, as on `develop`.

- **PR 2 (write both)** — branch `feat/0372-transaction-operations-dual-write`,
  commits `56dfb092` (code), `714a9483` (docs). New tables:

  - `transaction_operations` (codecs Delta / T64);
  - `pool_operation_amounts`;
  - both skip indexes of the old table left behind: no reader filters by pool
    or contract since 0491 / 0541.

  `operation_pools` is no longer written. Checks:

  - `persist_e2e` drives a pool deposit through the real writer and reads
    both twins back; verified red with the operation index left 1-based;
  - workspace clippy clean; 952 unit and 175 CH-gated tests pass.

- **Fill runbook** — [`fill_operations.zsh`](notes/fill_operations.zsh) with
  [`fill_transaction_operations.sql`](notes/fill_transaction_operations.sql),
  [`fill_pool_operation_amounts.sql`](notes/fill_pool_operation_amounts.sql),
  [`gate_operations.sql`](notes/gate_operations.sql). Loop dry-run with stubbed
  `chw`/`chq`: 6 fills and 48 gate queries for 3 slices. Read-only on
  production, 64,000,000–64,050,000:
  - the operations join reads 42.9 M rows in 1.2 s (2.2 GB);
  - quarter gate 6,214,520 = 6,214,520 keys;
  - amounts 7,911,730 = 7,911,730 keys.
- **Old and new side by side in `init.sql`** (decision karolkow, 2026-09-25):
  each new table sits directly under the one it replaces, with a comment
  saying so; the old block leaves in PR 4. A variant with the new definition
  in place of the old and the old in a separate `schema/transitional.sql`
  (applied by `apply_init_sql` and the compose sidecars) was tried
  (`6a8c05d7`) and reverted (`03574f2a`): too much new plumbing for a PR that
  only adds tables.
- **prices-api:** no `prices_*` user read `operations_appearances`,
  `operation_pools` or `lp_operation_amounts` in 14 days (checked 2026-09-25).
- **History fill done** (2026-09-25, 11:08–12:39 UTC): `fill_operations.zsh`
  over partitions 100–128 and the head range 64,500,000–64,609,690 passed
  every quarter gate; ClickHouse free space 784 → 703 GiB. Check after it:
  per-partition rows of `transaction_operations` = `operations_appearances`
  in all of 100–129; `pool_operation_amounts` = `lp_operation_amounts` except
  101, 109, 110, where the old table holds unmerged duplicates and distinct
  keys are equal (30,715,812 / 42,291,832 / 32,297,549).
- **`operation_pools` dropped** (Karol, after the fill; gone from
  `system.tables` at 12:44 UTC).
- **PR 3 deployed** (API Lambda 2026-09-25 12:59:01 UTC). Through the dev
  proxy: op-type cursor `ch_position`, pool activity cursor carries
  `operation_index`, transaction page operations numbered from 1.
  `query_log` 12:59:30–13:01 UTC: `api_reader` 474 queries, 0 errors, 50
  reads of `transaction_operations`, 1 of `pool_operation_amounts`, 0 of the
  old tables; only `ingestion_writer` still writes them.
- **PR 4 (stop old writes)** — [#502](https://github.com/rumblefishdev/soroban-block-explorer/pull/502), merged.
  Pre-drop check, 2026-09-25 13:18 UTC, `query_log` 14 days: no `prices_*`
  user read `operations_appearances` (103.42 GiB, 7.04 bn rows) or
  `lp_operation_amounts` (11.65 GiB, 994 M rows); `api_reader`'s last read of
  either was 12:59:02 UTC, the PR 3 deploy; the rest are `ingestion_writer`,
  the fill and gates (`dev_read`, `dev_shared`) and the 09-21 backup
  (`default`).
- **Pre-drop comparison** (2026-09-25 13:30–13:37 UTC, read-only), after the
  PR 4 deploy (indexer 13:25:34; old tables stop at ledger 64,611,391):
  - range: both pairs 50,457,424–64,611,391, 30 partitions each;
  - operations: rows per partition equal in all 30 — 7,043,648,565 each (the
    fill copied rows 1:1, duplicates included);
  - amounts: distinct keys per partition equal in all 30 — 982,202,932 each;
  - whole rows, both directions (`EXCEPT DISTINCT`, old mapped to position
    through `transactions`): 0 differences on a 12,500-ledger slice of every
    partition 100–129 and on the whole dual-written tail 64,609,690–64,611,391
    — 184.8 M operation rows compared.
- **Old tables dropped** (Karol, 2026-09-25 ~13:38 UTC):
  `operations_appearances` (`max_table_size_to_drop = 0`) and
  `lp_operation_amounts`. Ingest unaffected (head and `transaction_operations`
  both at 64,611,649 at 13:47, 0 writer or API errors since 13:37). ClickHouse
  free space 726.72 → 841.65 GiB after the 8-minute delay (+114.9 GiB against
  103.42 + 11.65 GiB of tables).
- **Saving, measured 2026-09-25 13:47 UTC:** old 122.04 GiB
  (`operations_appearances` 103.42, `lp_operation_amounts` 11.65,
  `operation_pools` 6.97) → new 65.19 GiB (`transaction_operations` 59.10,
  `pool_operation_amounts` 6.09; 230 and 162 parts, not fully merged yet):
  **56.85 GiB** net, against the ~46 GiB estimate.
- **PR 3 (readers)** — branch `feat/0372-readers-by-position`, commits
  `0df844ad` (code), `bc9dc95c` (docs); [#500](https://github.com/rumblefishdev/soroban-block-explorer/pull/500), merged. Every API reader and
  `repair-tier1` read the new tables; `/transactions` pages on the position
  under every filter (statement C: positions from `transaction_operations`,
  then statement B's page seek); pool activity's cursor is
  `(ledger_sequence, application_order, operation_index)`; the wire keeps the
  operation's 1-based position. Old cursors of both answer 400 once.
  Deploy only after the fill is gated in every partition, head included.
  Checks, 2026-09-25, local API on the branch against production vs the
  deployed API:
  - 14 of 14 transaction pages: identical operations, folded ones included;
  - lists (`/transactions` unfiltered, 6 op types, contract + op type,
    4 accounts, 6 assets, 2 ledgers): 0 differences outside ranges the fill
    had not reached (there: empty `operation_types`, as expected); within the
    oldest ledger of a full op-type page the cut differs, because the old
    statement C ordered a ledger's transactions by hash;
  - pool activity, 6 pools × {all, trade}: identical on the range both tables
    cover;
  - read cost on partition 115 (filled in both): statement C driver 63–147 M
    rows vs 24–147 M old, pool driver 0.57–0.71 M vs 0.25–0.29 M — the new
    tables still held the fill's 10 unmerged parts per partition (old: 1).
  - clippy `-D warnings`, fmt, `api` + `backfill-runner` tests pass except
    the 4 pool/search decode smokes that need pool rows (empty ClickHouse,
    as on `develop`) and `pool_reserves_reconciliation` (task 0374: one
    Soroban pool `CAZ6W4…` holds `[263512715771, 131948815702]`, chain
    `[0, 131948815702]`; untouched by this PR).
  - Known, unchanged: an operation folded into an earlier identical one has
    no `transaction_operations` row of its own, so its pool-activity row
    falls back to the transaction's source and has no `pools_crossed`.

## Acceptance Criteria

- [x] New tables filled and gated in every partition
- [x] API reads only the new tables (`query_log`: 0 reads of the old ones)
- [x] `operations_appearances`, `lp_operation_amounts` (old), `operation_pools`
      dropped; saving measured
- [x] `schema_conventions` allowlist shorter by three tables
- [x] prices-api check recorded before each drop
- [ ] **Docs updated** — schema overview, pilot, canonical SQL 02/03/07/10/18/24,
      runbooks and merge scripts that name the tables — all done in PRs 3–4
      (SQL 18 never named them) except the one-off historical runbooks
      (PG→CH cutover, 0225, 0228, the 0365 re-parse example) and the PG mirror
      script, left with the old names on purpose; open until Karol decides

## Superseded scope (2026-07, kept for the record)

The task was filed to drop `pool_ids` on the premise that the frontend did not
read it. It does (see Context), so `pool_ids` stays in the new table.

### PR 3 carries a pre-existing `repair-tier1` defect onto the new table (found 2026-09-25, task 0468)

The `lp_positions` rebuild matches deposits on the operation's `source_id`,
which is NULL when the operation has no source of its own — the depositor is
then the transaction's source. `transaction_operations` keeps the same
semantics (312,892 of 458,856 type-22 rows NULL so far). A LEFT JOIN miss
arrives as `0`, not NULL, so the rebuild wrote `0` over 102,693 positions on
2026-07-16. PR 3 moves the query as-is; the join needs
`coalesce(op source, transaction source)` and a miss must keep the stored
value — or the entry retires with task 0468's storage fix. Until one of the
two lands, a `repair-tier1` run re-zeroes those positions.
