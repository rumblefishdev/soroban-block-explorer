---
id: '0612'
title: 'Review the liquidity-pool module, then price swap volume as the average of both sides'
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
  - date: 2026-10-02
    status: backlog
    who: karolkow
    note: 'Scope widened: a review of the whole liquidity-pool module (classic and Soroban) comes first; its findings shape the volume change.'
  - date: 2026-10-02
    status: backlog
    who: karolkow
    note: 'Renumbered from 0608: the id collided with another task opened the same day.'
---

# Review the liquidity-pool module, then price swap volume as the average of both sides

## Summary

Pool volume prices each trade by one leg: leg A for classic and two-leg
Soroban pools, the lowest-index traded leg for three- and four-leg pools
(0374). The common definition prices a swap at the average of what went in
and what came out, or at the one side that has a price. Move every pool, both
kinds, to that definition, so volume no longer depends on the order of a
pool's legs.

Before that change, review the whole liquidity-pool module, classic and
Soroban: how it is built, how TVL, volume, fees and participants are computed
and shown, and whether the two kinds are consistent and well organised. The
review lists technical debt and other problems; its findings decide where the
volume change lands and become their own tasks.

## Review scope (phase 1)

- **Architecture.** `crates/api/src/liquidity_pools/**`, the indexer and
  ClickHouse tables it reads (`liquidity_pool_snapshots`, `pool_movements`,
  `pool_state_changes`, `liquidity_pools`, `lp_positions`), and
  `web/src/pages/pool-detail/**` + the list page. Which file owns which
  concept; where two paths answer the same question (classic vs Soroban
  branches, list vs detail, 24h vs chart).
- **Computation.** TVL, volume, fees, participants, share %, activity: the
  definition each path uses, whether classic and Soroban agree, whether list,
  detail and chart agree for one pool (measured on production, whole
  population, read-only).
- **Display.** Empty and unknown states ("not indexed", "USD values
  unavailable", `null` vs `0`), units and decimals, naming of legs, the
  hints a user sees.
- **Known leads.** The traded-leg SQL is written twice (24h and chart,
  #598 review S1); `Vol24ChRow` still says "leg A" next to `LegVol24ChRow`
  (S2); the chart's TVL query is the heaviest read of the module (~2.5 TiB
  over one 779-pool comparison run, 2026-10-02); pools whose leg is a pure
  Soroban token get no price (`CCNXGPE4…`, $768k/month unpriced); Soroswap
  2025-10 is +11.9 % against DefiLlama, unexplained (0607).
- **Output.** A findings list in `notes/`, each with severity, evidence and
  the task it becomes (or "won't fix" with the reason). Convert this task to
  a directory when the notes start.

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

Phase 2, after the review:

- Measure first: for the two-leg pools of both kinds, the change in 24h and
  chart volume between leg A and the average (full population, read-only).
- Classic: a `gross_volume_b` column next to `gross_volume_a`, written by the
  indexer, backfilled over history (`docs/backfills.md`).
- Soroban: price both rows of each trade and average the priced ones
  (`pool_movements` already holds both).
- One definition in `usd_analytics.rs` and the chart; docs updated.

## Acceptance Criteria

- [ ] Review done: findings list with evidence, each finding filed as a task
      or closed with a reason.
- [ ] The two-leg change is measured and reported before the switch.
- [ ] Classic and Soroban volume use the same definition, on the detail
      endpoint and the chart.
- [ ] Classic history backfilled; `repair-tier1` run if the backfill needs it.
- [ ] Re-measured against the protocol's hourly statistics.
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md`
      (volume definition), `docs/architecture/database-schema/**` (new column).
