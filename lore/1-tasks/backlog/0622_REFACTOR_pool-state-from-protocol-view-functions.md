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
  - date: 2026-10-08
    status: backlog
    who: karolkow
    note: 'Chosen as the foundation for Soroban pool state (W345 B, one pass, not staged). Widened: a scoped ledger-entry store, fee and liveness from functions. Not started; after #405.'
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

## Decision 2026-10-08 (W345 B) — the foundation for pool state

Pool state is the pool's own functions' answer, run on every change of the
pool; storage-key readers, `PoolData` and `NOT_A_POOL` go. One pass, not
staged: a staged rollout would keep both mechanisms live in production and
cost a second backfill.

Measured (one pool per program, 28 programs, 113 simulations at the RPC tip):
538 of 793 pools (every Aquarius program, Soroswap) answer every getter from
their instance. Phoenix (19) keeps everything in persistent entries; Sushi V3
reserves and Comet tokens/balances need the token contracts' `Balance(pool)`
entries. So the executor needs a **scoped store**: the latest persistent
entries of registered pool contracts and the `Balance(pool)` entries of their
leg tokens, collected from each ledger's changes — not the full copy of 0633.
Load: ~3.4 getter runs per ledger on average, ~92 worst case (estimate from
`pool_state_changes`, last 7 days).

Absorbs: 0632 (fee), 0590 (`total_shares` 0 meaning three things), the pool
half of 0325 (a failing or missing function is the "no longer a pool"
verdict), the rest of 0617, and the 57 concentrated pools that store
`total_shares` 0 while `get_total_shares` answers 94.6e12 (`CBBMQBNH…`).
Makes 0599's state side a table row per family. Unchanged: discovery and
movements stay on events; 0597, 0613, 0618, 0607, 0612 are not touched.
Split by production step: store table (CREATE) → switch + readers removed →
backfill.
