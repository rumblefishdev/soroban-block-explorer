---
id: '0375'
title: 'Fee-bump inner_tx_hash lookup — resolve the hard 404'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-small, layer-api, fee-bump]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker. K3-2.'
  - date: 2026-07-13
    status: active
    who: karolkow
    note: 'Activated for implementation.'
---

# Fee-bump inner_tx_hash lookup

## Summary

A lookup by a fee-bump's **inner** transaction hash returns a hard 404 — the
`inner_tx_hash` is stored on the transaction but never indexed for lookup, so
`/transactions/{inner_hash}` cannot resolve to the fee-bump.

## Context

Spawned from 0359 (K3-2). Read-side / index only: `ExtractedTransaction.inner_tx_hash`
is already captured (0359 read verified). Needs an index / hash-index row so the
inner hash resolves to the wrapping fee-bump tx (matches Horizon's
`inner_transaction.hash` lookup).

## Implementation

- Index the fee-bump `inner_tx_hash` (extend `transaction_hash_index` or add a
  lookup path) so `/transactions/{inner_hash}` resolves to the fee-bump.
- No re-parse: the hash is already stored; this is a read/index change.

## Acceptance Criteria

- [ ] `/transactions/{inner_tx_hash}` resolves to the fee-bump tx (no 404)
- [ ] matches Horizon `inner_transaction.hash` semantics
