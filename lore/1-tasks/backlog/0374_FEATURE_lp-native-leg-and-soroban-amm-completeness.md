---
id: '0374'
title: 'LP completeness: native XLM leg match + Soroban-AMM union + share% recompute'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-medium, layer-api, liquidity-pools]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker. Bundles F-B/K2-2, K3-5, K4-6.'
---

# LP completeness

## Summary

Make liquidity-pool activity complete: match the native XLM leg (currently
unmatchable → 21.7% of pools invisible), union Soroban-AMM pools into
`/liquidity-pools`, and recompute stale `share_percentage`.

## Context

Spawned from 0359. Mostly read/query-side: the native leg fails to match because
of the two-conventions native representation (see memory: native two
conventions); Soroban AMMs live outside the classic pool table.

## Implementation

- **F-B / K2-2** — match the native XLM leg in LP snapshots (16 552 pools /
  21.7% currently invisible).
- **K3-5** — union Soroban-AMM pools into `/liquidity-pools`.
- **K4-6** — recompute stale LP `share_percentage` (unconfirmed; verify first).

## Acceptance Criteria

- [ ] native XLM leg matches → pools visible — F-B / K2-2
- [ ] Soroban-AMM pools unioned — K3-5
- [ ] share_percentage correct (or confirmed already correct) — K4-6
