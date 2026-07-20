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
    who: claude
    note: 'Spawned from 0393 future work — detail-page breakdown + column-coverage consistency.'
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

## Acceptance Criteria

- [ ] Tx detail page shows the full per-asset net-settled breakdown (scaled).
- [ ] `LedgerTransactions` renders the "Net settled" column.
- [ ] AssetTransactions decision recorded (added with API toggle, or explicitly
      declined with reason).
- [ ] Naming stays `values` / `TransactionValue` (decided coherent in 0393).
