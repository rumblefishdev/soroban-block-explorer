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
  - date: 2026-08-18
    status: active
    who: karolkow
    note: >
      Consolidation stretch (08-11..08-18) closed by PR #422 to develop.
      Two live incidents validated the thesis in one week: 2026-08-12
      galexie-lag page = AWS Fargate task replacement (~25 min envelope,
      recorded as a health.md symptom path), and 2026-08-14 disk pressure
      on the shared ClickHouse box stopped ingestion ~9.5 h with ONE quiet
      page (DLQ level, 5 h in) - the backlog-age alarm on this branch
      would have paged at minute 4. Lessons written into health.md
      (Code 243 playbook, continuity-query fix, measured 0237 cost).
      0400 closed and archived (docs half done; drift-gate AC deferred to
      the comparator AC here). API-gateway stage cache decided NOT
      ADOPTED after measurement (browser tiers + in-process moka are the
      live caches, Cloudflare is DYNAMIC; return condition = measured
      origin pressure, preferred lever a Cloudflare cache rule).
      Dashboard reconciliation: a whole-batch rewrite was withdrawn by
      the operator to a stash and REDONE as per-decision atomic changes -
      the process lesson stands: one decision, one diff, one approval.
      First slice shipped: two never-populated widgets removed (cache
      hit/miss - cluster never provisioned; cold starts - InitDuration is
      not a CloudWatch metric, measured empty), the freshness widget
      moved onto the alarm's own doorbell signal, the backlog-age widget
      added with the paging threshold drawn from config, the convention
      comment states the rule. ch-write-failures threshold cut 10 -> 0
      (operator decision, the 5xx zero-tolerance rule; >10 would never
      see a single poison-pill ledger). Two develop merges absorbed
      (271 + 76 commits; the 0390 tag-driven CI deploy is now armed for
      Compute+SPA, Ingestion/CloudWatch stay laptop-only). Second slice
      planned for a follow-up PR from fresh develop: worker-errors and
      CH-write-failures widgets (with the guard learning
      MetricFilter-minted names), cost graph, runbook matrix cells,
      open decisions (Slack-chain witness, X-Ray, canary, retention).
  - date: 2026-08-18
    status: active
    who: stkrolikiewicz
    note: >
      Release-scope slice, found while assembling the release note for the
      next tag: the CDK app declares ten stacks, the tag deployed one, and
      nothing compared the two - this task's own alarm set was sitting in
      exactly that gap, invisible because `cdk diff` only ever covered the
      stack being deployed. Two changes, deliberately separate. (1) The diff
      is now wider than the deploy - `cdk diff --strict` over ALL stacks on
      every run, as the release's drift record. (2) The tag carries a
      selector: `production-<date>-<N>[-SELECTOR]` where empty keeps today's
      Compute+SPA default, `all` deploys every differing stack, `web` is
      SPA-only, and anything else is pasted onto `Explorer-production-`
      verbatim. Mapping in `infra/scripts/deploy-scope.sh` with a
      `--self-check` wired into the typescript CI job. `--fail` on the diff
      (auto-deploy whatever differs) was REJECTED: cdk diffs against deployed
      state, not against the last tag, so it fires on parked deltas and
      console edits and would ship them unreviewed - the 0312 stowaway, which
      is why the Makefile grew a typed `yes` the same week. Grammar is now
      anchored, closing a real hole: the trigger is `production-*`, so any tag
      matching that glob used to deploy the release set. Docs: deployment.md
      Releases section rewritten, TL;DR and CloudWatch rows note the tag form,
      /release skill steps 2/5/6 updated.
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

| Instance | Check                                                                                                                                                                                                                                                                                                                                            |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [[0312]] | **Done 2026-08-10** — confirmation step at deploy shipped, parked delta deployed, prod diff clean; 0312 archived                                                                                                                                                                                                                                 |
| [[0406]] | **Done 2026-08-06** — CI provisions CH from compose, runs both gates, sabotage-verified red; 0406 archived                                                                                                                                                                                                                                       |
| [[0400]] | **Done 2026-08-14, archived** — both directions reconciled (2026-08-06/10; the consumer-less `idx_oaa_transaction_id` dropped from prod and `init.sql`); docs half closed: last live-tense Postgres claims rewritten to the CH model, skip-index inventory documented with consumers. Drift-gate AC deferred HERE (the open comparator AC below) |
| [[0434]] | **Done 2026-08-06** — `_ => {}` arms replaced with exhaustive matches (drift is now a compile error); config-setting names derived from `ConfigSettingId`; remaining gaps measured and recorded in the child task                                                                                                                                |
| [[0382]] | Overlaps [[0431]], which is building a differential oracle against `stellar-xdr`                                                                                                                                                                                                                                                                 |
| [[0454]] | **Done 2026-08-06** — filter keyed on the structured field `alarm = "ch_write_failure"`; `infra/src/lib/stacks/declared-vs-emitted.spec.ts` asserts every filter contract exists in `crates/` (comment-stripped corpus), wired into CI with Rust-aware cache inputs                                                                              |

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

### 5. The release tag declares its own scope ([[0390]] follow-on) — **in code 2026-08-18**

Same defect class as the rest of this task, on the deploy plane: the CDK app
declares **ten** stacks, a release tag deployed **one**, and nothing at release
time compared the two. The gap was invisible by construction — `cdk diff` ran
only over the stack being deployed, so a delta parked in CloudWatch or
Ingestion produced no output anywhere. This task's own alarm set spent a full
release cycle in exactly that state.

Two changes, deliberately separate:

- **The diff got wider than the deploy.** `cdk diff --strict` now covers every
  stack on every run. It is the release's drift record: what a tag leaves
  behind is printed in front of whoever cut it. `--strict` because without it
  `cdk diff` hides non-ASCII entries (measured under [[0312]]).
- **The tag got a selector.** `production-<date>-<N>[-SELECTOR]`, where the
  selector is empty (Compute + SPA, unchanged), `all`, `web`, or a stack name
  pasted onto `Explorer-production-` verbatim. Mapping in
  `infra/scripts/deploy-scope.sh`.

**`--fail` on the diff was considered and rejected.** "Deploy whatever
differs" is `--all` with extra steps, and CDK diffs against _deployed state_,
not against the last tag — so it fires on another task's parked delta or on a
console edit, and CI would ship it unreviewed. That is precisely the stowaway
[[0312]] hit and the reason `infra/Makefile` grew a typed `yes` on the same
day. Widening a release stays a human act; `-all` is how it is spelled.

The tag grammar is now **anchored**: the trigger is `production-*`, so before
this any tag matching that glob deployed the release set — `production-test`
included. Malformed tags are rejected in the plan step, before the build.

Guard: `infra/scripts/deploy-scope.sh --self-check` pins the mapping table and
runs in CI. Its first version was wrong in a way worth recording — the script
emitted TAB-separated fields for the workflow to re-split, and TAB is IFS
whitespace, so `read` collapsed the empty ones and the `-web` selector came
back with the stack set to `true`. The self-check passed anyway, because it
tested the script's output rather than what CI consumed. Fixed by deleting the
seam: the script emits the final `key=value` lines and the workflow appends
them verbatim.

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
- [ ] The CDK half of that comparison exists at release time, not only on a
      schedule: `cdk diff` covers all ten declared stacks on every tag run, and
      a tag can deploy any of them (in code 2026-08-18,
      `infra/scripts/deploy-scope.sh` + selector grammar; AC checks off when a
      real `-<StackName>` tag has deployed a non-Compute stack and the diff of
      an undeployed one was read in the job log)
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
      cost-anomaly alert. First slice in PR #422 (two dead widgets removed,
      freshness widget on the alarm's signal, backlog-age widget with
      threshold line); still open for the second PR: worker-errors +
      CH-write-failures widgets, the cost graph + stated answer, and the
      per-decision leftovers (Galexie disk %, cost reading note, ledger
      RATE series)
- [ ] Each child task either closed by this work or explicitly re-scoped —
      triage in [S — child triage](notes/S-child-task-triage.md) (0406, 0312,
      0428, 0403, 0400 closed and archived; 0454 and 0449 wait on the
      deploy-gated verifications; re-scope of the five non-instances is with
      the operator)
- [x] **Docs updated** — `docs/runbooks/**` gains "how do I tell if it is broken",
      naming the signals and where they live (2026-08-11:
      `docs/runbooks/health.md` — the four-sentence convention, the coverage
      matrix with every cell decided, symptom→first-move paths, an escape
      hatch, and the feedback rule; plus `api-5xx.md`, `dlq.md`, `costs.md`
      shipped earlier)
- [ ] **API types regenerated** — N/A, no API surface change

## Carried to the follow-up PR (raised in PR #422 review, decided not to widen)

1. **`alarm="ch_write_failure"` fires on non-CH failures.** `reconcile()` can
   fail three ways (`HandlerError::S3Download`, `::Parse`, `::ClickHouse`) and
   all three log the same alarm field, so an S3 outage or a parser bug pages as
   "CH write failure". Coverage is total (nothing is missed) and the full error
   Display is in the log line, so it misleads for the first minute rather than
   blinding — but the name is a declared-vs-actual defect of exactly this
   task's class. Options: (a) one generic `reconcile_failed` contract for every
   terminal failure — honest, but renames a deployed metric, its filter, the
   alarm, the guard test and three docs; (b) keep the field, add a `cause`
   field and fix the alarm description — small diff, alarm name still lies;
   (c) document and accept. Operator decision pending.
2. **The NFT fetcher refuses redirects; measured 2026-08-18 whether it should.**
   `Policy::limited(0)` is the SSRF guard, and the gateway pool was chosen in
   0311 precisely because it answers `200` without redirecting. Re-measured:
   for a **file** CID (the normal metadata shape) both `ipfs.io` and
   `gateway.pinata.cloud` still answer `200` in one hop, so the pairing holds.
   But a **directory** CID without a trailing slash answers `301` to the same
   host + `/`, and refusing that loses the content; `dweb.link` (not in our
   pool) redirects to a per-CID subdomain, which is the shape the guard exists
   to stop; `cloudflare-ipfs.com` is dead, confirming the comment. Proposal:
   mirror the SEP-1 policy — bounded hops, https-only, same registrable domain
   as the gateway base — which recovers the trailing-slash case and still
   blocks the subdomain/off-host shapes. Note the classification fix shipped in
   this PR stays correct either way: once the policy follows safe redirects, an
   `is_redirect()` error can only mean "refused as unsafe or over budget",
   which is permanent by construction.
3. **NFT metadata coverage is ~82%, and nothing measures it.** Measured
   2026-08-18 on production, joining `nfts FINAL` to the deduped
   `nft_enrichment` side table (`argMax(_, version)`): of 13 294 promoted
   NFTs, **10 897 carry a name and 10 901 a media URL — about 2 390 have
   neither**. (Method note for whoever picks this up: the columns on `nfts`
   itself are vestigial NULL by design — the live indexer rewrites that row on
   every ownership change — so a count over `nfts.name` reads 0 and means
   nothing. This note first recorded exactly that false 0%.) The residual may
   be entirely legitimate (contracts with no `token_uri`), but no signal
   distinguishes "nothing to fetch" from "fetch is failing", which is defect 2
   applied to an enrichment family. Wants its own task: first establish the
   split, then decide whether a coverage signal is worth an alarm.

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
