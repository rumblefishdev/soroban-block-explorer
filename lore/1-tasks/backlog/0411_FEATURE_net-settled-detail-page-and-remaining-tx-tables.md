---
id: '0411'
title: 'FEATURE: net-settled on tx detail page + remaining tx-list tables'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0393']
tags: ['frontend', 'api', 'phase-future', 'effort-small', 'priority-low']
links: []
history:
  - date: 2026-07-18
    status: backlog
    who: karolkow
    note: 'Spawned from 0393 future work — detail-page breakdown + column-coverage consistency.'
  - date: 2026-07-20
    status: backlog
    who: karolkow
    note: 'Folded in 0393 code-review finding F: this task owns the per-endpoint `values` gating (which endpoints render the column), so the `wants_values` flag cleanup + moving the tx-value query out of the shared `common/ch.rs` belong here.'
---

# FEATURE: net-settled on tx detail page + remaining tx-list tables

## Summary

Task 0393 added the "Net settled" column to the two canonical tx-list tables
(global list, account tx). Two follow-ups it deliberately deferred: a per-asset
breakdown on the **transaction detail page** (the list cell collapses extras into
`+N`), and extending the column to the **remaining tx-list tables** where it is
consistent to do so.

## Context

- List cell: `web/src/pages/transactions/cells.tsx` (`ValueCell`) shows the
  primary asset + `+N`; the comment already points here for the full breakdown.
- The tx-list tables are **not** a uniform set — column coverage differs, and the
  API only returns `values` for the accounts / transactions / **ledgers**
  endpoints (assets + liquidity_pools pass `values=false`).
- The per-endpoint gating is `common::ch::fetch_tx_list_aggregates`'s
  **`wants_values: bool`** flag. The 0393 architecture review (finding **F**) flagged
  it: one flag toggles two structurally different queries (~1600× read-cost gap —
  op-types seek vs the value partition scan), and the transaction-only value query
  (a 4-table join) lives in the **shared** `common/ch.rs` that 5 domains link. Since
  deciding column coverage IS deciding `wants_values` per endpoint, the cleanup lands
  here.

## Implementation

- **Detail page** (`web/src/pages/TransactionDetailPage.tsx` /
  `transaction-detail/`): render the full per-asset net-settled list (each row:
  scaled amount + asset code link), reusing `scaleByDecimals` + `formatAmount`.
- **LedgerTransactions** (`web/src/pages/ledgers/LedgerTransactions.tsx`): add the
  "Net settled" column — **data already flows** (ledgers endpoint returns
  `values=true`); only the column render is missing. Cheapest win.
- **AssetTransactions** (`web/src/pages/assets/AssetTransactions.tsx`): DECISION
  needed — its column set differs (Hash/Ledger/Status, no Fee) and the assets
  endpoint currently passes `values=false`. Enabling it means an API toggle +
  api-types regen. Decide whether net-settled belongs on an asset's tx list.
- **Out of scope:** `PoolTransactions`, `ContractInvocations` — event / invocation
  lists, not transaction rows.
- **F cleanup (from 0393 review):** split `fetch_tx_list_aggregates(keys, wants_values)`
  into `fetch_tx_op_types` (shared) + `fetch_tx_values` (transactions domain); drop
  the flag; callers compose. Removes the control-coupling flag AND lifts the tx-only
  value query out of `common/ch.rs`. (If task 0417 restructures the value read for the
  `(ledger,tx)` companion first, do the relocation there — whichever lands first; the
  flag removal is cheap and independent.)

## Acceptance Criteria

- [ ] Tx detail page shows the full per-asset net-settled breakdown (scaled).
- [ ] `LedgerTransactions` renders the "Net settled" column.
- [ ] AssetTransactions decision recorded (added with API toggle, or explicitly
      declined with reason).
- [ ] Naming stays `values` / `TransactionValue` (decided coherent in 0393).
- [ ] **F:** `wants_values` flag removed — `fetch_tx_op_types` (shared) +
      `fetch_tx_values` (tx domain); the tx-value query no longer lives in
      `common/ch.rs`.
