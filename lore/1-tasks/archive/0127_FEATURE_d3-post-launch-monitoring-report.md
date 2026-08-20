---
id: '0127'
title: 'D3: 7-day post-launch monitoring report'
type: FEATURE
status: done
related_adr: []
related_tasks: ['0036']
tags: [priority-low, effort-small, layer-ops, audit-gap]
milestone: 3
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-04-10'
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from pipeline audit — D3 acceptance criteria #6 requires this but no task existed.'
  - date: '2026-08-19'
    status: done
    who: karolkow
    note: >
      Closed as delivered. The report was written outside this task as part of the
      Milestone 3 evidence package (docs/scf/milestone-3-7day-report.md, committed
      2026-07-25..2026-08-07) and is linked from milestone-3-evidence.md § AC6.
      Window 2026-07-17 13:40Z → 2026-07-24 13:40Z, 7 consecutive 24h periods.
---

# D3: 7-day post-launch monitoring report

## Summary

Deliverable 3 acceptance criteria #6 requires a 7-day post-launch monitoring report
demonstrating system stability, indexing completeness, and API performance under production
load.

## Acceptance Criteria

- [x] Report covers 7 consecutive days of production operation
- [x] Includes: uptime, indexing lag, API latency percentiles, error rates
- [x] Demonstrates no gaps in ledger ingestion during the period
- [x] Delivered to Stellar team — as part of the Milestone 3 submission package
      (`docs/scf/milestone-3-form-answers.md` + evidence PDF), not as a separate
      hand-off

## Outcome

Delivered outside this task. The report lives at
[`docs/scf/milestone-3-7day-report.md`](../../../docs/scf/milestone-3-7day-report.md)
and is summarised and linked from `docs/scf/milestone-3-evidence.md` § AC6.

Window: **2026-07-17 13:40Z → 2026-07-24 13:40Z** — seven consecutive 24-hour
periods measured from the moment the pre-launch Basic Auth gate was removed
(task 0405), not calendar days.

Result over the window: uptime 100.00 % (derived from zero 5XX, no Synthetics
canary deployed), error rate 0.000 % on 30,378 requests served, ingestion lag
7–9 s, 0 ledger gaps on every day. API p95 misses the 200 ms target on 3 of 7
days (worst 553 ms) — the same miss as AC4, accounted for there.

This task was filed 2026-04-10 as an audit-gap placeholder, before the work had
an owner; the real deliverable superseded it.
