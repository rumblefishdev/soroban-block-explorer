---
id: '0400'
title: 'FEATURE: find-by-amount — sort/filter transactions by net-settled value moved'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0393']
tags:
  [
    clickhouse,
    api,
    frontend,
    transactions,
    read-path,
    effort-large,
    priority-medium,
  ]
milestone: 1
links:
  - crates/api/src/common/ch.rs
  - crates/db-clickhouse/schema/init.sql
history:
  - date: '2026-07-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0393 future work. 0393's origin request was to LOCATE a
      transaction by how much it moved (fee is uninformative) — which implies
      sort/filter, not just display. 0393 shipped display-only MVP (the Amount
      column renders per (tx, asset) but is not sortable/filterable). This task
      is the sort/filter half.
---

# FEATURE: find-by-amount — sort/filter transactions by value moved

## Summary

Make the tx-list "Amount" column actionable: sort transactions by net-settled
value moved, and filter by an amount range (optionally per asset). The stored
figure already exists (`operation_asset_appearances.amount`, task 0393); this
task adds the query access-path and the UI controls.

## Context

Parent: [0393](../active/0393_FEATURE_transaction-value-amount-column/README.md)
computed and displays the net-settled `amount` per (transaction, asset) but
deliberately deferred sort/filter — a genuine amount filter needs an
amount-oriented access path this table does not have (it is `asset_id`-leading;
the tx-list read is a partition-pruned scan). This is gated on the **0393
read-path performance follow-up** — the same access-path decision (skip index vs
companion table) that measurement drives.

## Implementation (sketch — refine after the 0393 perf measurement)

- **Access path first.** The current read scans a pruned partition. Sorting/
  filtering by amount over that is worse. Decide the mechanism from the 0393
  load measurement: a value-oriented projection is ruled out (CH refuses
  projections on RMT, task 0353), so likely a companion table keyed for the
  amount query, or a skip index — do NOT guess.
- **Per-asset vs per-tx amount.** A tx moves several assets; "filter by amount"
  needs an asset context (e.g. "> 1000 USDC") or a canonical asset (native-first,
  per the 0393 decision). Define the semantic before building UI.
- **API:** extend the tx-list query params with an amount range + asset filter;
  add a sort key.
- **Frontend:** sortable "Amount" column header + a filter control on the
  transactions list (global + per-account).
- Regenerate API types (openapi.json + generated/) per the CLAUDE.md gate.

## Open questions

- Filter by which asset when a tx moved several? Native-first canonical, or an
  explicit asset picker?
- USD-denominated filter is separate (blocked on the Prices API, task 0247) —
  keep this asset-native.

## Acceptance Criteria

- [ ] Transactions sortable by net-settled amount on the global + per-account
      lists, with a chosen amount access-path (measured, not guessed).
- [ ] Amount-range filter (per-asset semantic defined) on the tx-list endpoints.
- [ ] Read cost stays within quota under load (the 0393 read-path concern).
- [ ] API types regenerated; docs updated per ADR 0032.
