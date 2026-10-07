---
id: '0627'
title: 'PERF: replace the accounts_recent copy with a projection on accounts'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0447', '0385', '0353']
tags: [phase-future, effort-medium, priority-low, performance, clickhouse]
links: []
history:
  - date: '2026-10-06'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0447. 0447 drops `account_id` from the copy (~82 % fewer
      writes); this task removes the remaining full rewrite every 2 minutes.
---

# Replace the accounts_recent copy with a projection on accounts

## Summary

After 0447 the refreshable MV still rewrites the narrowed `accounts_recent`
(~180 MiB) every 2 minutes, per database. A projection
`(last_seen_ledger, id, home_domain) ORDER BY (last_seen_ledger, id)` on
`accounts`, with `deduplicate_merge_projection_mode = 'rebuild'`, keeps the
list's sort order at ~0.7 GiB/day of extra writes and removes the copy and
its MV.

## Context

Spike on 2026-10-06 (0447,
`notes/R-usage-disk-and-projection-spike-2026-10-06.md`): the projection
works on an RMT on 26.3 and adds ~13 % to `accounts`' writes, but it is
**not read in order**. A page bounded only by the cursor reads the whole
table; a ledger range prunes it (580k rows for the last 1,000 ledgers).
Row-version density: ~690 per ledger at the tip, ~1.3 per ledger across
history.

## Implementation

- Projection on `accounts` (init.sql + an online `ADD PROJECTION` /
  `MATERIALIZE PROJECTION` for production).
- List reader: a ledger window below the cursor, widened until a page fills;
  drop stale versions by checking each candidate's current `last_seen_ledger`
  by `id`. Size the first window from the density above.
- `total_accounts` (`network/queries.rs`) needs its own source once the copy
  is gone: a one-row refreshable `uniqExact(id)` at a long interval, or an
  approximate incremental state.
- Remove `accounts_recent` and `accounts_recent_mv` in both databases.

## Acceptance Criteria

- [ ] `/v1/accounts` pages identical to the copy-based read on a sample that
      includes hot accounts with unmerged versions, both directions
- [ ] Rows read per page measured at the tip and deep in history
- [ ] `total_accounts` from the new source within the stated accuracy
- [ ] MV inner-table writes gone from `part_log`
- [ ] Docs updated — `database-schema-overview.md`
