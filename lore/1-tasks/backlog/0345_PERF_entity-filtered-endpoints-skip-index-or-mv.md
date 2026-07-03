---
id: '0345'
title: 'PERF: entity-filtered endpoints do ~25M-row full-scans — add skip-index / MV / filter pushdown'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0338']
tags:
  [priority-high, effort-medium, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Spawned from 0338 load-test analysis — the ~25M-row full-scan cluster (tier 2 bottleneck).'
---

# PERF: entity-filtered endpoints do ~25M-row full-scans

## Summary

A cluster of account/contract-filtered endpoints reads **~24–49M rows per
request** and takes **4–10 s** in the 0338 load test. They filter by an entity
(account / contract) but still full-scan — no skip index, MV, or filter
pushdown. Same failure mode as the earlier `api_throttle` quota incident.

## Context

Evidence: `crates/load-tests/out/2026-07-01T13-43-39Z/results.csv` (10-VU smoke),
per-endpoint read_rows max / duration p95:

- `ctrinvoc` 49.0M / 7.1 s · `acctxs` 33.3M / 7.2 s · `lplist` 27.3M / 4.9 s
- `acclist` 26.1M / 7.7 s · `accdetail` 25.7M / 10.4 s · `ctrdetail` 25.5M / 6.6 s
- `ctrevents` 25.3M / 5.5 s

`read_rows` almost linearly predicts latency (<2.5M rows → sub-second; >20M →
multi-second). At only 10 VU CH already read 2.65B rows in 64 s — this is a
query/indexing problem, not concurrency.

## Implementation

- For each endpoint, confirm the entity filter (`account` / `contract`) is
  pushed down; add a `bloom_filter` / `set` skip index on the filter column, or a
  materialized view that pre-aggregates per entity, so CH prunes granules.
- Prioritise by rows×frequency; verify read_rows drop in `system.query_log`.

## Acceptance Criteria

- [ ] Each listed endpoint's read_rows/request drops from ~25M to a small fraction (measured)
- [ ] Sub-second p95 for these endpoints at load
- [ ] No regression to the shared `operations_appearances` / write path
