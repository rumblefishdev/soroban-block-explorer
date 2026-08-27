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
  - date: 2026-08-19
    status: active
    who: karolkow
    note: >
      Second slice opened as PR #427 and the post-incident review sweep closed
      out: 68 findings, every one dispositioned. 17 fixed, 31 already covered
      by open tasks, 10 rejected once measured, 5 verified healthy, 3 skipped
      on the operator's call, 2 handed off or refuted. Five tasks spawned
      (0507-0511), four existing ones extended. Three acceptance criteria
      updated here: the dashboard/alarm one advanced with the second slice, the
      latch-proofing one now records that production contradicts it knowingly,
      and the child-task one names what was spawned.
  - date: 2026-08-19
    status: active
    who: karolkow
    note: >
      Acceptance criteria triaged. Two closed: the cost one on the deploy, with
      the read-only evidence and the unproven last hop both stated; the API
      types one because a stated N/A is answered, not pending. Of the eight
      left, four need a production window, one is a measurement now runnable,
      one is in flight with PR #427, one is an operator decision, and one — no
      alarm sits latched and mute — is dead as written, because the alarm that
      breaks it was skipped knowingly. Recorded with the two honest ways out.
  - date: 2026-08-27
    status: active
    who: karolkow
    note: >
      Two consistency items closed on the branch. The plan section claimed the
      DLQ-growth alarm pair was "still pending" eight days after ADR 0054
      withdrew it; corrected with the reason. 0403's deferred measurement ran:
      the sep1 issuer resolve averages 25 057 read_rows over 9 370 production
      calls, the expected order. The dev_read vs ingestion_writer discrepancy
      is explained rather than left open — probing the same statement across
      five keys as dev_read spans 24 576 to 73 728 read_rows, so granule
      placement sets the cost and the user does not. Seven criteria left.
  - date: 2026-08-27
    status: active
    who: karolkow
    note: >
      Child-task triage reversed. The note had said five tasks should be
      re-scoped out of this umbrella; nothing acted on it for two weeks and the
      decision is now the opposite — they stay in related_tasks and are judged
      one at a time. Two were never live questions: 0403 closed 2026-08-11 and
      0127 is archived. The real judgement is 0232, 0250 and 0087.
  - date: 2026-08-27
    status: active
    who: karolkow
    note: >
      The latched-alarm criterion narrowed rather than left contradicting
      production: the enrichment DLQ alarm is excluded by operator decision,
      with the reason written into the criterion. It covers every other level
      alarm, and its remaining open half is the re-arm test message. The
      dead-as-written state is resolved.
  - date: 2026-08-27
    status: active
    who: karolkow
    note: >
      The three remaining child-task judgements made individually. 0250 stays
      and its priority is raised — quotas the config declares are not enforced
      on the production auth path. 0087 leaves related_tasks; the umbrella's
      own notes always excluded frontend telemetry. 0232 turned out to be
      superseded in premise: task 0497 already decided the RMT/MIN compromise
      goes rather than gets per-column mitigations — the two are cross-linked
      and 0232's fate is a close-or-narrow decision recorded there.
  - date: 2026-08-27
    status: active
    who: karolkow
    note: >
      Dashboard/alarm coverage closed. The three C7 leftovers shipped: Galexie
      disk percentage built from the alarm's own metric objects, a ledger RATE
      series so a stall reads as a drop rather than a flat line, and a reading
      note stating why the cost-anomaly alert has no widget by design. Six
      criteria left, four of them needing a production window.
  - date: 2026-08-27
    status: active
    who: karolkow
    note: >
      All three carried items closed. The reconcile alarm gained a cause field
      rather than a rename; the NFT fetcher took SEP-1's redirect policy rather
      than a second copy of it; NFT metadata coverage was re-measured (20.7%
      of promoted NFTs carry neither name nor media) and spawned as 0520. The
      measurement changed the shape of that question: the gap is all-or-nothing
      per collection, 51 collections fully missing against 7 partial, so it is
      one cause per collection rather than scattered fetch failures.
  - date: 2026-08-27
    status: active
    who: karolkow
    note: >
      Two of the three dashboard cells closed with a measured "no widget"
      rather than a widget. The Galexie disk chart was reverted after 57 days
      of history showed 26.5-30.5% and never past 40% - a flat line; the 60%
      alarm guards a catchup spike, not that baseline. The ledger RATE series
      was reverted as redundant with ingestion-backlog-age, which catches the
      same stall sooner and pages. The cost note stays, shortened, and records
      that EstimatedCharges has no tag dimension so a per-project split cannot
      exist in CloudWatch. Operator caught both; neither had been measured
      before being written, which is the exact failure this task keeps finding
      in other people's work.
---

# Observability umbrella — recurring defects, not isolated bugs

## Summary

Eight open tasks turned out to be instances of a small number of recurring
defects. (The original framing said sixteen; a triage found eight genuine
instances, three that form a separate cost cluster, and five that do not
belong — the inflated headline is corrected here rather than left standing at
the top while the Notes refute it further down.)

The defects, each with several instances:

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
  per planned pause is the accepted cost. **The DLQ-growth pair is not
  pending — it was withdrawn 2026-08-11** (ADR 0054, "Considered and
  withdrawn"): the DLQ contents were investigated and the latch turned out to
  be a missing drain procedure, not a wrong alarm shape, so a plain level
  alarm under rule 2's carve-out is correct and `docs/runbooks/dlq.md` carries
  the drain. This line said "still pending" for eight days after the ADR
  killed it
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

### 3. The comparator — the schedule withdrawn, the requirement kept

A single scheduled job printing deltas for every row of defect 1 was the first
answer, and it is **not being built**. ADR 0054 records why: the infrastructure
comparison already runs (`make diff-production`); its output was read on
2026-06-22, measured again on 2026-07-27, and the delta was still pending on
2026-08-04. The gap was never detection. Putting the same report on a timer
produces it more often, to the same effect.

What survives is the requirement inside the idea: **a human sees the delta
without asking for it.** That is now bound to the release instead of to a
clock — `cdk diff` covers all ten declared stacks on every tag run
(`infra/scripts/deploy-scope.sh`), so the delta arrives at the moment a change
ships, which is also the moment someone can act on it. Same principle as
ADR 0054 rule 5: verify on change, not on a schedule.

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
- [ ] The planned-pause constraint has a stated answer — resolved 2026-08-10
      the opposite way from the first draft: pauses are NOT machine-readable;
      one knowing page per pause is the accepted design (ADR 0054 rule 4).
      **Un-ticked 2026-08-19**: the box was checked against a verification
      that was never run. Worse, the deploy window it referred to is the one
      in which no alarm could deliver at all (see the mute incident below),
      so "exactly one page" was unobservable by construction. Re-ticks after
      pausing the event-source mapping and counting the pages that arrive
- [ ] An alarm fires on ingestion stall — in code
      (`production-ingestion-backlog-age`); AC checks off after a simulated
      stall against the deployed alarm
- [x] Every alarm's `treatMissingData` reviewed and justified in a comment
      (2026-08-06)
- [ ] Every level-triggered alarm has a stated re-arm answer; no alarm can sit
      latched and mute — **narrowed 2026-08-27, operator decision: the
      enrichment DLQ is excluded from this criterion.**
      `production-enrichment-dlq-depth` has been in ALARM since 2026-07-03
      with a standing tail of messages, is mute by construction (CloudWatch
      pages on transition), and the operator chose to accept that rather than
      drain on a schedule. The exclusion is the honest form: the alarm still
      exists and still marks the queue as non-empty on the dashboard, but this
      criterion no longer claims it can page. The rule the ADR states —
      standing content never accepted — holds for every OTHER level alarm.
      What remains to tick for those: the drain half is proven (purging the
      ledger DLQ moved its alarm ALARM -> OK after five days latched,
      2026-08-19); the re-arm half — that a re-dirtied queue pages again —
      still needs the test-message gate (an operator-window item).
      **Gate run 2026-08-27, and it failed at a stage nobody was testing.**
      The synthetic message moved the alarm OK -> ALARM in three minutes, so
      detection re-arms exactly as designed — but the page never left AWS:
      `CloudWatch Alarms is not authorized to perform: SNS:Publish`. The
      2026-08-19 policy repair had not worked, and eighteen alarm actions
      across nine days delivered nowhere. Detection half: PROVEN. Delivery
      half: BLOCKED on the policy fix. Re-running this gate after that deploy
      is what ticks the box — see "Discovery 2026-08-27"
- [ ] The declared-vs-actual delta reaches a human without anyone asking for it.
      **Re-scoped 2026-08-19** from "a comparator runs on a schedule": the
      schedule is withdrawn (ADR 0054, and the measured six weeks of a read-but-
      unacted delta), the requirement is not. The mechanism is the tag run's
      `cdk diff` over all ten stacks — so this criterion now ticks with the one
      below, which gates that same run
- [ ] The CDK half of that comparison exists at release time, not only on a
      schedule: `cdk diff` covers all ten declared stacks on every tag run, and
      a tag can deploy any of them (in code 2026-08-18,
      `infra/scripts/deploy-scope.sh` + selector grammar; AC checks off when a
      real `-<StackName>` tag has deployed a non-Compute stack and the diff of
      an undeployed one was read in the job log)
- [x] Alarm filter strings verified against the strings the code actually emits
      (2026-08-06 — `declared-vs-emitted.spec.ts`, enforced in CI)
- [x] 0403's deferred measurement executed after the next deploy + a drain:
      the sep1 issuer resolve reads ~24.6k rows/call in `system.query_log`
      (not ~24.9M), and the `dev_read` vs `ingestion_writer` read-count
      discrepancy explained or recorded as still open.
      **Measured 2026-08-27.** Over 14 days the query
      (`nullIf(account_id, ?) AS issuer_strkey … FROM accounts WHERE id = ?`)
      ran **9 370 times as `ingestion_writer`, averaging 25 057 read_rows**,
      max 109 822 — the expected order, three orders of magnitude below the
      ~24.9M that prompted the check.
      **The two-user discrepancy is explained, and it is not the user.**
      Probing the identical statement as `dev_read` across five different keys
      returned 24 576 / 40 960 / 65 536 / 73 728 / 73 728 read_rows — a 3×
      spread inside one account, straddling `ingestion_writer`'s average. The
      cost is set by how many granules the probed key touches, not by who
      asks; sample one key and any two users look different. So read estimates
      taken from the readonly account **do** describe production, provided
      more than one key is sampled — which 0397 did not do
- [x] Cost allocation tags active; a per-project cost answer takes minutes, and a
      step change in spend raises an alert (tags active + backfill, runbook
      shipped; anomaly detection committed — checks off after deploy).
      **Ticked 2026-08-19 on the deploy**, and the boundary is stated rather
      than glossed: read-only checks confirm the monitor exists
      (`production-cost-anomaly-by-service`, DIMENSIONAL/SERVICE, created
      2026-08-18) and that an IMMEDIATE subscription routes it to the alarm
      topic. What is NOT proven is the last hop to the channel — that is true
      of every alarm here and is what ADR 0054 rule 5 gates from now on.
      **2026-08-27: that unproven hop was broken the whole time.** The caveat
      was written as prudence about a step nobody had checked; it turned out
      to be an accurate description of a live defect. The box stays ticked for
      the monitor and the subscription, which do exist and are correct, and
      the delivery failure is carried by "Discovery 2026-08-27" rather than by
      re-opening this criterion
- [x] Dashboard↔alarm coverage reconciled (7 widgets without alarms, 2 alarms
      without widgets) — including a stated dashboard answer for the new
      cost-anomaly alert. First slice in PR #422 (two dead widgets removed,
      freshness widget on the alarm's signal, backlog-age widget with
      threshold line). **Second slice in PR #427**: worker-errors and
      CH-write-failures widgets and the cost section are in. Still open: the
      stated dashboard answer for the cost-anomaly alert, and the
      per-decision leftovers. **2026-08-27: one closed, two answered with a
      measured "no widget".** The cost reading note landed — it states why the
      anomaly alert deliberately has no widget (it already pages per-service
      on a step change; the graph beside it is for the creep that never looks
      like a step), and records that a per-project split is impossible here:
      `EstimatedCharges` carries no tag dimension, measured — that split is
      Cost Explorer's job.
      The Galexie disk and ledger-rate widgets were **built, measured and
      reverted the same day**. Disk sat between 26.5% and 30.5% across 57 days
      and never passed 40%, so the chart was a flat line; its 60% alarm guards
      a catchup spike, not the steady BucketList underneath. The ledger RATE
      series duplicated `ingestion-backlog-age`, which catches the same stall
      sooner, with a threshold line, and pages. Both cells are now "no widget,
      and here is the measurement", which is what the matrix asks for.
      Still deliberately NOT taken: sharing metric constants for the other
      four alarm/widget pairs (review finding 39) — skipped on the operator's
      call despite favourable arithmetic, so those four stay assertions
- [ ] Each child task either closed by this work or explicitly re-scoped —
      triage in [S — child triage](notes/S-child-task-triage.md) (0406, 0312,
      0428, 0403, 0400 closed and archived; 0454 and 0449 wait on the
      deploy-gated verifications; re-scope of the five non-instances **reversed
      2026-08-27** — they stay in `related_tasks` and are judged one at a time
      rather than dropped as a group; 0403 and 0127 are closed anyway, so the
      live question was 0232, 0250 and 0087 — judged 2026-08-27: 0250 stays
      with raised priority, 0087 is out (dropped from related_tasks), 0232 is
      superseded in premise by 0497 and cross-linked there). **2026-08-19**: 0449 moved backlog -> active, because its
      detection half has been live since the 2026-08-18 release while the file
      still read "not started". Five further tasks spawned from the review
      sweep below — 0507 (schema migration ladder), 0508 (crate boundaries),
      0509 (RPC pools and declared egress), 0510 (auth path missing from the
      API schema), 0511 (infrastructure configuration is not one thing) — and
      four existing tasks absorbed findings rather than spawn near-duplicates
      (0103, 0414, 0418, 0458)
- [x] **Docs updated** — `docs/runbooks/**` gains "how do I tell if it is broken",
      naming the signals and where they live (2026-08-11:
      `docs/runbooks/health.md` — the four-sentence convention, the coverage
      matrix with every cell decided, symptom→first-move paths, an escape
      hatch, and the feedback rule; plus `api-5xx.md`, `dlq.md`, `costs.md`
      shipped earlier)
- [x] **API types regenerated** — N/A, no API surface change (an N/A with a
      reason is answered, not pending)

### Where the remaining eight stand (triaged 2026-08-19)

Triaged once so nobody re-derives it. Three shapes: an **operator window**
(needs a deliberate action in production and cannot be done from a keyboard
here), a **measurement** (runnable now), or **dead as written**.

| Criterion                                  | Shape               | What unblocks it                                                                                                                           |
| ------------------------------------------ | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Planned pause pages exactly once           | operator window     | pause the event-source mapping, count the pages that arrive                                                                                |
| Alarm fires on ingestion stall             | operator window     | simulate a stall against the deployed alarm                                                                                                |
| CDK half of the comparison at release time | operator window     | one real `-<StackName>` tag deploying a non-Compute stack, with the diff of an undeployed one read in the job log                          |
| 0403's deferred measurement                | **done 2026-08-27** | 25 057 read_rows/call over 9 370 calls; the two-user gap is granule placement, not permissions                                             |
| Dashboard↔alarm coverage                   | **done 2026-08-27** | cost note landed; disk and ledger-rate widgets measured and rejected, each cell now carries its measurement                                |
| Each child task closed or re-scoped        | operator decision   | judge 0232, 0250 and 0087 individually — the group re-scope was reversed 2026-08-27                                                        |
| Declared-vs-actual delta reaches a human   | gated               | ticks with the release-time criterion above; not independently actionable                                                                  |
| No alarm sits latched and mute             | narrowed 2026-08-27 | the enrichment DLQ alarm is excluded by operator decision; the criterion now covers the rest, and its open half is the re-arm test message |

**The latched-alarm criterion cannot be ticked, and that is a decision, not a
backlog item.** `production-enrichment-dlq-depth` has been in ALARM since
2026-07-03 and was skipped deliberately on 2026-08-19. The rule it states —
standing content is never accepted — is being knowingly broken. Two honest
ways out, both the operator's: drain that queue and let the criterion mean what
it says, or narrow the criterion to exclude it with the reason written down.
Leaving it open and unexplained is the one option that makes the task lie.

**Four of the remaining seven need a production window, not code.** That is the
real state of this umbrella: the building is done, the proving is not, and the
proving is deliberately not something that can be faked from here — which is
the whole point of a task named "declared vs actual, never compared".

## Review sweep 2026-08-19 — 68 findings, and what survived measuring them

After the mute incident, six review passes were run over the merged first
slice plus a whole-repo architecture audit, and their output was tracked to a
disposition each. The register itself is deliberately not in git (it names
files and shapes for one finding that must not be described in a public repo);
what belongs here is the outcome and the lesson.

**Where the 68 went**

| Disposition                                    | Count |
| ---------------------------------------------- | ----- |
| Fixed                                          | 17    |
| Already covered by an open task                | 31    |
| Examined and rejected on measurement           | 10    |
| Verified healthy, recorded so nobody re-audits | 5     |
| Skipped on the operator's call                 | 3     |
| Handed off / refuted outright                  | 2     |

**What shipped from it.** The alarm-mute fix (the only finding that was
actually breaking production, deployed 2026-08-19 13:59 UTC and verified
live); ADR 0054 rule 5; the comparator re-scope; threshold sources named; the
trash policy's worktree hole closed; the task-status correction on 0449;
ASCII-only synthesized strings; four alarms stripped of a re-derived ADR rule;
the T4 decision recorded inside the guard.

**The lesson, because it cost most of the session.** Every finding that named
a file and a line held up. Every _aggregate characterisation_ did not:

- "the alarm blocks are ~600 lines of copy-paste" — all ten together are 226,
  and roughly 30 are recoverable;
- "~30 findings on other people's files" — the range carries nine files from
  other tasks, all of them this operator's;
- "four near-identical justification essays" — one argument repeated four
  times, 11 lines net;
- "a magic count guard instead of an exact filter" — the exact filter is
  already there, and the count is the guard **on** it;
- "the Makefile reinvents an approval flag" — the flag guards
  security-broadening changes, the confirm guards a parked delta, and both are
  in use.

Ten findings died that way. The pattern is pattern-recognition without
arithmetic: the direction was right every time, the magnitude was never
computed. **Verify the summary at least as hard as the finding.** The same
discipline is why the alarm factory, the error-rate helper and the shared
metric constants were all rejected or reverted rather than shipped on the
strength of three reviewers agreeing.

A second, narrower lesson: `npx --prefix <dir> nx …` does not change the
working directory, so several verification runs this session measured the main
checkout while the work sat in a worktree. It reported four passing guard
tests where the worktree has five, and it reported green while a deliberately
broken emit site sat on disk. Verification commands must be run from the tree
they claim to verify.

## Incident 2026-08-18 — this task's own deploy muted every alarm for 9 days

> **Title corrected 2026-08-27.** This section said "19 h" and every sentence
> below was written in the past tense. Both were wrong: the mute never ended.
> The 2026-08-19 repair restored a policy that CloudWatch still cannot publish
> under, and the section's own closing "Fix" line describes that repair as if
> it had worked. It did not. The measured end of the mute is not yet in the
> past — see the section that follows. The original text is kept unedited
> because being able to read a confident, wrong write-up is the point.

The defining event of this task, and it is ours. Recorded here rather than in
a child task because it invalidates two acceptance criteria and rewrites one
open decision from optional to blocking.

**What happened.** Adding a cost-anomaly grant to the alarm topic
synthesised an `AWS::SNS::TopicPolicy`, and that resource REPLACES a topic's
access policy rather than extending it. The replaced policy was the default
SNS attaches at topic creation — the statement CloudWatch publishes under.
From the deploy at 14:52 until 09:58 the next morning, every alarm evaluated
correctly, changed state correctly, and could not deliver.

**Why nothing caught it.** Three reasons, each worth keeping:

1. The deploy was green. Nothing failed — the resource was created exactly as
   written. Losing the implied statement is obedient behaviour, not an error.
2. A refused publish is nobody's alarm. It surfaces only in an alarm's action
   history, which no one reads unless already suspicious.
3. The post-deploy verification checked alarm STATES and metric flow, both
   healthy. It did not check action EXECUTION — and could not, because for
   26 hours no alarm attempted a delivery. The first attempt was the DLQ
   returning to OK after a purge, which is how this was found: by accident,
   during unrelated work.

**Measurement that settled it**: three `Failed to execute action` entries on
our topic in the window, against twelve `Successfully executed` on the
co-tenant's topic in the same window — a topic-scoped cause, not a service
fault.

**What it changes here.**

- The Slack-chain witness stops being an open decision and becomes a
  precondition. ADR 0054 named this exact failure under Negative consequences
  and listed two cheap mitigations; the work identified the hole, wrote it
  down, and walked into it. An ADR that governs alarms but says nothing about
  the channel they travel is incomplete — the channel needs a rule.
- Verification gates must run immediately after a deploy, not "later". The
  drained-DLQ test-message gate existed precisely for this and would have
  exposed it in a minute; it was deferred and the defect lived 19 hours.
- Two acceptance criteria were ticked against proofs that never ran, one of
  them referring to the very window in which no page could arrive. Both are
  un-ticked above.

**Fix**: `ops/0455_sns-topic-policy-mute-fix` — the policy now lists every
principal explicitly (owner account, cost anomalies with a source-account
condition), with a comment recording the replacement semantics so the next
grant is added to the list rather than on top of it.

## Discovery 2026-08-27 — the repair did not repair; 18 actions, 0 delivered

The re-arm gate was finally run: a synthetic message was sent to the ledger
DLQ to prove a re-dirtied queue pages again. The alarm crossed in three
minutes. Nothing arrived. The alarm's own action history answers why, in
CloudWatch's words:

```
error: "CloudWatch Alarms is not authorized to perform: SNS:Publish
        on resource:...:production-soroban-explorer-alarms"
```

**The mute never lifted.** Full action history on the topic, 2026-08-14 to
2026-08-27:

```
14.08 03:28  OK    ledger-processor-dlq-depth      <- last delivery, ever
18.08 14:52  FAIL  api-gateway-5xx                 <- deploy
18.08 14:52  FAIL  ingestion-backlog-age
19.08 09:58  FAIL  ledger-processor-dlq-depth
21.08 16:47  FAIL  indexer-ch-write-failures
21.08 16:52  FAIL  indexer-ch-write-failures
21.08 16:52  FAIL  ingestion-backlog-age
21.08 16:59  FAIL  ingestion-backlog-age
26.08 09:00  FAIL  api-gateway-5xx
26.08 09:15  FAIL  api-gateway-5xx
27.08 02:40  FAIL  galexie-ingestion-lag
27.08 03:18  FAIL  galexie-ingestion-lag
27.08 03:22  FAIL  ingestion-backlog-age
27.08 03:31  FAIL  galexie-ingestion-lag
27.08 03:37  FAIL  ingestion-backlog-age
27.08 03:40  FAIL  galexie-ingestion-lag
27.08 03:49  FAIL  ingestion-backlog-age
27.08 03:57  FAIL  ingestion-backlog-age
27.08 12:13  FAIL  ledger-processor-dlq-depth      <- the synthetic test
```

Eighteen attempts, zero deliveries, nine days. SNS confirms it from the other
side: `NumberOfMessagesPublished` has exactly one non-zero hour since 13.08,
and `NumberOfNotificationsFailed` is empty throughout — the publish never
happened, so there was nothing to fail to deliver.

### Root cause of the failed repair

The repair restored `AllowOwnerAccountPublish` with an account-root principal
and dropped the `cloudwatch.amazonaws.com` grant as cross-account-only. The
code comment stated that reasoning as fact:

> "Same-account alarms publish AS THE ACCOUNT, so the owner statement is the
> one that matters; a `cloudwatch.amazonaws.com` service-principal grant is
> only needed for cross-account topics and was dropped as redundant here."

It is wrong. An account-root principal admits IAM identities in the account;
it does not admit the CloudWatch Alarms service. The default policy that the
first deploy destroyed admitted it via `"AWS": "*"` + `AWS:SourceOwner`, not
via root — and that distinction is the whole defect. AWS's own setup page
repeats the same claim the comment made, so consulting it a second time would
have confirmed the error rather than caught it. The arbiter was the chain.

**Fix**: a third statement, `AllowCloudWatchAlarmsPublish`, service principal
`cloudwatch.amazonaws.com`, scoped by `AWS:SourceAccount` like the cost grant.
The owner statement stays (hand-publishing needs it). Verified in the
synthesized template, not only in source.

### Three real incidents nobody was told about

Not hypothetical exposure — these happened during the mute.

**A. 2026-08-21 14:46 UTC, ClickHouse unreachable.** Two `ch_write_failure`
lines four seconds apart, `network error: client error (Connect)`. TCP connect
to the database host failed. Ingestion backlog reached 263 s and drained
within seven minutes on SQS redelivery.

**B. 2026-08-26 06:58:27 UTC, a 500 from the decompiled-contract route.**

> **Corrected within the day.** This entry first read "06:55 UTC, a 5xx from
> the API edge... the request never reached our code... it cannot be
> diagnosed". All three claims were wrong, and they were wrong for one
> reason: the alarm's datapoint label `06:55:00` was read as the minute the
> request happened. The alarm's `Period` is 300, so that label is the START
> of the 06:55-07:00 bucket. Searching the Lambda log around the wrong minute
> found nothing, and "no invocation" was then promoted to "never reached our
> code". A CloudWatch datapoint timestamp names a bucket, not an event.

The single fault in the window is one request:

```
GET /v1/contracts/{contract_id}/decompiled
HTTP 500, 10.244 s
```

It carries the same X-Ray trace id as the `decompilation timed out` WARN the
API logged at 06:58:38, so the warning and the 500 are one request, not two
neighbouring events. The route computed for ten seconds and then failed.

**It is diagnosable, and was.** The stage has `TracingEnabled: true`, and
X-Ray held the URL, status, duration and trace id without any extra
instrumentation. What the stage does NOT have is `accessLogSettings` (null)
and per-method metrics (`metricsEnabled: false`) — worth having for
volume-shaped questions, but not the blocker this entry originally claimed.

Frequency, sampled across four days of X-Ray fault queries in the window:
2026-08-20 none, 08-22 none, 08-24 none, 08-26 one. Rare, and tied to one
contract. The decompiled tab shipped under task 0465; this is a timeout in
that route, not an outage.

**Spawned as 0522** (operator's call, over the "known one-off" note this entry
first proposed). Two separable defects: the timeout, and a bare 500 shown to a
reader for an expected outcome on an optional view.

**C. 2026-08-27 01:10 UTC, the node fell out of consensus.** `Herder: Lost
track of consensus`, then `Herder: Ledger took 282.692064376 seconds` against
a normal ~5 s close. The node entered catchup and six external history
archives refused in sequence — corrupt archive metadata, missing HAS entries,
failed checkpoint downloads. Two alarm/OK cycles on the Galexie lag alarm,
backlog to 294 s, self-recovered by 01:55. The archives are third-party
infrastructure: not our fault, but our exposure, and worth checking whether
the node's archive list still carries dead entries.

### Decisions taken on these three, 2026-08-27

**A (ClickHouse connect) — left as a blip, deliberately.** Two failures four
seconds apart, self-healed, no gap. The host itself is outside monitoring
(task 0237), which is where a recurrence would be diagnosed, not here.

**B (the 500) — spawned as 0522** rather than filed as a known one-off. The
timeout and the bare 500 a reader is shown are separable defects, and the
second does not depend on how rare the first is.

**C (consensus loss, six archives refused) — no configuration change.** The
first instinct was "review our history-archive list and drop the dead
entries". There is no such list of ours: `ingestion-stack.ts` sets
`network = "pubnet"`, a preset, and the archives come from the captive-core
defaults inside the Galexie image. Overriding them means supplying a full
`captive_core_toml_path`, which per Galexie's own documentation **completely
replaces** the defaults — the quorum set included. Taking ownership of the
quorum set to drop one dead archive is the wrong trade for an event that
self-healed in 45 minutes with zero gaps, and where six third-party archives
failed at once. Recorded so a recurrence has a starting point instead of a
fresh instinct.

**Per-method API metrics turned on** (`metricsEnabled: true`,
`api-gateway-stack.ts`). Access logging stays off: its stated trigger — "add
it only when a silent-504 investigation actually needs it" — did not fire,
because X-Ray held the answer. But the reason X-Ray held it was never written
down and nothing watches it: X-Ray samples the first request per second and
~5% of the rest, and measured peak traffic on this stage is **0.8 req/s**
(~2k requests a day, 2026-08-25/26). At today's volume nearly everything is
traced, so "X-Ray covers it" is true by accident of traffic. A modest increase
crosses the reservoir and single errors start disappearing, with no signal
that the property has lapsed. Per-method metrics do not sample, and they also
answer the question X-Ray cannot: how often a given route fails over a week.
This ships in the ApiGateway stack, not CloudWatch — a separate deploy.

### No data was lost

```
sequence 64 000 000 -> 64 146 961   have = 146 962   missing = 0
```

Zero gaps across the whole mute window, including incident A's own minutes.
All three incidents self-healed. The failure here is purely that the system
recovered in silence — which is this task's thesis, demonstrated on itself
twice.

### A second trap found while fixing the first

`infra/cdk.json` declares `"app": "node dist/bin/production.js"`. The first
`cdk synth` after editing `cloudwatch-stack.ts` emitted a template WITHOUT the
change, because it read a `dist/` build produced before the edit — and exited 0.

**Scope corrected before this was acted on.** The first write-up said "synth
and deploy both describe stale code as current" and proposed adding a build
guard to the Makefile. `infra/Makefile` already has one: every
`deploy-production-*` target declares `build` as a prerequisite, which runs
`nx build`. The documented deploy path was never exposed. What bit here was a
raw `npx cdk synth` — a shortcut around the Makefile — so the lesson belongs
to the shortcut, and the guard that was about to be proposed already ships.
Recorded because the sequence (read the code, infer a defect, propose a fix,
find the fix already there) recurred through this task's review sweep and is
worth being able to recognise from the inside.

### What this changes

- ADR 0054 rule 5 ("the delivery path is verified on every change to it,
  before the change is called done") was written from the first incident and
  caught the second on its first real run. It stays; the evidence for it is
  now measured rather than argued.
- The cost-tag criterion's caveat — "what is NOT proven is the last hop to the
  channel" — was not a theoretical gap. It was the defect, unproven and
  present, for nine days.
- No repair of this topic may be called done on inspection again. The gate is
  a delivered page, and nothing weaker.

## Carried to the follow-up PR (raised in PR #422 review, decided not to widen)

1. **`alarm="ch_write_failure"` fires on non-CH failures.** `reconcile()` can
   fail three ways (`HandlerError::S3Download`, `::Parse`, `::ClickHouse`) and
   all three log the same alarm field, so an S3 outage or a parser bug pages as
   "CH write failure". Coverage is total (nothing is missed) and the full error
   Display is in the log line, so it misleads for the first minute rather than
   blinding — but the name is a declared-vs-actual defect of exactly this
   task's class. **Resolved 2026-08-27 — option (b).** Every emit site now carries
   `cause=s3|parse|clickhouse` (`mtls_init` at the cold-start site) and the
   alarm description tells the reader to check it before assuming the
   database. Renaming the metric was rejected: it would cost the deployed
   series its history for a name.
2. **The NFT fetcher refuses redirects; measured 2026-08-18 whether it should.**
   `Policy::limited(0)` is the SSRF guard, and the gateway pool was chosen in
   0311 precisely because it answers `200` without redirecting. Re-measured:
   for a **file** CID (the normal metadata shape) both `ipfs.io` and
   `gateway.pinata.cloud` still answer `200` in one hop, so the pairing holds.
   But a **directory** CID without a trailing slash answers `301` to the same
   host + `/`, and refusing that loses the content; `dweb.link` (not in our
   pool) redirects to a per-CID subdomain, which is the shape the guard exists
   to stop; `cloudflare-ipfs.com` is dead, confirming the comment. **Resolved 2026-08-27 — implemented, and by sharing rather than
   copying:** the fetcher now uses SEP-1's own `same_etld1_redirect_policy`,
   so there is one SSRF redirect policy for every outbound enrichment fetch
   instead of two that can drift. Recovers the trailing-slash case, still
   blocks the subdomain and off-host shapes. Note the classification fix shipped in
   this PR stays correct either way: once the policy follows safe redirects, an
   `is_redirect()` error can only mean "refused as unsafe or over budget",
   which is permanent by construction.
3. **NFT metadata coverage — spawned as [[0520]] 2026-08-27.** Re-measured
   before filing: **13 752 promoted NFTs, 2 849 (20.7%) carry neither a name
   nor a media URL.** The shape is the finding — the gap is all-or-nothing per
   collection: 51 collections are 100% missing (2 716 tokens) and only 7 are
   partial (133 tokens), which points at one cause per collection rather than
   scattered fetch failures. The 7 partial ones are where a real defect would
   hide. Method note kept in 0520 because it already produced a false 0% once:
   the `name`/`media_url` columns on `nfts` are vestigial NULL by design, so
   the count must join `nft_enrichment` with `argMax(_, version)`. Note that
   the redirect-policy change landed the same day, so that figure is the
   pre-change baseline and 0520 re-measures after deploy.

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
