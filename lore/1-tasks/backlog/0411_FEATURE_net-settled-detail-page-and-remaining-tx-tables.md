---
id: '0411'
title: 'FEATURE: net-settled on tx detail page + remaining tx-list tables'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0393', '0453']
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
  - date: 2026-07-29
    status: backlog
    who: karolkow
    note: >
      Scope grew by one item: the column was REMOVED from the shipped tx-list
      table, so this task now owns bringing it back as well as extending it.
      A pre-deploy audit of production found no part of 0393 live — the CH
      column, the API `values` field and the SPA bundle are all pre-0393 — so
      shipping the frontend as it stood would have rendered a dash in every
      row of every view that uses `TransactionsTable` (global list, ledger
      detail, account detail), not merely for history. Blocked on 0419 (CH
      ALTER → indexer deploy → S3 re-ingest) and 0417 (read-path release gate).
  - date: 2026-07-29
    status: backlog
    who: karolkow
    note: >
      The API-side value read was removed too, so this task now owns restoring
      both halves. Leaving it in place would have kept a ~26M-row/page scan plus
      three un-pruned dimension joins running on POLLED endpoints
      (transactions, accounts, ledgers) to fill a field no client renders — the
      exact shape that caused the 0243/0386 quota outages, and against the
      warning in the function's own comment. Response shape unchanged: `values`
      still serialises, always empty, so `api-types` regenerates byte-identical
      and no client contract breaks. Side effect: 0393 review finding **F**
      (the `wants_values` control-coupling flag, and the tx-only value query
      living in the shared `common/ch.rs`) is resolved by deletion — the flag
      and the query are both gone. Reinstating the read is the moment to place
      it correctly, in the transactions domain, on 0417's companion table.
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

- **Restore the value read itself** (`crates/api/src/common/ch.rs`): the whole
  net-settled aggregate was removed, not just gated — the `wants_values` flag,
  the value SQL, its `TxValueChRow`, the issuer resolution and the merge are
  gone, and all five callers now take `(client, keys)`. With the column pulled
  from the frontend the read was scanning ~26M rows/page on POLLED endpoints for
  a result nobody rendered. `TxListAggregates::values` stays in the response
  shape (still serialised, always empty), so the OpenAPI surface and
  `api-types` are unchanged — regenerated and verified byte-identical.
  Reinstating it here means writing the read back, and 0417's `(ledger,tx)`
  companion is the natural moment: this deletion removed finding **F** by
  removing its subject, so the flag cleanup below is already satisfied.
- **Restore the tx-list column** (`web/src/pages/transactions/TransactionsTable.tsx`):
  the `net_settled` column entry was removed ahead of a frontend deploy, because
  none of 0393 is live in production — the column definition is gone, the comment
  in its place points here, and `ValueCell` / `cells.tsx` are untouched. Put the
  entry back once 0419 has run; nothing else on the frontend was changed.
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

- [ ] Tx-list `net_settled` column restored in `TransactionsTable` (removed
      pre-deploy; only valid once 0419 has run and the API returns `values`).
- [ ] Tx detail page shows the full per-asset net-settled breakdown (scaled).
- [ ] `LedgerTransactions` renders the "Net settled" column.
- [ ] AssetTransactions decision recorded (added with API toggle, or explicitly
      declined with reason).
- [ ] Naming stays `values` / `TransactionValue` (decided coherent in 0393).
- [ ] **F:** `wants_values` flag removed — `fetch_tx_op_types` (shared) +
      `fetch_tx_values` (tx domain); the tx-value query no longer lives in
      `common/ch.rs`.
