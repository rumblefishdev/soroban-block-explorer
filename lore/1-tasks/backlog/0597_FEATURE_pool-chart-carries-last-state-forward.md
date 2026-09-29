---
id: '0597'
title: 'Pool chart carries the last state forward (classic + soroban)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0374']
tags: ['effort-medium', 'priority-medium']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-09-29
    status: backlog
    who: karolkow
    note: 'Spawned from 0374 (decision 142 A): after #405, for both pool kinds.'
---

# Pool chart carries the last pool state forward

## Summary

A pool's chart plots a bucket only where the pool's state changed, so a quiet
pool's short range is empty ("No activity in this period") while its detail
page shows a TVL. The chart should start from the last state before the window
and carry it forward, so every bucket has a point: the last known state,
priced at that bucket's price.

## Context

- Found in the `/code-review` of the soroban chart (0374, PR #551). Classic
  pools have the same gap: `liquidity_pool_snapshots` is written only on a
  change, and `fetch_pool_chart` reads only the window's snapshots.
- Production 2026-09-29: of 779 soroban pools, 105 changed state in the last
  hour, 338 in 7 days, 493 in 90 — so on the default 1D range ~57% show an
  empty TVL chart.
- Decided 2026-09-29 (142 A): one fix for both kinds, after #405, not a
  soroban-only fix inside the chart PR.

## Implementation

- Seed: the newest state row at or before the window start (classic:
  `liquidity_pool_snapshots`; soroban: `pool_state_changes`, deduped).
- Fill: one point per bucket from the seed onward; a bucket with no change
  takes the previous state (ClickHouse `WITH FILL … INTERPOLATE`, or a bucket
  grid joined to the state series).
- Price each filled bucket at its own price bucket with the existing ASOF +
  `MAX_PRICE_CARRY_SECONDS` rule; volume and fee stay per-bucket flows (a
  filled bucket has no volume — `0`, not carried).
- `samples_in_bucket` stays the count of real state rows (0 for a filled
  bucket), so the frontend can still tell activity from carry.

## Acceptance Criteria

- [ ] A pool with no state change in the window shows a flat TVL line equal to
      the detail TVL (both kinds), verified on production for a quiet pool.
- [ ] The newest hourly point equals the detail TVL for quiet and busy pools.
- [ ] Volume and fees of filled buckets read as no activity, never carried.
- [ ] 1w latency stays within today's order (~5 s) on the busiest pool.
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md`
      (chart endpoint semantics).
- [ ] **API types regenerated** — expected no diff (response shape unchanged).
