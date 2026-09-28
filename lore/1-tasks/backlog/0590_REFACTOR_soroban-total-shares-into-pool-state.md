---
id: '0590'
title: 'REFACTOR: store a soroban pool total shares with its reserves, as history'
type: REFACTOR
status: backlog
related_adr: ['0058']
related_tasks: ['0374', '0325']
tags:
  [
    soroban,
    liquidity-pools,
    clickhouse,
    schema,
    backfill,
    effort-medium,
    priority-medium,
  ]
links: []
history:
  - date: 2026-09-28
    status: backlog
    who: karolkow
    note: >
      Spawned from 0374 (decision 84 A, 85: a task of its own, not now).
      Supersedes the W2 item of 0374's PR split.
---

# REFACTOR: store a soroban pool total shares with its reserves, as history

## Summary

A classic pool keeps reserves and total shares in one row per ledger
(`liquidity_pool_snapshots`). A soroban pool keeps its reserves as history in
`pool_state_changes` but its total shares only as the current value in
`pool_instance_state`, the one-row-per-pool table for the pool's relations
(plane, share token). Move `total_shares` into `pool_state_changes` as
`Nullable(Int128)`, written in the same pass as the reserves, with its history
re-parsed, and drop it from `pool_instance_state`.

## Context — why it is split today

- **2026-09-02, commit `050ee9109`:** `total_shares` was added to
  `pool_instance_state` because it "rides the same entry, the same extraction
  pass and the same version clock as the share token". A router pool's
  reserves then came from the plane's `PoolData` entry — another contract,
  another write — so one table per writer and clock put the two apart.
- **2026-09-15, decision C′ (0374):** router reserves moved from the plane to
  the pool's own instance storage. Reserves and `TotalShares` now come from
  the same instance write, and the table shape was not revisited.
- Measured 2026-09-28: the newest state row and the newest instance row share
  their ledger for 235 of 235 pair pools, 244 of 383 constant, 66 of 85
  stable, 40 of 49 concentrated, 3 of 3 elastic; the rest are instance
  rewrites that leave the reserves alone.

## What the split costs today

- Every soroban read joins a second table. The API's shares query reads
  `pool_instance_state` twice and scans `soroban_contracts` whole on each
  request (152,397 of 154,364 rows by `EXPLAIN ESTIMATE`: the table is sorted
  by `contract_id`, the filter is on `id`) to reach the share token's
  decimals — which the shared identity resolver already gives: 730 of 730
  share tokens are in `assets`.
- The column is `Int128`, so a missing `TotalShares` key is stored as `0`.
  `served_total_shares` (API) therefore infers from the reserves whether a `0`
  was measured (0374 decision 110).
- Concentrated pools store `0`, yet `get_total_shares()` answered a non-zero
  value for 45 of 46 on 2026-09-13 (0374 production verification). The key
  they keep it under is not read.
- Soroban total shares have no history, so no chart or share price over time.

## Implementation

1. Find where each family keeps its total: router constant / stable / elastic
   (`TotalShares`), concentrated (not yet known — probe an instance), pair
   family, config family (supply on the separate share token). Record the key
   per family and per code version, as C′ did for reserves.
2. DDL (operator): `ALTER TABLE pool_state_changes ADD COLUMN total_shares
Nullable(Int128) DEFAULT NULL`. `NULL` = the pool keeps no such key.
3. Writer: stage `total_shares` on the state row from the same instance
   post-image; emit a row when the reserves **or** the total change.
4. History: targeted re-parse of `pool_state_changes` over the soroban range
   (same shape as the 2026-09-13 backfill); `docs/backfills.md` procedure.
5. Read: list and detail take the total from the state row; share-token
   decimals through the identity resolver; `served_total_shares` loses its
   reserve inference (`NULL` → null, value → scaled).
6. Rollout for the old column, in the driver's DESCRIBE order (0310): give
   `pool_instance_state.total_shares` a `DEFAULT` (operator), remove it from
   the row struct and `init.sql`, deploy, then `DROP COLUMN` (operator).
7. Revisit a shared classic/soroban state view or table (0374 decision 71 B):
   after this both rows are (pool, ledger, reserves, total shares).

## Acceptance Criteria

- [ ] Total-shares key recorded for every family and code version that ran
- [ ] `pool_state_changes.total_shares` written live; `NULL` only where the pool keeps no total
- [ ] History re-parsed; coverage measured per family
- [ ] Chain check: newest total equals `get_total_shares()` / `total_supply()` on a sample of every family, concentrated included
- [ ] API reads the total from the state row; `served_total_shares` has no reserve inference; no `soroban_contracts` scan
- [ ] The soroban read is plain: one state-row join carrying reserves and total, no SQL templated by string replacement, no soroban-only special cases on the classic row (the rewiring alone was tried in PR #526 and withdrawn — it moved the complexity instead of removing it)
- [ ] `pool_instance_state.total_shares` dropped (struct, `init.sql`, production)
- [ ] Docs: database-schema, indexing-pipeline, backend overview, ADR 0058 amended
