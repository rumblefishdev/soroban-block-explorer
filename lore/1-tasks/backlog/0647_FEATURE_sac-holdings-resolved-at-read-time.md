---
id: '0647'
title: 'FEATURE: store a contract-held SAC balance under the SAC it sits in, resolve the wrapped asset at read time'
type: FEATURE
status: backlog
related_adr: ['0051']
related_tasks: ['0571', '0640', '0641', '0374']
tags: ['clickhouse', 'indexer', 'api', 'schema', 'priority-low', 'effort-large']
links: []
history:
  - date: '2026-10-16'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0571. 0571 keeps translation at write time and fixes its
      two causes; this is the shape that removes the cross-ledger lookup from
      the writer altogether, chosen as the long-term direction.
---

# FEATURE: SAC holdings resolved at read time

## Summary

Write what the ledger says: a contract-held SAC balance is a `ContractData`
entry inside the SAC contract, so store it under that contract. Translate the
SAC to the classic or native asset it wraps when reading, through `asset_sac`
(one row per SAC; 4,065 on mainnet, 15,368 on testnet, 2026-10-16). The
writer then needs no fact from another ledger, so a parallel backfill writes
exactly what the live indexer writes and no SAC re-key pass exists.

## Context

0571 found two causes of SAC-keyed rows (mainnet: identity read from the
envelope; testnet: backfill order) and fixes both while keeping the
translation at write time. The write-time lookup stays a cross-ledger read
inside the writer — the class 0640 and 0641 deal with for other columns.

This reverses a recorded decision: `crates/db-clickhouse/schema/init.sql`
(`liquidity_pools.legs` comment) withdrew read-side resolution in favour of
the writer, after 1,084 of 1,175 soroban legs resolved to no asset (task
0374). That failure was legs keyed on the SAC with no reader translating
them; this task makes every reader translate.

## Scope to settle first

- Every write site keyed through `contract_token_asset_id` or
  `build_balance_rows`: `balances`, `liquidity_pools.legs`,
  `pool_operation_amounts`, `pool_movements`.
- Every read that groups or filters these by asset: asset holders, account
  balances, pool legs, `balance_aggregates_mv`.
- Rows to re-key back onto the SAC contract — measured 2026-10-16,
  contract-held `balances` pairs on a SAC-wrapped asset: 83,130 on `default`,
  156,464 on `testnet`; plus pool legs and movements (not yet counted).
- Whether `asset_sac` gains a lookup by `sac_contract_id` (today every read
  aggregates the whole table).

## Acceptance Criteria

- [ ] No writer reads `asset_sac` or any other table to key a SAC holding
- [ ] Asset pages, account balances, pool legs and supply show the same
      figures as before on both networks (differential check)
- [ ] A backfill of a range containing a SAC deploy and a balance in a later,
      parallel partition needs no post-backfill step
- [ ] **Docs updated** — `docs/architecture/**` (asset identity, ingestion),
      `docs/backfills.md`
- [ ] **API types regenerated** — if any DTO changes
