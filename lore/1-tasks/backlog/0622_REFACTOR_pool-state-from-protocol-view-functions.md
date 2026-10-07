---
id: '0622'
title: 'Soroban pool reserves and total shares from the protocols view functions, not storage keys'
type: REFACTOR
status: backlog
related_adr: ['0061']
related_tasks: ['0620', '0374', '0617', '0619', '0325']
tags: ['effort-large', 'priority-medium', 'soroban', 'liquidity-pools']
links: []
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Spawned from the 0617 audit (W244/W251); builds on the 0620 executor.'
---

# Soroban pool state from the protocols' view functions

## Summary

Read each Soroban pool's reserves and total shares by calling the pool's own
public functions with the 0620 executor, instead of decoding each family's
storage keys. Remove the key-based readers when values match.

## Context

No AMM standard exists, so this stays per protocol — but a protocol's public
functions (`get_reserves`, Phoenix `query_pool_info`, `get_total_shares`) are
its contract with every client, while storage keys are an implementation
detail. Today: `pool_state.rs:111-134` (`ReserveA/B`, `Reserves`,
`Reserve0/1`), plane `PoolData` (`:50`), pair `u32(2,3)`, config `u32(1,2)`,
`TotalShares` (`:151`); a lone reserve write is dropped
(`pool_config_factory.rs:337`). Recorded trouble: 0374 (plane values 10× and
10^11× too high), 0617 (Phoenix pool turned staking contract kept a stale
row), 66 pools hold reserves with total shares 0.

## Implementation

- Per family, the view function and its result shape, in one table in code.
- Call it on each pool state change; write `pool_state_changes` from the
  answer. A call that fails because the contract is no longer a pool is the
  0325 verdict for pools (retires `NOT_A_POOL`).
- Compare with today's rows on one pool per family and code version, then
  remove the storage-key readers.

## Acceptance Criteria

- [ ] Reserves and total shares of every family come from its functions;
      0 unexplained differences on one pool per family and code version.
- [ ] Storage-key readers for reserves and shares removed.
- [ ] **Docs updated** — `xdr-parsing-overview.md`, `indexing-pipeline-overview.md`.
