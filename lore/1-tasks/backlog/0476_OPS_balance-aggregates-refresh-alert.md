---
id: '0476'
title: 'OPS: alert on a stalled balance_aggregates_mv refresh (system.view_refreshes)'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0310', '0331']
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

## Implementation

Source query (from 0310/0331):

```sql
SELECT view, status, last_success_time, exception
FROM system.view_refreshes
WHERE view = 'balance_aggregates_mv';
```

Alert when `exception != ''` OR `now() - last_success_time > ~10 min` (a few
missed 2-minute cycles).

- Decide the transport: the CH box is Hetzner (no CloudWatch agent on it), so
  either a cron + webhook on `ch-prod-01` (Ansible-managed, see
  `infra-hetzner/`), or a small scheduled Lambda in the CloudWatch stack that
  queries CH over mTLS like the API does. Pick whichever the existing
  monitoring already leans on.
- Pairs with the existing CH monitoring — do not build a new channel for one
  alert.

## Acceptance Criteria

- [ ] Alert fires on `exception != ''` or `last_success_time` older than ~10 min.
- [ ] Verified end-to-end once (e.g. by temporarily suspending the MV in a
      controlled window, or lowering the threshold to trigger).
- [ ] Runbook note in 0331's `ops-runbook.md` updated to point at the alert.
