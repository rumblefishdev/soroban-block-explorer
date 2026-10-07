---
id: '0601'
title: 'REFACTOR: pool participants carry `holder`, not `account` (it can be a contract)'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0600']
tags: [api, frontend, effort-small, priority-low]
links: []
history:
  - date: '2026-09-30'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0600 future work (thread 369 B): same naming fix as NFT
      `owner`, left out of #564 to keep its scope.
---

# Pool participants carry `holder`, not `account`

## Summary

`ParticipantItem.account` (`crates/api/src/liquidity_pools/dto.rs`) holds a
`G…` account or a `C…` contract (gauges, vaults hold Soroban pool shares), so
the name misleads the way NFT `owner_account` did before task 0600.

## Implementation

- Rename the wire field `account` → `holder`; the query row follows.
- SPA `PoolParticipants` reads `holder` (link type already from `addressType`).
- Regenerate API types; update `docs/architecture/**`.

## Acceptance Criteria

- [ ] `ParticipantItem.holder`; no `account` field left on it
- [ ] Pool participants page renders and links as before (production data)
- [ ] **API types regenerated**
- [ ] **Docs updated**
- [ ] Deploy note: Compute (API) and Web in one window
