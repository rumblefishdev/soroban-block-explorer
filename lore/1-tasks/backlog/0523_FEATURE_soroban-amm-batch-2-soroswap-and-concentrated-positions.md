---
id: '0523'
title: 'Soroban AMM batch 2 — Soroswap adapter + concentrated positions'
type: FEATURE
status: backlog
related_adr: ['0058']
related_tasks: ['0374']
tags: [soroban, amm, liquidity-pools, priority-medium, effort-large]
links: []
history:
  - date: '2026-08-29'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0374 at its code-complete point: the router family
      (Aquarius's shape) ships in batch 1; these two remaining acceptance
      items are deliberately the NEXT batch (decision karolkow 2026-08-29).
---

# Soroban AMM batch 2 — Soroswap adapter + concentrated positions

## Summary

Finish issue #405's second protocol and the last Aquarius gap: index
Soroswap pools into the same tables ADR 0058 established, and represent
concentrated-pool POSITIONS (they mint no share token — positions are NFTs,
so the participants endpoint refuses them today, explicitly).

## Context

Everything structural already exists (ADR 0058): `liquidity_pools` union
rows, `pool_state_changes` (chain-grain reserves), `pool_share_tokens`,
leg resolver, list/detail/filter API+FE. A new protocol needs its own
DETECTOR/ADAPTER, not new tables. Depth-first rule stands: one protocol at
a time, measured on the full mainnet population, four-oracle checklist per
protocol (see memory / reference-stellar-validation-sources).

## Implementation

- **Soroswap** — discovery is a lookup: its LP tokens carry on-chain
  `METADATA` we already read (248 `…Soroswap…` names in
  `soroban_contract_metadata`, measured in 0374 research). Decode its
  factory/pair shape (its OWN vocabulary — pool_type stays verbatim per
  ADR 0058), reserves from ledger STATE (find where its pairs keep them —
  never event arithmetic without an oracle), verify against the vendor API
  - RPC simulation + a raw corpus, add a verified-operator label.
- **Concentrated positions (Aquarius)** — positions ride `position_update`
  events / position NFTs (decided in 0374 T-decisions, see its notes).
  Index them, then teach the participants endpoint the third population
  (classic lp_positions / share-token balances / position NFTs) and lift
  its explicit 400 for concentrated pools.

## Acceptance Criteria

- [ ] Soroswap pools discovered shape-first, unioned, reserves indexed —
      verified against vendor API + RPC (four-oracle checklist recorded)
- [ ] Soroswap label resolves at read time; unverified deployments stay
      unlabelled
- [ ] Concentrated positions indexed; participants endpoint serves them and
      the explicit refusal is removed
- [ ] Docs per ADR 0032 (architecture overviews + ADR update if decisions
      shift)
