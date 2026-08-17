---
id: '0493'
title: 'FEATURE: account detail renders the LP positions the account holds'
type: FEATURE
status: backlog
related_adr: ['0055']
related_tasks: ['0463', '0126', '0162']
tags:
  [
    frontend,
    backend,
    api,
    clickhouse,
    liquidity-pools,
    priority-low,
    effort-medium,
  ]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/377']
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0463 planning map. Scope research there found the
      account page renders no LP positions at all — the assumption that this
      was a cheap ride-along on the zero-balance fix was wrong. The write-path
      half (lifecycle for LP positions) stays in 0463 because deferring it
      would cost a second full backward pass; the rendering half is this task
      because it costs the same whenever it is done.
---

# FEATURE: account detail renders LP positions

## Summary

The account detail page shows classic, native and Soroban holdings. It shows
**no liquidity-pool positions** — `crates/api/src/accounts/dto.rs:85-95` has no
field for them, and no query fetches them.

The mirror already exists: task 0126 shipped the pool-side view ("which
accounts are in this pool"). The account-side view ("which pools is this
account in") was never built.

## Why it is not part of 0463

Task 0463 fixes the zero-versus-closed ambiguity and, per
[ADR 0055](../2-adrs/0055_holding-lifecycle-column-on-balances.md), carries the
LP **write path** with it — because all extractors run over the same decoded
changes slice, so deferring any holding kind would force a second full backward
pass (~24 h at 6 workers plus a mandatory `repair-tier1`, estimate).

Rendering has no such coupling. It costs the same today or in six months, and
bundling it would have grown 0463 by a feature nobody asked for in #377.

## The non-obvious cost

`lp_positions` is ordered `(pool_id, account_id)` — built for the pool-side
read. An account-side query is therefore a **full scan**, not a key seek. This
task is not "add a field to the DTO"; it needs either a projection, a second
sort order, or a companion table, chosen on measurement.

Note also the existing API convention on the pool side: task 0126 filters
`WHERE shares > 0`, the same zero-filter pattern 0463 is removing elsewhere.
Decide deliberately what a zero-share position means on the account page rather
than inheriting that filter by accident — once 0463 lands, closure is
distinguishable from emptiness here too.

## Acceptance criteria

- [ ] An account holding pool shares sees them on its detail page
- [ ] A closed (fully withdrawn) position is not shown, and the distinction
      comes from the lifecycle column rather than from `shares > 0`
- [ ] The account-side read is not a full scan — measured, with the chosen
      access path recorded
- [ ] **Docs updated** — read path and frontend data contract
- [ ] **API types regenerated** — yes, the account DTO gains a field
