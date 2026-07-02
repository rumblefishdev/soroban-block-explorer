---
id: '0341'
title: 'Hide LP-detail charts card (TVL/Fee/Volume) behind flag until price-oracle lands'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0199', '0215']
tags: [layer-frontend, priority-low, effort-small]
milestone: 2
links: []
history:
  - date: '2026-07-02'
    status: active
    who: stkrolikiewicz
    note: 'Task created — gate PoolCharts behind a build-time flag until 0199/0215 ship real series.'
---

# Hide LP-detail charts card (TVL/Fee/Volume) behind flag until price-oracle lands

## Summary

Add a build-time flag that hides the whole `PoolCharts` card on the liquidity
pool detail page until the price-oracle API ships (tasks 0199 + 0215). All three
metric series (`tvl` / `volume` / `fee_revenue`) come back NULL today, so the
card only ever renders a "pending" placeholder — hiding it removes dead UI until
the data exists.

## Status: Active

**Current state:** Flag implemented (`CHARTS_ENABLED = false` in
`web/src/pages/pool-detail/PoolCharts.tsx`) — `PoolCharts` returns `null`, so
the card never renders and `usePoolChart` never fires. Lint + typecheck green;
shipped to `develop`. Re-enable (flip flag + drop the pending-oracle
placeholder) is future work under 0199/0215, not this task.

## Context

`web/src/pages/pool-detail/PoolCharts.tsx` currently renders an inline
"Chart data not yet available — pending the price-oracle integration (task
0199)" placeholder, because the chart endpoint returns null for every series
(blocked on the team price oracle per 0199/0215; endpoint ownership per ADR
0043). We want the card removed entirely, not placeholdered, until real data
lands.

## Implementation Plan

### Step 1: Build-time flag

Add `const CHARTS_ENABLED = false;` in `PoolCharts.tsx` and early-`return null`
from `PoolCharts` when it's off. Returning null means `LazySection` never
mounts and `usePoolChart` never fires.

### Step 2: Re-enable (future, not this task)

When 0199/0215 deliver non-null series, flip the flag to `true` and drop the
now-dead const + pending-oracle placeholder branch.

## Acceptance Criteria

- [x] Charts card does not render on the LP detail page while the flag is off
- [x] No chart API request fires (`usePoolChart` not called)
- [ ] **Docs updated** — N/A: frontend-only visibility toggle; no schema,
      endpoint, pipeline, or data-contract change (endpoint still exists, just
      not called).
- [ ] **API types regenerated** — N/A: no change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Notes

Re-enable is a one-line flag flip, coupled to 0199/0215 delivering non-null
series. Related: 0199 (LP analytics, blocked-on-oracle), 0215 (FE-impact
catalog of the blocked endpoint), ADR 0043 (Lambda 2 owns USD-denominated
fields).
