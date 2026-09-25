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
| 4. Old tables no longer written                                                                                         | small                       | yes    | after: `DROP` ×2                                                                                                                       |

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
- **prices-api:** no `prices_*` user read `operations_appearances`,
  `operation_pools` or `lp_operation_amounts` in 14 days (checked 2026-09-25).

## Acceptance Criteria

- [ ] New tables filled and gated in every partition
- [ ] API reads only the new tables (`query_log`: 0 reads of the old ones)
- [ ] `operations_appearances`, `lp_operation_amounts` (old), `operation_pools`
      dropped; saving measured
- [ ] `schema_conventions` allowlist shorter by three tables
- [ ] prices-api check recorded before each drop
- [ ] **Docs updated** — schema overview, pilot, canonical SQL 02/03/07/10/18/24,
      runbooks and merge scripts that name the tables

## Superseded scope (2026-07, kept for the record)

The task was filed to drop `pool_ids` on the premise that the frontend did not
read it. It does (see Context), so `pool_ids` stays in the new table.
