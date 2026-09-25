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
  - date: '2026-09-25'
    status: backlog
    who: karolkow
    note: >
      Re-measured on production (read-only). Still true and slightly larger:
      690 GiB/day, 85-98 % of all new-part bytes on the server every day since
      2026-09-01. Measured its effect on a co-located backfill: no measurable
      throughput loss, a CPU burst every 2 minutes that triples tail latency.
      Raising the interval was rejected on review; a projection on `accounts`
      is the candidate direction, not decided. Deferred. See "Re-measured
      2026-09-25".
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

## Re-measured 2026-09-25

All figures read-only from production system tables (`part_log`,
`query_log`, `view_refreshes`, `asynchronous_metric_log`, `parts`).

### Volume and cost per refresh

| Metric                                    | Value                                                                                                                                                     |
| ----------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Inner-table writes per day (`NewPart`)    | 683-690 GiB every day 2026-09-01 → 2026-09-24                                                                                                             |
| Share of all new-part bytes on the server | 85-98 % per day (92-98 % on days without a backfill)                                                                                                      |
| One refresh, 3 h window (90 runs)         | avg 8.4 s, max 18.4 s; reads 27.8M rows; 1.19 GiB memory; **25.4 CPU-s**                                                                                  |
| `balance_aggregates_mv`, same window      | avg 4.3 s; reads 455M rows; **79.5 CPU-s** per refresh                                                                                                    |
| Schedule                                  | all three refreshable MVs (`accounts_recent_mv`, `balance_aggregates_mv`, `pool_activity_mv`) start at second 0 of every even minute                      |
| Disk                                      | RAID1 on 2 × NVMe, 1.72 TiB; average 21.2 MB/s written per device ≈ 1.8 TB/day each (estimate from the average rate) — about one full drive write per day |

### Effect on a co-located backfill

A backfill writing to another database on the same server ran through the
window (12:34-15:34 UTC, ~80k inserts). Odd minutes, when no refresh runs,
are the control group:

| Metric                                          | Even minute (refresh) | Odd minute (none) |
| ----------------------------------------------- | --------------------- | ----------------- |
| Rows written per minute, median                 | 3 243                 | 3 243             |
| Rows per minute, mean ± SE                      | 11 931 ± 2 648        | 13 840 ± 3 639    |
| Inserts per minute                              | 449                   | 450               |
| Server user CPU, seconds 0-4 (24 cores)         | **71 %** (p90 94 %)   | 7 %               |
| Insert p99 inside vs outside the refresh window | 308 ms                | 85 ms             |

No measurable throughput loss: that backfill is paced by its own source, and
the server is idle 88 % of the time on average. The refresh saturates the CPU
for ~5 s every 2 minutes, which triples tail latency for anything running then.
Of that burst, `balance_aggregates_mv` is ~3/4 of the CPU (task 0583);
`accounts_recent_mv` is almost all of the disk writes. A workload bound by
ClickHouse would lose that ~5 s per 2 minutes.

### Numbers for a redesign

| Metric                          | Value                                                                                                                                       |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `accounts`                      | 1.87 GiB, 25.4M physical rows for 14.88M accounts (~11M unmerged duplicates)                                                                |
| `accounts` writes per day       | 691 MiB inserts, 4.71 GiB merges                                                                                                            |
| `accounts` compressed by column | `account_id` 1.29 GiB, `id` 194 MiB, `sequence_number` 135 MiB, `first_seen_ledger` 101 MiB, `last_seen_ledger` 99 MiB, `home_domain` 9 MiB |
| Exact count, `uniqExact(id)`    | 0.39 s, 8.3 CPU-s, 809 MiB                                                                                                                  |
| Exact count, `count() FINAL`    | 0.79 s, 16.7 CPU-s, 740 MiB                                                                                                                 |

The header KPI (`total_accounts`, `network/queries.rs`) reads
`count()` over `accounts_recent`. Network stats are recomputed once per
ledger, so neither exact count above is affordable per request: removing the
copy needs another source for that number.

### Direction (not decided)

Raising `REFRESH EVERY` (the Implementation section below) was **rejected on
review**. It keeps the full-table rewrite and only spaces it out.

The candidate is to keep the second sort order inside `accounts` instead of
in a copy:

- a projection `(last_seen_ledger, id, home_domain) ORDER BY (last_seen_ledger, id)`
  with `deduplicate_merge_projection_mode = 'rebuild'`. 0353 rejected a
  projection on Code 344, which is the default `throw` mode; `rebuild` was
  never measured on `accounts`. 0580 confirmed locally on 26.3 (2026-09-24)
  that it works on an RMT.
- list read in two steps. First, candidates in projection order. Second, one
  lookup by `id` (existing bloom index) for their current version, keeping
  only rows whose `last_seen` equals it. That drops unmerged stale versions in
  both sort directions, and the same lookup returns the display fields.
- the account count from an incremental one-row `uniqCombined64` state fed on
  insert (approximate, ~0.3 % estimated) or a periodic exact count.
- estimated writes ~1 GiB/day instead of ~690 (projection ≈ 16 % of the table
  bytes × current inserts and merges). Unknown until a local spike: rows read
  for page 1 and deep pages (0353 measured 6M rows / 105 ms for a `LIMIT 1 BY`
  variant), and the merge cost of `rebuild`.

Related defects found on the way, out of scope here:

- issuers get `last_seen` bumped on every holder's trustline change
  (`persist/stage.rs`), so they dominate the "recently active" list;
- every write rewrites the whole `accounts` row, clobbering `first_seen`,
  `home_domain` and `sequence_number` (0421).

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
