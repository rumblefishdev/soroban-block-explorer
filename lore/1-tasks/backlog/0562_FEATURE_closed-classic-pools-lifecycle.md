---
id: '0562'
title: 'Closed classic pools — a lifecycle column, and the API stops listing them as live'
type: FEATURE
status: backlog
related_adr: ['0055', '0057']
related_tasks: ['0210', '0374', '0523', '0401']
tags:
  [
    backend,
    clickhouse,
    liquidity-pools,
    data-quality,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: '2026-09-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from task 0210's first snapshot-seed dry-run. The parser fix there
      makes an erased pool's snapshot zero; whether the pool is shown at all is
      this task.
---

# Closed classic pools — lifecycle column and API

## Summary

A classic pool (`LiquidityPoolEntry`) is erased when its last pool-share
trustline goes. Nothing in our tables says so: `liquidity_pools` keeps the row
and the pool API lists every pool with its newest snapshot. Production
(2026-09-17) has 12,607 classic pools whose newest snapshot is 0 reserves and 0
shares — closed normally — shown as live, empty pools. Task 0210 zeroes the
snapshots of another 1,381 erased by an authorization revocation, which today
show their old reserves and TVL.

Horizon marks an erased pool `deleted = true`, filters it from every read and
purges it after 100 ledgers; stellar-etl emits `deleted = true`. Our holdings
already model the same lifecycle (ADR 0055: `closed_at_ledger`).

## Context

- **Meta semantics** (task 0210, 2026-09-17): a pool `removed` is always preceded
  by a `state` of the same key; the extractor now writes a zero snapshot at the
  removal ledger and a pool row from the `state` params.
- **A pool id is SHA-256 of the pair and fee**, so an erased pool can be
  re-created under the same id; a later `created` must reopen it.
- `participant_count` (from `lp_positions`) is already 0 for these pools, but
  `lp_positions` lacks holders from before our floor (task 0523), so it cannot
  decide liveness.
- The pool list sorts newest first by `created_at_ledger` (task 0401 plans to
  store it); a closed-pool filter interacts with that ordering and with the
  `min_tvl` filter.

## Implementation

- [ ] `liquidity_pools.closed_at_ledger Int64 DEFAULT 0` (ALTER first, then the
      writer — ADR 0055 order): set by a pool `removed`, reset to 0 by a later
      `created`.
- [ ] Backfill: from the checkpoint (`snapshot-seed` already knows every live
      pool; a pool of ours absent from it is closed at the checkpoint) or from
      the zero newest snapshot — pick the one that states a true ledger.
- [ ] API: list and detail — hide closed pools by default or show them as
      closed. UX decision first (`/ux-expert`).
- [ ] `balance_aggregates_mv` needs nothing (a closed pool's reserves are 0);
      confirm.
- [ ] Docs: schema overview (`liquidity_pools`), API field docs.

## Acceptance Criteria

- [ ] No erased pool is listed as live by the pool API.
- [ ] A pool re-created under the same id is live again.
- [ ] Closed count matches the checkpoint: pools of ours absent from the
      snapshot = pools with `closed_at_ledger > 0`.
