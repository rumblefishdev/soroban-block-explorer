---
id: '0583'
title: 'PERF: balance_aggregates_mv recomputes every asset every 2 minutes — 45% of the database CPU'
type: PERF
status: backlog
related_adr: ['0051']
related_tasks: ['0374', '0210', '0331', '0385', '0581']
tags: [layer-clickhouse, priority-medium, effort-medium]
links: []
history:
  - date: '2026-09-25'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0374 (decision 115 A). Found while measuring whether a
      refreshable MV or a plain view should hold the Soroban pool state.
---

# PERF: balance_aggregates_mv — 45% of the database CPU

## Summary

`balance_aggregates_mv` (asset supply and holder count, read by the assets
list, asset detail and search) recomputes every asset from scratch every
2 minutes. Over 24 hours it is the largest consumer on the server: nearly half
of all query CPU and half of all rows read. Make it cheaper without changing
what the assets pages show.

## Context

Measured on production, `system.query_log`, 24 h to 2026-09-25 01:40 (CEST):

| query                                  | runs/day | CPU    | share of CPU | share of rows read | peak memory |
| -------------------------------------- | -------- | ------ | ------------ | ------------------ | ----------- |
| `balance_aggregates_mv` refresh        | 720      | 16.5 h | **45.5%**    | **49%**            | 1.17 GiB    |
| `accounts_recent_mv` refresh           | 720      | 4.9 h  | 13.5%        | 2.8%               | 1.26 GiB    |
| next largest (a hash-prefix uniqExact) | 1,168    | 4.7 h  | 13.1%        | 28.9%              | 835 MiB     |

Each refresh reads ~463M rows in ~4.3 s: `balances FINAL` +
`claimable_balance_holdings FINAL` + the newest classic pool snapshot per pool,
then `GROUP BY asset_id` (`init.sql`, `balance_aggregates_mv`). About 0.7 of a
core, continuously, whatever the traffic.

## Implementation

Options to measure, cheapest first:

1. **Refresh less often.** Supply and holder counts are not second-sensitive;
   `EVERY 10 MINUTE` cuts the cost 5× with no code change (DDL only). Check
   what the assets pages promise about freshness.
2. **Drop `FINAL`.** Dedup the RMT sources with `argMax`/`LIMIT 1 BY` (task
   0420 measured FINAL at up to 19× the rows elsewhere); verify the sums stay
   exact against the current table.
3. **Incremental.** Recompute only assets whose balances changed since the last
   refresh, keeping the rest. More moving parts; only if 1–2 are not enough.

## Acceptance Criteria

- [ ] Chosen option measured before/after (CPU, rows, memory per refresh and
      per day)
- [ ] `balance_aggregates` identical before and after for every asset
      (full-table diff), or each difference explained
- [ ] Freshness the assets pages imply stays true, or the change is agreed
- [ ] Docs: `database-schema-overview.md` (ADR 0032)
