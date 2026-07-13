---
id: '0373'
title: 'Contract-as-holder + non-G participants (C/L/B) + SAC C-address resolve'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-large, layer-indexer, contracts]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker. Bundles F-D, K2-8, K2-3, K3-3, K3-6.'
---

# Contract-as-holder + non-G participants

## Summary

Index contracts as first-class asset holders and transaction participants, and
resolve SAC C-addresses to their wrapped asset. Today `transaction_participants`
filters to `G…` accounts only — contracts, pools and claimable-balance addresses
never register.

## Context

Spawned from 0359. The participant filter is `starts_with('G')` — non-G
addresses (contract `C`, pool `L`, balance `B`) are dropped. Contracts hold
assets via SAC ContractData (not trustlines), so they are orphaned from the
balances / holder model (see memory: contract-as-holder gaps).

## Implementation

- **K2-3** — allow non-G participants (`C` / `L` / `B`) into
  `transaction_participants` via the shared address-surrogate space.
- **F-D / K2-8** — index contract-held classic/native balances (SAC ContractData)
  so a contract holder is not orphaned when the SAC is un-sighted.
- **K3-3** — capture nested `contract_ids[]` (dropped for ~100% of Soroban txs).
- **K3-6** — search: resolve a SAC `C`-address to its wrapped classic asset.

## Acceptance Criteria

- [ ] non-G participants registered (C/L/B) — K2-3
- [ ] contract-held classic/native indexed — F-D / K2-8
- [ ] nested contract_ids captured — K3-3
- [ ] SAC C-address resolves to wrapped asset in search — K3-6
