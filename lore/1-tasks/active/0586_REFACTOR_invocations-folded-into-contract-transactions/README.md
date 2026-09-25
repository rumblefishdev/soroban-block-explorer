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
- **No invoked flag, no fold count:** every invocation row names exactly one
  caller (0 rows without one in the whole table, 0 with both on
  64,000,000–64,050,000), so "invoked" = a caller is present; nothing reads
  the fold count (`amount`) — the stats count rows.

Target shape:

```sql
CREATE TABLE contract_activity (
    contract_id        Int64,
    ledger_sequence    Int64,
    application_order  Int16,             -- transaction position
    caller_id          Nullable(Int64),   -- invoked by an account
    caller_contract_id Nullable(Int64)    -- invoked by a contract
) ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (contract_id, ledger_sequence, application_order);
```

| PR                                                                                                                  | Attention  | Deploy | Operator                                                                                                              |
| ------------------------------------------------------------------------------------------------------------------- | ---------- | ------ | --------------------------------------------------------------------------------------------------------------------- |
| 1. New table (position key, codecs, caller columns, fold count); the indexer writes it beside both old ones         | write path | yes    | before: `CREATE`; after: fill per 50k-ledger slice from `contract_transactions` ⟕ invocations ⋈ `transactions`, gated |
| 2. Readers on the new table; Invocations tab cursor on the position (old cursors 400; `ChSurrogate` leaves the API) | read path  | yes    | —                                                                                                                     |
| 3. Old tables no longer written                                                                                     | small      | yes    | after: `DROP` ×2                                                                                                      |

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
