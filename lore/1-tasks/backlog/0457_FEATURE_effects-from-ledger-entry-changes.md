---
id: '0457'
title: 'FEATURE: Effects section from ledger_entry_changes — the data the page still cannot show'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0453', '0411', '0352']
tags:
  [backend, api, frontend, transaction-detail, priority-medium, effort-large]
links: []
history:
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0453 (spec decision D14). stellar.expert computes effects
      client-side from result+meta (MIT tx-meta-effects-parser); our Rust
      parser already computes the equivalent (`ledger_entry_changes`) and
      persists per-op grains — nothing exposes it.
---

# FEATURE: Effects section from ledger_entry_changes

## Summary

The one dimension where stellar.expert still beats this page: per-hop
order-book fills, actual LP deposit/withdraw reserve amounts, credited/
debited lines, claim-CB asset without waiting for details keys. All of it
is derivable from `crates/xdr-parser/src/ledger_entry_changes.rs`, which no
endpoint exposes.

## Scope

- Backend: per-operation effects derived from same-op ledger entry changes
  (created/updated/removed entries -> typed effect rows), exposed on the tx
  detail response (runtime enrichment, same absence-safe pattern as
  `op_index`). Use Horizon's effects vocabulary for naming.
- Frontend: an "Effects" disclosure on the operation card (or per-op rows in
  a tx-level section) — this also lights up the RouteStrip's order-book
  edges with real amounts and the strict-send "Received" value from actual
  deltas (complements, not replaces, the 0411 net_settled read path).

## Acceptance criteria

- [ ] The VELO-arbitrage mixed route shows the order-book hop amount
- [ ] LP deposit/withdraw show actual reserve amounts (parity with Horizon
      `reserves_deposited`/`reserves_received`)
- [ ] Strict-send delivered amount shown from deltas; the D9 "Received —"
      slot retires
- [ ] Effects hidden gracefully on pre-meta / degraded responses
