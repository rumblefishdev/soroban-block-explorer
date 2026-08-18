---
id: '0491'
title: 'FEATURE: pool activity is a list of operations, with a trades filter'
type: FEATURE
status: active
related_adr: ['0032']
related_tasks: ['0279', '0482', '0489', '0490']
tags:
  [
    api,
    frontend,
    layer-backend,
    layer-frontend-pages,
    priority-medium,
    effort-medium,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/371'
history:
  - date: '2026-08-17'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from the 0279 release review, and deliberately scoped to hold
      BOTH remaining halves of issue #371 — the row unit and the trades filter
      — because they need the same API change and splitting them would pay for
      that migration twice.
  - date: '2026-08-17'
    status: active
    who: stkrolikiewicz
    note: >
      Activated, with the sequencing decided at the same time. 0490 is NOT done
      first — this task's AC already requires its cap to be removed or proven
      dead, so patching the row height beforehand writes code only to delete it.
      0279 stays open deliberately: its remaining data-side criteria (multi-pool
      Horizon check, `read_rows` measurement) validate figures this task merely
      re-presents, while its "Docs updated" and "API types regenerated" items
      are left for this task to satisfy — the response shape changes here, so
      doing them under 0279 would mean writing them twice.
---

# Pool activity is a list of operations, with a trades filter

## Summary

Change the unit of a "Recent transactions" row on the pool detail page from a
transaction to an operation, and add the trade / deposit / withdrawal filter
issue #371 asked for. One change, because the filter cannot be built honestly
on the current row unit.

## Context

[0279](../active/0279_FEATURE_lp-op-details-amount-column.md) answered the
first third of issue #371 — the amounts are on the list now, no detail-page
hop. Two thirds are open, and both are blocked on the same thing.

The reporter's ask was to match stellar.expert's pool view, and pointed at
`?filter=trades`. That view lists **trades**, not transactions holding trades.

Everything awkward about the current table traces back to the row unit:

- **The `Event` chip cannot be honest.** `classifyLpTx(row.operation_types)`
  collapses a transaction's operation types into one chip; a bundled deposit +
  trade gets a chip that is wrong for one of them.
- **The Amount cell has to stack** (see
  [0490](./0490_BUG_pool-amount-cell-row-height-unbounded.md)) because one row
  holds several figures that must not be summed.
- **A trades filter is not expressible.** What does "trades only" return for a
  transaction that deposits and trades? Every answer is a lie: include it and
  the list is not trades, exclude it and a real trade vanishes.
- **The count in the pager means transactions**, which is not the number the
  page is about.

Per-operation rows dissolve all four. And the identity that makes it navigable
already shipped: [0482](../archive/0482_BUG_op-selection-url-state-ownership.md)
gave every operation a URL-addressable `#op-N` anchor on the transaction detail
page, so each row has a real destination.

## Implementation

### Step 1: API — the page is operations

`/liquidity-pools/:id/transactions` returns one item per (transaction,
operation) against the pool, with the cursor keyed on
`(ledger_sequence, transaction_id, application_order)` rather than on the
transaction. `lp_operation_amounts` and `operation_pools` are already keyed
that way, so this removes the per-transaction grouping rather than adding
work. Decide at this point whether the path gets a new name — `/activity`
reads truer than `/transactions` — and whether the old shape needs a
deprecation window.

### Step 2: API — the filter

`filter[event]` over operation type: trades, deposits, withdrawals. One
predicate on a per-operation row.

### Step 3: Frontend

One line per row, so the Amount cell stops stacking and the `Event` chip
becomes accurate by construction. Row links to the operation's `#op-N` anchor.
Filter control on the section header.

## Acceptance Criteria

- [ ] One row per operation; `Event` chip describes exactly that operation
- [ ] Amount cell renders a single figure — the stacking case is gone, not
      merely capped
- [ ] Each row links to its operation's `#op-N` anchor on the transaction
      detail page
- [ ] `filter[event]` returns trades / deposits / withdrawals, and the mixed
      bundle that motivated this appears under each type it actually contains
- [x] Pagination is stable across the new cursor, including at a page boundary
      that falls inside a multi-operation transaction — verified on prod, see
      [Verification](#verification-on-prod-2026-08-18)
- [x] Read path stays a PK-prefix seek with the same partition prune — measure
      `read_rows` before and after; more rows per page must not mean a scan —
      measured, and it caught a regression; see below
- [ ] 0490's cap is removed or confirmed dead, not left as unreachable code
- [ ] **Docs updated** — per ADR 0032, the endpoint contract and the canonical
      SQL under `docs/architecture/**`
- [ ] **API types regenerated** — required, the response shape changes

## Verification on prod (2026-08-18)

Run against `sorban-prod` / `app-clickhouse-1` as read-only SELECTs, on the
pool with the most recent activity (`7a042a04…0e6e`, 1.68M leg rows).

### read_rows — the measurement rejected the first implementation

Returning 21 operations. **Medians of 3**: a cold run of _either_ shape reads
0.7–1.0M rows, so one sample each inverts the comparison.

| shape                                            | read_rows   | ms    | memory     |
| ------------------------------------------------ | ----------- | ----- | ---------- |
| `GROUP BY` pivot (first cut)                     | 2 597 380   | 109   | 182 MiB    |
| + `optimize_aggregation_in_order`                | 2 597 297   | 253   | 230 MiB    |
| + `FINAL`                                        | 3 174 852   | 110   | 195 MiB    |
| **read-in-order + pair in Rust (shipped)**       | **114 888** | **9** | **11 MiB** |
| per-transaction endpoint 20 (what this replaces) | 159 021     | 11    | 11 MiB     |

The first implementation was a **regression** against the endpoint it
replaces: a `GROUP BY` must consume the pool's whole slice before
`ORDER BY … LIMIT` can pick the newest 21. Reading in sort-key order stops at
the window, because `asset_id` is the last key component and an operation's two
legs are therefore adjacent — Rust folds them without an aggregation.

Two hypotheses died here and are recorded so they are not retried: `FINAL` was
never the cost (+22%, not an order of magnitude), and
`optimize_aggregation_in_order` bought nothing while doubling latency.

### Cursor at a boundary inside a multi-operation transaction

Transaction `4849775023734824275` in ledger `64007288` runs **11 operations**
against this pool, occupying positions 4–14, so any page shorter than 13 splits
it. Walking 4 pages of 5 with the shipped keyset returned 20 rows, 20 distinct,
byte-identical to the top-20 taken in one shot — no duplicates, no gaps. Pages 3
and 4 both open and close _inside_ that transaction (cursors at `ao` 14, 9, 1).

The test is not a tautology. The same walk with a transaction-level keyset
`(ledger_sequence, transaction_id)` — the shape the retired endpoint's cursor
used — jumps from `ao = 13` straight to the next ledger, silently dropping the
remaining 12 operations of that transaction. Carrying `application_order` in the
cursor is what the per-operation row unit requires.
