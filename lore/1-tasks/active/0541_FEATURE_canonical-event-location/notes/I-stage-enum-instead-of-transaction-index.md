---
title: 'A three-value stage instead of transaction_index in soroban_events'
type: idea
status: seed
spawned_from: notes/S-review-and-contract-transactions.md
spawns: []
tags: [clickhouse, storage, soroban-events, idea]
links:
  - crates/db-clickhouse/schema/init.sql
history:
  - date: 2026-09-22
    status: seed
    who: karolkow
    note: 'Raised after the swap; kept for the next rebuild of soroban_events, not worth one of its own'
---

# A three-value stage instead of transaction_index

## The idea

`transaction_index` differs from `application_order` on two kinds of row only:
the fee charge (0) and the refund after protocol 23 (1048575). On every
operation event, and on the refund before protocol 23 (told apart by
`operation_index = 4095`), it repeats the transaction's position.

A stage column with three values carries the same information:

| stage              | code | rows                                                |
| ------------------ | ---- | --------------------------------------------------- |
| `charge`           | 0    | the fee charge                                      |
| `in_tx`            | 1    | operation events, and the refund before protocol 23 |
| `refund_after_all` | 2    | the refund after protocol 23                        |

Sort key `(contract_id, ledger_sequence, stage, application_order,
operation_index, event_index)`:

- **Same row order as today.** Charges first, the transactions in order, the
  refunds after all of them — ordered by position, which is the order of their
  rank. The stored order does not change, so no other column compresses
  differently.
- **Unique.** One charge and at most one refund per transaction; the rest are
  told apart by operation and position.
- **The rpc id is derived on read.** Transaction part: 0, the position or
  1048575 by stage; operation and event parts as stored.

## What it would save

`transaction_index` costs 0.634 B/row (partition 127, phase 1), about 6.3 GiB
over 10.68 bn rows. A sorted three-value column compresses to almost nothing.
Net about 6 GiB — ~3% of the table, ~0.5% of the database (_estimate_).

## Why not now

A sort-key column cannot be dropped by `ALTER`, so it is a rebuild of the whole
table (195 GiB, ~8 h of fill, both copies on disk) and a second window with the
indexer paused. The writer, the readers, the event cursor (another 400 for old
cursors) and ADR 0059 — which stores the id literally, as `getEvents` returns
it — would all change. Not worth it for 6 GiB.

## When

Fold it into the next rebuild of `soroban_events` that happens for another
reason.
