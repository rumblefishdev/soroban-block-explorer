---
id: '0491'
title: 'FEATURE: pool activity is a list of operations, with a trades filter'
type: FEATURE
status: backlog
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
- [ ] Pagination is stable across the new cursor, including at a page boundary
      that falls inside a multi-operation transaction
- [ ] Read path stays a PK-prefix seek with the same partition prune — measure
      `read_rows` before and after; more rows per page must not mean a scan
- [ ] 0490's cap is removed or confirmed dead, not left as unreachable code
- [ ] **Docs updated** — per ADR 0032, the endpoint contract and the canonical
      SQL under `docs/architecture/**`
- [ ] **API types regenerated** — required, the response shape changes
