---
id: '0613'
title: 'Index liquidity positions of concentrated Soroban pools'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0374', '0516', '0612', '0618']
tags: ['effort-small', 'priority-low', 'liquidity-pools']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Spawned from 0516 (the "Aquarius concentrated positions" item, 0374 deferred step 23): concentrated pools show their providers as not indexed.'
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Plan rewritten to the read-time aggregation the measurement allows (no table, no backfill); effort-large → effort-small. Phoenix staking, the same shape, spawned as 0618.'
---

# Index liquidity positions of concentrated Soroban pools

## Summary

A concentrated pool issues no share token: each provider holds a position (a
price range and an amount of liquidity) stored inside the pool contract. The
participants list and count read share-token holders from `balances`, so
these pools answer `not_indexed` and the page says the providers are not
indexed. Index the positions, then list and count them.

## Context

Production, 2026-10-02: every Soroban pool without a share token
(`pool_instance_state.share_token_id = 0` on its latest row) is concentrated —
51 pools: 37 from the Aquarius router `CBQDHNBF…` (all of its concentrated
pools; the protocol's API reports `pool_type: concentrated` and the pool
address as its "share token", decimals 0) and 14 from the deployment
`CA7RQDMM…`, which `protocol_labels.rs` does not name and the Aquarius API
does not list. Example: `CD74JDJI…` (AQUA/sUSD), $6.6M volume in September.
All 2-leg pools with a share token and all 3- and 4-leg pools list their
providers today.

The 0374 worklog has the earlier findings: concentrated reserves ride the
pool instance after the legs (T4), and positions are not NFTs (2026-09-02
correction).

## Measured (2026-10-05)

The data is already indexed: `soroban_events` holds every `position_update`
of these pools (25,090 in the 37 Aquarius pools, 194 in the 14 `CA7RQDMM…`
pools, which emit the same event set), and its topic and data columns are
stored as JSON, so SQL reads them directly: topic 2 the owner, data
`[tick_lower, tick_upper, liquidity delta]`. Summed per
`(pool, owner, tick_lower, tick_upper)` over the whole history (deduped on the
event key): 993 open positions, 332 providers, in 48 of the 51 pools; no
position sums below zero, which is what a delta should give. So no new table
and no backfill — a read-time aggregation, as decided on 2026-08-26. Not yet
checked against contract state on chain.

## Implementation Plan

- Read path only: a query that sums `position_update` deltas per
  `(pool, owner, tick_lower, tick_upper)`, deduped on the event key; the
  participants handler sends a pool without a share token to it instead of
  answering `not_indexed`. The count follows the same query.
- What a row shows (owner, range, liquidity; a share of the pool or not) is
  settled by a prototype before the API shape is written.
- Spot-check a sample of positions against the position storage in ledger
  entries (raw XDR is the arbiter), including pools from both deployments.
- Identify `CA7RQDMM…` (protocol, router) and label it, or record why not.

## Acceptance Criteria

- [ ] The participants list and count of every concentrated pool come from
      its `position_update` events; spot-checked against contract state on
      chain.
- [ ] `CA7RQDMM…` named or explained.
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md`
      (participants); `docs/architecture/database-schema/**` N/A — no schema
      change.
