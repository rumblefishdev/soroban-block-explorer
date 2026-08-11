---
id: '0428'
title: 'OPS: alert on `accounts_recent` MV refresh failure — a stale MV now reports as truth'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0420']
tags: ['area-clickhouse', 'area-ops', 'effort-small', 'priority-high']
links: []
history:
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Spawned from 0420 future work — the last of the four /devils-advocate
      concerns without an owner. 0420 (F1) made the accounts KPI read from
      accounts_recent, so a silent refresh failure now degrades the headline
      total as well as the accounts list.
  - date: 2026-08-11
    status: backlog
    who: karolkow
    note: >
      Re-scoped under the 0455 umbrella - alarm DEFERRED, diagnosis query
      ships in the health runbook instead. Measured against production:
      part_log shows the refresh wrote in EVERY hour of the view's 29-day
      life (694/693 hours incl. the 2026-07-29 outage and a co-tenant
      hammering the box all day), view_refreshes shows zero exceptions and
      retry=0 across all 8 refreshable views. The heavy causes of a stale
      view (box down, disk full) break indexer writes to the same DB and
      page ch-write-failures + backlog-age within minutes; the residual
      class (refresh fails while inserts work - OOM on the FINAL recompute,
      forgotten SYSTEM STOP VIEW, scheduler bug) measured zero occurrences.
      Return conditions for the alarm: (a) a real silent refresh failure is
      observed, or (b) 0447 raises REFRESH EVERY and changes the risk
      profile. The design when it returns: publish
      now()-last_success_time as a metric from the indexer's existing CW
      publish call, bare threshold 3x interval, NOT_BREACHING (absence =
      indexer paused, already alarmed) - recorded in 0455.
---

# Alert on `accounts_recent` MV refresh failure

## Summary

`accounts_recent` is a **refreshable materialized view** that nothing watches.
It is a plain `MergeTree`, so it has no dedup safety net, and `count()` on it is
a **metadata read** — which means a partial, stale or failed refresh is served
as truth, with no error and no visible symptom. Add alerting on
`system.view_refreshes`.

## Context

Task 0420 deduplicated every RMT read in the API. For the accounts KPI it chose
`count() FROM accounts_recent` over `count() FROM accounts FINAL`, because the
MV copy is already one-row-per-account and the count costs nothing (measured:
`accounts FINAL` would merge 14M rows on a **polled** endpoint).

That decision was right on cost and accuracy, but it widened the blast radius of
an MV failure:

| Consumer               | Before 0420                        | After 0420        |
| ---------------------- | ---------------------------------- | ----------------- |
| `/accounts` list pages | `accounts_recent`                  | `accounts_recent` |
| `total_accounts` KPI   | `system.tables.total_rows` (wrong) | `accounts_recent` |

Shared fate with the accounts list means this is **not a new single point of
failure** — but it is now a point of failure with two visible consumers and zero
monitoring. Both degrade quietly: the list shows fewer accounts, the KPI shows a
smaller number, and nothing anywhere says "stale".

Refresh interval is ~2 minutes and a refresh takes ~6 s, so a healthy view is
never more than a couple of minutes behind. There is currently no way to tell a
healthy view from one that stopped refreshing an hour ago.

## Implementation

`system.view_refreshes` already exposes everything needed — no new tables, no
new instrumentation, just a check on columns ClickHouse maintains:

| Column              | Alert when                                      |
| ------------------- | ----------------------------------------------- |
| `status`            | not in (`Scheduled`, `Running`)                 |
| `exception`         | non-empty                                       |
| `last_success_time` | older than **3× the refresh interval** (~6 min) |

- Add the query to whatever already pages on prod ClickHouse. Read-only, single
  row, negligible cost — safe to poll on the existing schedule.
- Alert text should name the two affected consumers (accounts list + network
  KPI) so whoever gets paged knows the user-visible impact without digging.
- The `3×` factor is a starting point: it tolerates one skipped refresh but
  catches a view that has genuinely stopped. Tune after seeing real jitter.

Deliberately **not** in scope: switching the KPI off the MV. 0420 recorded an
exact-count-on-its-own-long-cache fallback if the MV dependency ever hurts —
that is a reaction to this alert firing repeatedly, not a substitute for having
the alert.

## Acceptance Criteria

- [ ] `system.view_refreshes` checked on a schedule for `accounts_recent`
- [ ] Alert fires on bad `status`, non-empty `exception`, or `last_success_time`
      older than 3× the interval
- [ ] Alert names both affected consumers (accounts list, `total_accounts` KPI)
- [ ] Verified by observation, not just by reading the query — confirm the check
      reports healthy against the live view before trusting it

## Design Decisions

### From Plan

1. **Monitor the view, don't remove the dependency.** The MV is the cheap
   correct source; the gap is that nobody watches it. Fixing the watch is a few
   lines, replacing the source costs read quota on a polled endpoint.
