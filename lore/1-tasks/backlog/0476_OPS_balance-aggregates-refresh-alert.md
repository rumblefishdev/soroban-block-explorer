---
id: '0476'
title: 'OPS: alert on a stalled balance_aggregates_mv refresh (system.view_refreshes)'
type: OPS
status: backlog
related_adr: ['0062']
related_tasks: ['0310', '0331', '0428', '0447']
tags:
  ['phase-future', 'effort-small', 'priority-low', 'clickhouse', 'monitoring']
links: []
history:
  - date: '2026-08-12'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0310's last open AC. Confirmed before spawning: 0331/0339
      carry only a MANUAL check (`0331_…/ops-runbook.md` queries
      `system.view_refreshes`), and nothing in `infra/` wires an alert — so
      this is not a duplicate.
  - date: '2026-08-12'
    status: backlog
    who: stkrolikiewicz
    note: >
      Renumbered 0474 → 0476: id 0474 collided with
      `0474_FEATURE_inferara-logo-attribution`, created the same day on the
      other side of the develop/master split (PR #394) and invisible from here
      until the back-merge in PR #397 put both on one branch. This backlog task
      is the cheaper side to move — the FEATURE one is already referenced by
      merged commits. 0475 was taken meanwhile, hence 0476. Content unchanged:
      only the id, the filename and the three references from 0310 moved.
  - date: '2026-11-27'
    status: backlog
    who: stkrolikiewicz
    note: >
      Placed under ADR 0062 (PR #762, slimmed after Karol's review): the
      transport is decided, and the alarm stays closed by 0428's decision
      until one of its return conditions is met. The "Under ADR 0062" section
      below carries 0428's design and two notes from that review for when it
      returns.
---

# OPS: alert on a stalled `balance_aggregates_mv` refresh

## Summary

`balance_aggregates` (asset supply/holders for the API) is recomputed every
2 minutes by the refreshable MV `balance_aggregates_mv`. A failed refresh
degrades safely (previous table stays — stale, never empty), but a silently
stalled one (OOM/timeout/lock) has **no signal**: the figures just age. Wire
an automated alert.

## Context

Inherited from 0310 (which closed the dead-column cleanup). The check itself
exists as a manual runbook query in task 0331's `ops-runbook.md`; this task is
about making it fire on its own.

## Under ADR 0062

- **Not built yet, by decision.** 0428 measured the same class on all 8
  refreshable views (0 exceptions over 29 days) and closed the alarm with two
  return conditions: a silent refresh failure observed, or 0447 raising
  `REFRESH EVERY`. Until one is met, this stays in the backlog.
- **When it returns, 0428's design, on CloudWatch's path** (ADR 0062, rule 1:
  what our Lambdas measure): the indexer publishes `now() - last_success_time`
  from its existing `put_metric_data` call, with a bare threshold of 3× the
  refresh interval and missing data `NOT_BREACHING` (absence means the indexer
  is paused, which is alarmed already).
- **Two notes from the review of the first draft (PR #762):**
  - don't count every `exception` as failing: stellar-prices-api's
    `refresh_waits.rs` skips a `Disabled` view (the `SYSTEM STOP VIEW` is its
    own signal) and a `Running` one (it may be retrying that very error), or
    the alarm fires on every retry and every planned stop;
  - drill the pubnet alarm, not only testnet's.

## Implementation

Source query (from 0310/0331):

```sql
SELECT view, status, last_success_time, exception
FROM system.view_refreshes
WHERE view = 'balance_aggregates_mv';
```

Alert when `exception != ''` OR `now() - last_success_time > ~10 min` (a few
missed 2-minute cycles).

- The transport is decided (ADR 0062, above): the indexer's CloudWatch
  metric, not a cron, a webhook or a new Lambda.
- Pairs with the existing CH monitoring — do not build a new channel for one
  alert.

## Acceptance Criteria

- [ ] Alert fires on `exception != ''` or `last_success_time` older than ~10 min.
- [ ] Verified end-to-end once (e.g. by temporarily suspending the MV in a
      controlled window, or lowering the threshold to trigger).
- [ ] Runbook note in 0331's `ops-runbook.md` updated to point at the alert.
