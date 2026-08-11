---
id: '0455'
title: 'OPS: observability umbrella — "declared vs actual, never compared" and "health measured by success"'
type: OPS
status: active
related_adr: ['0054']
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
  - date: 2026-08-10
    status: active
    who: karolkow
    note: >
      Large execution block landed. Defect-1 instances: 0406 closed (CI
      provisions ClickHouse and runs the gated e2e suite, sabotage-verified
      red), 0312 closed (deploy target shows the full diff and asks before
      --all; parked delta deployed, prod diff clean), 0454's dead filter fixed
      fundamentally (structured field alarm="ch_write_failure" plus a
      declared-vs-emitted CI guard over a comment-stripped crates/ corpus; an
      SSOT/codegen variant was built and reverted as overkill at 2 contracts),
      0434 exhaustive matches, 0400 reconciled both directions with the
      consumer-less idx_oaa_transaction_id dropped from prod and init.sql.
      Defect-2 groundwork: treatMissingData justified on every alarm; stall
      math IF(received>0, age, 0) measured to separate pause from failure,
      pending merge. Cost package (0449) executed: Project tag activated from
      the management account with historical backfill, Galexie task-tag
      propagation, account-wide Cost Anomaly Detection to the existing SNS
      topic, costs runbook, leftover staging-Postgres snapshots deleted,
      hand-provisioned secrets tagged. Remaining: alarm core merge
      (stall + DLQ-growth + re-arm), MV freshness from data recency, budgets,
      health runbook, final deploy + verification.
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
missing is an inverted signal, an alarm design that re-arms, and — for defect 1 —
a check placed where somebody can act on it. Note that last phrasing: the first
draft of this task said "a comparator", and that turned out to be the wrong
shape. See defect 1 below.

There is also a fifth defect, one level up: **each of the sixteen tasks proposes
its own detection AND its own notifier.** Built as written that is five
schedulers, four delivery paths and three secrets for two people — and 0237's
24-hour cooldown sentinel file is a hand-rolled alarm state machine in bash. No
task is wrong; nothing existed to say "do not grow your own notifier". That
statement is now [ADR 0054](../../../2-adrs/0054_one-alarm-engine-and-three-rules-for-alarms.md),
and it is what stops this recurring after the umbrella closes.

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

| Instance | Check                                                                                                                                                                                                                                                               |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [[0312]] | **Done 2026-08-10** — confirmation step at deploy shipped, parked delta deployed, prod diff clean; 0312 archived                                                                                                                                                    |
| [[0406]] | **Done 2026-08-06** — CI provisions CH from compose, runs both gates, sabotage-verified red; 0406 archived                                                                                                                                                          |
| [[0400]] | **Measured 2026-08-06/10** — both directions reconciled; the consumer-less `idx_oaa_transaction_id` found declared-but-unused, dropped from prod and `init.sql`; docs half remains in the child task                                                                |
| [[0434]] | **Done 2026-08-06** — `_ => {}` arms replaced with exhaustive matches (drift is now a compile error); config-setting names derived from `ConfigSettingId`; remaining gaps measured and recorded in the child task                                                   |
| [[0382]] | Overlaps [[0431]], which is building a differential oracle against `stellar-xdr`                                                                                                                                                                                    |
| [[0454]] | **Done 2026-08-06** — filter keyed on the structured field `alarm = "ch_write_failure"`; `infra/src/lib/stacks/declared-vs-emitted.spec.ts` asserts every filter contract exists in `crates/` (comment-stripped corpus), wired into CI with Rust-aware cache inputs |

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
pattern — pages during planned work. The first draft of this task treated that
as a blocker ("any absence alarm is undeployable until pauses are
machine-readable") and designed a discriminator: `IF(received > 0, age, 0)`,
measured to cleanly separate pause (received = 0) from failure (received
53-54/5 min).

**Resolved 2026-08-10 the other way — operator decision.** One knowing page
per planned pause is cheap: the person who just paused the indexer knows why
the page arrived. The discriminator was withdrawn as overcomplication with its
own blind spot (an ACCIDENTALLY disabled event source reads as "pause" →
unbounded silence, which then needs a second backstop alarm to patch). The
bare `ApproximateAgeOfOldestMessage` threshold covers stall, planned pause and
forgotten-disable with one alarm and no suppression logic. Full reasoning in
ADR 0054 (rule 4 rewritten + "Considered and withdrawn").

## Defect 4 — a latched alarm is a mute alarm

Two live instances: one DLQ alarm sat in ALARM for 15 days — a lag event passed
inside that window with the alarm already red — and another has been in ALARM for
32 days while its queue grew, with neither growth event producing a notification.

Resolved 2026-08-11 after two withdrawals in one day (both recorded with
return conditions in ADR 0054). Inspecting the actual DLQ contents showed
the stuck messages were dead-issuer-domain fetches, and the 15/32-day
latches were a **missing drain procedure**, not a wrong alarm shape. Final
shape, operator-proposed and measurement-endorsed (30 d of worker logs:
100% of "transient" retries were connect-level dead domains — 6 keys,
~1000 wasted retries, one key retried 668× in 83 min — and 0% genuine
blips): **connect-level fetch failures classify permanent** and write the
sentinel immediately (`http_transient.rs`; 429/5xx/post-connect timeouts
stay transient; `--retry-sentinels` repairs any host that comes back). The
DLQ therefore receives only DB incidents and poison pills; the LEVEL
alarms stay (ADR 0054 rule 2 carve-out — level is correct where policy
forces zero steady state); `docs/runbooks/dlq.md` carries the drain
procedure (doorbells: purge; enrichment: redrive after the fix); every
level alarm states its re-arm answer in a comment. Withdrawn: `DIFF`
growth alarms, and a last-chance-sentinel intake plug (memory machinery
for a memoryless problem).

Related: queue age alone is insufficient. When failures drain to the DLQ the
main queue empties, so the age metric reads green while ingestion is still
dead (measured 2026-07-10: 125 consecutive hours of DLQ growth). The
backlog-age alarm and the DLQ level alarms are that pair.

## Implementation — ordered by return

### 1. Read-only credentials for the assistant workspace — **done**

Granted 2026-08-04 as a read-only SSO role. Both investigations were slow mostly
because AWS commands had to be hand-relayed; that asymmetry is gone. The grant is
the broad managed read-only policy, wider than the narrow action list originally
specified here — worth narrowing if the account ever holds data that should not
be readable.

### 2. Invert the signals, with the pause constraint solved first

- **In code 2026-08-10** — bare-threshold alarm on
  `ApproximateAgeOfOldestMessage` for the ingest queue (120 s × 3 min, from
  the measured bimodal distribution); the `IF(received > 0, age, 0)`
  discriminator withdrawn per rewritten ADR 0054 rule 4 — one knowing page
  per planned pause is the accepted cost. DLQ-growth pair still pending
  (R3)
- **Done 2026-08-06** — every alarm's `treatMissingData` carries a one-line
  justification comment; the write-failure threshold rewritten in doorbell
  units
- **Resolved 2026-08-11 by measurement** — [[0428]] re-scoped: the refresh
  wrote in every hour of the view's 29-day life (694/693, incl. the 0454
  outage), heavy causes already page via ch-write-failures/backlog-age, the
  residual class measured zero. Diagnosis query goes into the health
  runbook; the alarm design is recorded in 0428 with return conditions
- **Done 2026-08-06** — `indexer-ch-write-failures` keys on the structured
  field `alarm = "ch_write_failure"` emitted at both hard-failure sites; an
  SSOT/codegen variant was built, judged overkill at 2 contracts, and
  reverted (return threshold recorded in the guard's commit message)

### 3. The comparator

One scheduled job printing deltas for every row of defect 1, publishing to the
SNS topic that already reaches the team. Start with the two that already bit us —
schema and CDK — then alarm filters vs emitted strings, then tests vs CI.

### 4. Cost attribution and cost detection ([[0449]]) — **done 2026-08-10** (detection live after deploy)

Attribution: the `Project` cost-allocation tag was activated from the
organization management account, with a historical backfill requested; the
largest untagged share (Galexie Fargate tasks — ECS bills the task, not the
service) is fixed with `propagateTags: SERVICE` in the ingestion stack.
Hand-provisioned secrets tagged; leftover us-east-1 snapshots of the retired
staging Postgres deleted. Per-project cost is now one Cost Explorer view
(runbook: `docs/runbooks/costs.md`).

Detection: account-wide Cost Anomaly Detection (native, free) — a per-SERVICE
monitor plus an IMMEDIATE SNS subscription to the existing alarm topic, so it
covers both projects, tagged or not. Threshold in
`infra/envs/production.json` (`costAnomalyAlertThresholdUsd`), topic policy
grants `costalerts.amazonaws.com` publish. Committed in
`infra/src/lib/stacks/cloudwatch-stack.ts`; live after the next CloudWatch
deploy. Per-project budgets are **dropped** (decision 2026-08-10, recorded in
[[0449]]): attribution + anomaly detection suffice for two people, and the
slow-creep gap is accepted and named in the costs runbook.

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
- [x] The planned-pause constraint has a stated answer — resolved 2026-08-10
      the opposite way from the first draft: pauses are NOT machine-readable;
      one knowing page per pause is the accepted design (ADR 0054 rule 4).
      Verified at deploy by pausing and confirming exactly one page
- [ ] An alarm fires on ingestion stall — in code
      (`production-ingestion-backlog-age`); AC checks off after a simulated
      stall against the deployed alarm
- [x] Every alarm's `treatMissingData` reviewed and justified in a comment
      (2026-08-06)
- [x] Every level-triggered alarm has a stated re-arm answer; no alarm can sit
      latched and mute (2026-08-11 — DLQs: drain per `docs/runbooks/dlq.md`,
      standing content never accepted; Galexie disk: act-before-ceiling
      comment; latch-proofing verified at deploy by the drained-DLQ
      test-message gate)
- [ ] Comparator runs on a schedule and reports schema + CDK deltas; its output is
      seen by a human without anyone asking for it
- [x] Alarm filter strings verified against the strings the code actually emits
      (2026-08-06 — `declared-vs-emitted.spec.ts`, enforced in CI)
- [ ] 0403's deferred measurement executed after the next deploy + a drain:
      the sep1 issuer resolve reads ~24.6k rows/call in `system.query_log`
      (not ~24.9M), and the `dev_read` vs `ingestion_writer` read-count
      discrepancy explained or recorded as still open
- [ ] Cost allocation tags active; a per-project cost answer takes minutes, and a
      step change in spend raises an alert (tags active + backfill, runbook
      shipped; anomaly detection committed — checks off after deploy)
- [ ] Dashboard↔alarm coverage reconciled (7 widgets without alarms, 2 alarms
      without widgets) — including a stated dashboard answer for the new
      cost-anomaly alert
- [ ] Each child task either closed by this work or explicitly re-scoped —
      triage in [S — child triage](notes/S-child-task-triage.md) (0406, 0312
      closed and archived)
- [x] **Docs updated** — `docs/runbooks/**` gains "how do I tell if it is broken",
      naming the signals and where they live (2026-08-11:
      `docs/runbooks/health.md` — the four-sentence convention, the coverage
      matrix with every cell decided, symptom→first-move paths, an escape
      hatch, and the feedback rule; plus `api-5xx.md`, `dlq.md`, `costs.md`
      shipped earlier)
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
