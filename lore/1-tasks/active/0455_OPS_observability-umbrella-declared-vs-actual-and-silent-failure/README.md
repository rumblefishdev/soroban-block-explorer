---
id: '0455'
title: 'OPS: observability umbrella — "declared vs actual, never compared" and "health measured by success"'
type: OPS
status: active
related_adr: []
related_tasks:
  [
    '0454',
    '0428',
    '0400',
    '0312',
    '0434',
    '0232',
    '0406',
    '0250',
    '0237',
    '0449',
    '0448',
    '0447',
    '0403',
    '0382',
    '0087',
    '0127',
    '0367',
    '0368',
  ]
tags: [priority-high, effort-large, observability, ops, umbrella, cross-project]
links:
  - infra/src/lib/stacks/cloudwatch-stack.ts
  - crates/db-clickhouse/schema/init.sql
  - crates/indexer/src/handler/mod.rs
history:
  - date: 2026-07-29
    status: backlog
    who: karolkow
    note: >
      Spawned after two investigations in two days, both of which ended in the
      same place: the data existed, but nothing compared it or alarmed on it, so
      it took hours of manual queries across six systems to find. Sixteen open
      tasks turned out to be instances of two recurring defects rather than
      sixteen separate problems. This task owns the two defects; the children own
      their instances.
  - date: 2026-08-04
    status: active
    who: karolkow
    note: >
      Activated, and grounded against production with a read-only principal —
      see notes/R-production-evidence-2026-08-04.md. Both defects confirmed from
      live data; the dead alarm filter is proved by its own metric (~108k events
      evaluated per day, never a single match). Three findings change the plan: a
      fourth defect (a latched alarm is a mute alarm — two instances, one running
      32 days), a hard constraint (three of seven lag events in 30 days were
      planned pauses, so a naive threshold alarm would page monthly for
      maintenance), and queue age alone being insufficient because failures
      draining to the DLQ make it read green. Child triage in
      notes/S-child-task-triage.md: eight genuine instances, not sixteen.
  - date: 2026-08-05
    status: active
    who: karolkow
    note: >
      Surveyed all four surfaces — logs, metrics, dashboard, alarms — and checked
      each statement against the account; results in
      notes/R-deep-review-findings-2026-08-04.md. Three earlier statements did
      not survive measurement and are recorded as refuted, including a proposed
      enrichment regression: sampling 16 issuer domains directly found no case
      where metadata is published and the row is blank, so the blank rows track
      the population rather than the pipeline. The single periodic comparator is
      set aside — the infrastructure comparison already runs and its output was
      read twice while the delta stayed open. Widest-effect candidate is an
      `infra/` test asserting that each filter string and metric namespace
      appears in `crates/`, which covers the class rather than an instance.
---

# Observability umbrella — recurring defects, not isolated bugs

## Summary

Sixteen open tasks looked like sixteen problems. They are a small number of
recurring defects, each with several instances:

1. **Declared vs actual, never compared.** We write down how things should be,
   reality drifts, and nothing continuously checks one against the other.
2. **Health measured by success, so failure is silent.** Signals are emitted on
   the success path, so when a thing breaks its signal does not go bad — it goes
   _absent_, and absence is not an alarm state by default.
3. **No owner dimension.** Shared account and shared database box, no
   attribution, so "who caused this" is always an investigation ([[0449]]).
4. **A latched alarm is a mute alarm.** CloudWatch notifies on state
   _transition_. An alarm that enters ALARM and stays there is silent from the
   second minute on, and cannot signal the next incident because it is already in
   the target state.

None of this is a monitoring-tooling gap: the data existed in every case. What is
missing is a comparator, an inverted signal, and an alarm design that re-arms.

## Evidence

Two incidents produced this task: a 19-minute total ingestion outage on
2026-07-29 ([[0454]]) where seven alarms were live and none could fire, and a
cost investigation on 2026-07-28 that took roughly a day because the account
bills two projects as one ([[0449]]).

Both were re-checked against production on 2026-08-04. Full measurements in
[R — production evidence](notes/R-production-evidence-2026-08-04.md); the
load-bearing results:

- `indexer-ch-write-failures` evaluates ~108 000 log events a day and its metric
  has **never been non-zero**, including every 5-minute bucket of the 0454
  outage. Its filter string was removed by the doorbell rewrite (`bee784df`);
  the code now emits `reconcile failed — will redeliver doorbell`.
- Indexer Lambda `Errors` was **0 on every day** of a 30-day window containing
  seven lag events. A total stall cannot reach that metric.
- **Three of those seven lag events were planned pauses**, confirmed from the
  audit log against the documented pause procedure.
- Two alarms have been latched in ALARM for 15 and 32 days. In 90 days no alarm
  in the account has changed state other than that pair and one manual test.

A wider survey on the same date covered all four surfaces — logs, metrics,
dashboard and alarms — and is recorded in
[R — measured state](notes/R-deep-review-findings-2026-08-04.md). It carries the
per-surface measurements, three statements that measurement refuted, and the
candidate work ordered by breadth of effect. Two results from it change this
task's shape:

- The scope is wider than the alarm set. The three Lambdas take different
  telemetry routes, seven dashboard widgets have no alarm and two alarms have no
  widget, and log retention holds three different values.
- Notification volume concentrates in one alarm: 24 of 32 transitions in a month
  originated from 48 errors, all self-clearing within minutes.

## Defect 1 — declared vs actual, never compared

| Declared in             | Actual                 | Instance                                                                   |
| ----------------------- | ---------------------- | -------------------------------------------------------------------------- |
| `init.sql`              | prod schema            | [[0400]]                                                                   |
| CDK app                 | deployed stacks        | [[0312]] — `make diff-production` already reports it; nothing schedules it |
| alarm filter strings    | strings the code emits | [[0454]] defect 6 — dead since the rewrite                                 |
| our index               | the chain              | [[0382]] — the strongest comparator in the set                             |
| protocol tables in code | the chain              | [[0434]]                                                                   |
| test files in repo      | what CI runs           | [[0406]] — 25 e2e files no pipeline has executed                           |

A single periodic comparator was the first candidate and is **set aside** — see
[R — measured state](notes/R-deep-review-findings-2026-08-04.md). For
infrastructure the comparison already runs: `make diff-production` reports this
delta, its output was read on 2026-06-22 and measured again on 2026-07-27, and
the delta was still pending on 2026-08-04. Adding a schedule to a report that is
already produced does not change that.

Each row instead takes the cheapest check that sits where it can be acted on,
and they are deliberately different mechanisms:

| Instance | Check                                                                                                                             |
| -------- | --------------------------------------------------------------------------------------------------------------------------------- |
| [[0312]] | A confirmation step at deploy. `make deploy-production` is `--all`, so an undeployed delta ships with the next deploy of anything |
| [[0406]] | Not a comparison — the gap is recorded since 2026-07-17. Add ClickHouse to the CI job                                             |
| [[0400]] | A one-off reconciliation of four enumerated items; whether schema changes route through the repo is a separate question           |
| [[0434]] | A unit test asserting the match arms cover the crate's enum                                                                       |
| [[0382]] | Overlaps [[0431]], which is building a differential oracle against `stellar-xdr`                                                  |
| [[0454]] | Covered by an `infra/` test asserting each filter string appears in `crates/` — one check for the whole class                     |

## Defect 2 — health measured by success

| Signal                | Why silence looks healthy                                                      | Instance |
| --------------------- | ------------------------------------------------------------------------------ | -------- |
| `IngestionLagSeconds` | published after a successful persist — a failed persist emits nothing          | [[0454]] |
| refreshable MV        | reports rows from metadata; a failed refresh is indistinguishable from success | [[0428]] |
| host reboot flag      | the box knows; nothing asks                                                    | [[0237]] |

Exactly one of the eight deployed alarms sets `treatMissingData: BREACHING`. Fix
shape: **watch upstream of the thing that breaks, and treat absence as failure.**

## Constraint — planned pauses look exactly like failure

The indexer is deliberately paused for maintenance and re-index work, by
disabling the SQS event-source-mapping (`docs/deployment.md:202`). Three of the
seven lag events above 10 minutes in the last 30 days were exactly that.

So an absence-based alarm — queue age, or the co-tenant's `Invocations < 1`
pattern — would have paged three times in one month for planned work. An alarm
that cries wolf monthly gets muted, and a muted alarm is the failure this whole
task exists to fix. **The system has no machine-readable notion of "planned
pause", and any absence alarm is undeployable until it does.**

Candidate answers, cheapest first:

- CloudWatch **composite alarm with an actions suppressor** — a native feature,
  no code: the suppressor is a second alarm that is ALARM while maintenance is
  declared.
- Make the pause its own declaration: the event-source-mapping's `State` is
  already the ground truth, so a small scheduled check can publish a "paused"
  metric for the suppressor to read.

Do not solve this by widening the threshold. That trades one silent hour for
another and is exactly the plaster this task exists to avoid.

## Defect 4 — a latched alarm is a mute alarm

Two live instances: one DLQ alarm sat in ALARM for 15 days — a lag event passed
inside that window with the alarm already red — and another has been in ALARM for
32 days while its queue grew, with neither growth event producing a notification.

Fix shape: alarm on **change**, not on level. A non-empty DLQ is a condition to
clear, not a page to repeat; a DLQ that _grew_ is an event. Every level-triggered
alarm needs a stated answer to "what re-arms it".

Related: queue age alone is insufficient. When failures drain to the DLQ the main
queue empties, so the age metric reads green while ingestion is still dead
(measured 2026-07-10: 125 consecutive hours of DLQ growth). The signal must be a
pair.

## Implementation — ordered by return

### 1. Read-only credentials for the assistant workspace — **done**

Granted 2026-08-04 as a read-only SSO role. Both investigations were slow mostly
because AWS commands had to be hand-relayed; that asymmetry is gone. The grant is
the broad managed read-only policy, wider than the narrow action list originally
specified here — worth narrowing if the account ever holds data that should not
be readable.

### 2. Invert the signals, with the pause constraint solved first

- alarm on `ApproximateAgeOfOldestMessage` for `production-ledger-ingest`,
  **paired** with a DLQ-growth alarm, **suppressed** during declared maintenance
- `treatMissingData: BREACHING` where absence genuinely means failure — plus a
  one-line comment on every alarm justifying its setting either way
- MV freshness measured from **data recency**, not metadata ([[0428]])
- repair `indexer-ch-write-failures`: match a stable field, not prose

### 3. The comparator

One scheduled job printing deltas for every row of defect 1, publishing to the
SNS topic that already reaches the team. Start with the two that already bit us —
schema and CDK — then alarm filters vs emitted strings, then tests vs CI.

### 4. Cost attribution and cost detection ([[0449]])

All 11 stacks already emit the `Project` tag; the missing step is activating it
in Billing, which is not retroactive. Detection has no owner at all: the "nobody
noticed the spend rise for three weeks" problem is proposed in [[0449]]'s prose
and exists in no task. Fold it into [[0449]]'s acceptance criteria or spawn it —
a per-project budget with a forecast trigger, and anomaly detection for step
changes, both routed to the existing SNS topic.

## Tooling decision — no external log platform yet

Considered and rejected for now (Datadog / Grafana Cloud / Axiom):

- Neither incident would have been caught by one. Both needed an alarm on
  **absence**, not a log search.
- Shipping logs off the database box is the same egress line that drove the July
  cost investigation; the proxy access log alone holds 11.5M entries.
- It adds a vendor, a bill and a secret to rotate, for two people.

If a single sink is wanted later, **CloudWatch** is the default: the Lambdas are
already there, it is one credential surface and one alarm engine, and the box can
ship a filtered stream into it. **ClickHouse is explicitly rejected as the sink for
the critical path** — observability living inside the system it observes goes
quiet exactly when it is needed. Revisit when there are more than two people on
call, or when retention beyond CloudWatch's is required.

Two retention facts bound every future investigation: Lambda log groups keep
**30 days**, and there is **no CloudTrail trail** — only the free 90-day event
history, which cannot be exported or queried beyond that. Anything older than a
month is unanswerable by construction.

## Acceptance Criteria

- [x] Read-only AWS principal exists and is usable from the assistant workspace
- [ ] Planned pauses are machine-readable, so an absence alarm can tell
      maintenance from failure — verified by pausing and confirming no page
- [ ] An alarm fires on ingestion stall — verified by simulating a stall
- [ ] Every alarm's `treatMissingData` reviewed and justified in a comment
- [ ] Every level-triggered alarm has a stated re-arm answer; no alarm can sit
      latched and mute
- [ ] Comparator runs on a schedule and reports schema + CDK deltas; its output is
      seen by a human without anyone asking for it
- [ ] Alarm filter strings verified against the strings the code actually emits
- [ ] Cost allocation tags active; a per-project cost answer takes minutes, and a
      step change in spend raises an alert
- [ ] Each child task either closed by this work or explicitly re-scoped —
      triage in [S — child triage](notes/S-child-task-triage.md)
- [ ] **Docs updated** — `docs/runbooks/**` gains "how do I tell if it is broken",
      naming the signals and where they live
- [ ] **API types regenerated** — N/A, no API surface change

## Notes

- Deliberately NOT in scope: replacing CloudWatch, dashboards, tracing coverage,
  frontend telemetry ([[0087]]). Those are worth doing and none of them would have
  caught either incident.
- The cross-project half needs the other team: shared infrastructure with no
  shared signal means each side is blind to the other. Minimum viable version is
  one health signal for the box that both teams can see. The co-tenant project in
  the same account already runs several of the patterns proposed here — copying
  them is cheaper than designing them.
- The original "sixteen instances" is inflated. Eight are genuine instances,
  three are a cost cluster, five should be re-scoped out.
