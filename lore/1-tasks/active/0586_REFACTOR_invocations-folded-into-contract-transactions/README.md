---
id: '0586'
title: 'REFACTOR: soroban_invocations_appearances folded into contract_transactions — the last surrogate-keyed contract table'
type: REFACTOR
status: active
related_adr: ['0059']
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

## Acceptance Criteria

- [ ] New table filled and gated in every partition; whole rows compared
      before each drop
- [ ] API reads only the new table (`query_log`)
- [ ] `soroban_invocations_appearances` and the old `contract_transactions`
      dropped; saving measured
- [ ] `schema_conventions` allowlist without `soroban_invocations_appearances`
- [ ] prices-api check recorded before each drop
- [ ] **Docs updated** — schema overview, canonical SQL (contracts,
      transaction page, `/transactions` contract filter), deployment, backfills
