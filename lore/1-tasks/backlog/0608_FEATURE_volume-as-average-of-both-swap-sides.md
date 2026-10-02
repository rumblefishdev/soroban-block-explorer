---
id: '0608'
title: 'Price swap volume as the average of both sides, for every pool'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0374', '0607']
tags: ['effort-medium', 'priority-low', 'liquidity-pools']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-02
    status: backlog
    who: karolkow
    note: 'Spawned from 0374: the multi-leg volume decision took the lowest traded leg now and left the industry definition for later.'
---

# Price swap volume as the average of both sides, for every pool

## Summary

Pool volume prices each trade by one leg: leg A for classic and two-leg
Soroban pools, the lowest-index traded leg for three- and four-leg pools
(0374). The common definition prices a swap at the average of what went in
and what came out, or at the one side that has a price. Move every pool, both
kinds, to that definition, so volume no longer depends on the order of a
pool's legs.

## Context

Definitions read 2026-10-02 from the source of public indexers: the average
of both sides when both are priced, else the priced side (Uniswap v2
subgraph `getTrackedVolumeUSD`; Messari's Curve subgraph
`calculateAverage([amountInUSD, amountOutUSD])`); a stablecoin side first,
then the priced side, then the average (Balancer v2 subgraph
`swapValueInUSD`).

Measured on the five three-leg Aquarius pools against the protocol's hourly
`statistics/pool/<address>/` `volume_usd` (window from each pool's first hour
to 2026-10-01 16:00 UTC; 13,498 swaps on both sides, equal per pool): over
the two pools with real volume, lowest traded leg +0.41 % / +0.33 %, average
+0.39 % / +0.12 %, input side +0.35 % / +0.02 %; median hourly error 0.3–0.4 %
for every rule. The choice of side moves volume by about the fee and slippage.

Why it is not done in 0374: classic snapshots store only `gross_volume_a`
(`init.sql`, `liquidity_pool_snapshots`), so the average needs the B side
written too, and a backfill; doing it for Soroban alone would make the two
kinds measure differently.

## Implementation Plan

- Measure first: for the two-leg pools of both kinds, the change in 24h and
  chart volume between leg A and the average (full population, read-only).
- Classic: a `gross_volume_b` column next to `gross_volume_a`, written by the
  indexer, backfilled over history (`docs/backfills.md`).
- Soroban: price both rows of each trade and average the priced ones
  (`pool_movements` already holds both).
- One definition in `usd_analytics.rs` and the chart; docs updated.

## Acceptance Criteria

- [ ] The two-leg change is measured and reported before the switch.
- [ ] Classic and Soroban volume use the same definition, on the detail
      endpoint and the chart.
- [ ] Classic history backfilled; `repair-tier1` run if the backfill needs it.
- [ ] Re-measured against the protocol's hourly statistics.
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md`
      (volume definition), `docs/architecture/database-schema/**` (new column).
