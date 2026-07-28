---
id: '0447'
title: 'PERF: accounts_recent_mv rewrites a 950 MiB table every 2 minutes — 661 GiB/day'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0385', '0403']
tags: [phase-future, effort-small, priority-high, performance, clickhouse, ops]
links: []
history:
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Measured during a cost investigation. 0385 shipped the MV and flagged its
      recompute as an unmeasured risk; 0403 owns that risk but asks about the
      memory cap, not the write volume. The write volume has now been measured
      and is 661 GiB/day.
---

# accounts_recent_mv rewrites the whole table every 2 minutes

## Summary

`accounts_recent_mv` is a refreshable materialized view that runs
`SELECT ... FROM accounts FINAL` **every 2 minutes** and rewrites its entire
target table. The target holds ~950 MiB / 14.4M rows, so each refresh writes
~940 MiB. That is **661 GiB of writes per day, 241 TB per year**, on a box that
is already 66 % full.

## Measured

Read from `system.part_log` on production, 2026-07-28:

| Metric                            | Value                                                |
| --------------------------------- | ---------------------------------------------------- |
| MV created                        | 2026-07-13 15:18:43                                  |
| Writes to its inner table, hourly | 420 parts / **27.56 GiB**, flat since 16:00 that day |
| Daily                             | **661 GiB**, 10.33 billion rows                      |
| `accounts_recent` actual size     | 948.79 MiB, 14,395,971 rows                          |
| Merge load the same day           | 372 GiB `MergeParts`, 84 GiB `RemovePart`            |

The step is visible to the hour: `default` went from 5–13 GiB/day of inserts on
1–12 July to 480 GiB on the 13th and ~675 GiB/day flat thereafter.

## What this does NOT cost

**Nothing in AWS.** The refresh runs entirely server-side on the ClickHouse host,
so it generates no data transfer and no Lambda time. It was ruled out as the
cause of the July AWS bill increase — that was a different project's ingestion.

It also does **not** degrade API latency: daily average API Lambda duration over
the ten days after the MV landed is 65–142 ms, against a June average of 205 ms.

What it does cost is NVMe write endurance and the IO/CPU headroom of a
single-node box with no failover.

## Context

The MV is not gratuitous. Task 0385 built it deliberately so the account-list
browse could seek an ordered read model instead of paying `accounts FINAL` on
every request, after a ReplacingMergeTree projection was rejected by CH 26.3
(Code 344) and re-keying `accounts` was ruled structurally impossible.

Task 0385's own history entry flags the recompute as
_"a live risk carried, not closed; 0403 owns it"_. 0403 is still in backlog and
scopes the question as a **memory** check against the prod 6 GB cap. Nobody
looked at write volume.

## Implementation

- Establish what freshness the account-list browse actually needs. The list is
  ordered by `last_seen_ledger`; a browse view almost certainly tolerates far
  more than 2 minutes of skew, and `init.sql` already says as much
  (_"≤interval-stale ... browse is fine"_).
- Raise `REFRESH EVERY` accordingly. 30 minutes cuts the write volume ~15× to
  ~44 GiB/day; 1 hour cuts it ~30×.
- Check the same question for `balance_aggregates_mv`, which shares the 2-minute
  interval by precedent. Its volume is far smaller (3.33 GiB/day) but the
  reasoning is the same.
- Re-measure `system.part_log` for a full day after the change.

## Acceptance Criteria

- [ ] Required freshness for the account-list browse stated explicitly, with the
      consumer that sets the requirement named
- [ ] `REFRESH EVERY` raised; new daily write volume measured from `part_log`
- [ ] `balance_aggregates_mv` interval reviewed against the same question
- [ ] `/v1/accounts` responses unchanged apart from the documented staleness skew
- [ ] 0403's memory question either answered here or explicitly left with 0403
