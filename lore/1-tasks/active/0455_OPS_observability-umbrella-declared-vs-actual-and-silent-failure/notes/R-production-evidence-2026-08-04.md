---
prefix: R
title: Production evidence for the two defects — measured from CloudWatch, CloudTrail and SQS
status: mature
---

# R — Production evidence (measured 2026-08-04)

All figures read from the production account (`eu-central-1`) with a read-only
principal. Nothing was changed. Every number below is a direct API result, not
an estimate.

## 1. The dead alarm filter, proved from the metric itself

`indexer-ch-write-failures` filters on two exact log strings. One of them,
`failed to process S3 record`, has zero occurrences in `crates/` — it was
removed by the doorbell rewrite (`bee784df`). The string the code emits today on
a terminal reconcile failure is `reconcile failed — will redeliver doorbell`
(`crates/indexer/src/handler/mod.rs:171`), which the filter does not match. The
surviving string, `failed to build mTLS CH client`, is cold-start only
(`crates/indexer/src/main.rs:63`).

The metric confirms this without any code reading:

| Window                                                             | `ChWriteFailures` Sum | Log events evaluated |
| ------------------------------------------------------------------ | --------------------- | -------------------- |
| Each day, 2026-07-28 → 08-04                                       | 0.0                   | ~108 000 / day       |
| Every 5-min bucket of the 0454 outage (2026-07-29 07:00–09:00 UTC) | 0.0                   | —                    |

The filter is live and evaluating ~108 000 events a day. It has never matched.
During the outage it was built to catch, it recorded nothing.

## 2. Lag events in 30 days, and what each one actually was

`ApproximateAgeOfOldestMessage` on `production-ledger-ingest`, hourly maximum,
every bucket above 600 s:

| Date       | Peak                | Cause (verified)                                       |
| ---------- | ------------------- | ------------------------------------------------------ |
| 2026-07-06 | 953 s (16 min)      | deliberate — 5 ESM toggles 16:31–16:58                 |
| 2026-07-07 | 1 027 s (17 min)    | **unexplained**                                        |
| 2026-07-09 | 11 576 s (3 h 13 m) | proto-27 incident — [[0367]] / [[0368]]                |
| 2026-07-10 | 26 505 s (7 h 21 m) | deliberate pause 09:50 → resume 13:09, during [[0368]] |
| 2026-07-16 | 15 759 s (4 h 22 m) | deliberate pause 15:48 → resume 19:14                  |
| 2026-07-29 | 1 421 s (24 min)    | [[0454]]                                               |
| 2026-08-01 | 656 s (11 min)      | **unexplained**                                        |

Plus a flat ~1 980 s (33 min) plateau running 14 hours overnight 07-09 → 07-10:
sustained half-hour lag, no signal.

Classification method: `cloudtrail lookup-events` on
`UpdateEventSourceMapping20150331` — the API call behind the documented pause
procedure in `docs/deployment.md:202`. Note the `20150331` version suffix; the
bare name `UpdateEventSourceMapping` returns nothing and silently reads as "no
pauses ever happened".

**Consequence for the design.** Three of the seven events were planned. A naive
threshold alarm on queue age would have paged three times in one month for
maintenance. An alarm that cries wolf monthly is an alarm that gets muted, so
this is not a detail to fix later — it is a precondition for the alarm being
deployable at all.

## 3. Failure is invisible in every metric an alarm can watch

- Indexer Lambda `Errors`: **0 on every single day** of the 30-day window,
  across all seven lag events. The handler returns `Ok` with a batch-item
  failure, so a total stall never touches the error metric.
- `IngestionLagSeconds`: **no datapoints at all** through the 2026-07-10 window.
  It is published after `persist_with_retry(...).await?`
  (`handler/mod.rs:327-328`), so a failed persist emits nothing.

## 4. Defect 4 — a latched alarm is a mute alarm

CloudWatch notifies on state _transition_. An alarm that enters ALARM and stays
there is silent from the second minute onward, and cannot signal the next
incident because it is already in the target state.

| Alarm                                   | ALARM from       | ALARM until      | Duration             |
| --------------------------------------- | ---------------- | ---------------- | -------------------- |
| `production-ledger-processor-dlq-depth` | 2026-07-09 21:13 | 2026-07-24 16:50 | 15 days              |
| `production-enrichment-dlq-depth`       | 2026-07-03 11:02 | still ALARM      | 32 days and counting |

During the 15-day mute window the 2026-07-16 lag event passed with the DLQ alarm
already red. The enrichment DLQ grew from 4 to 6 messages while latched; neither
new failure produced a notification.

**In 90 days no alarm in this account has changed state**, except that DLQ pair
and a manual `set-alarm-state` test on 2026-07-13 (`stateReason: "test"`, from
the [[0367]] close-out).

## 5. Queue age alone is not sufficient

On 2026-07-10 the DLQ took 524 → 1 137 → 1 755 → 2 372 → … messages over 125
consecutive hours while the main queue drained. Queue age therefore reads
_green_ while ingestion is still dead: failures leaving for the DLQ empty the
queue that the age metric measures. The signal has to be a pair — queue age and
DLQ growth — not either alone.

Retry is already bounded: `maxReceiveCount: 10`, `VisibilityTimeout: 660 s`.
[[0454]] defect 3 ("unbounded retry") is inaccurate as written; the real gap is
that exhaustion lands in a DLQ whose alarm latches.

## 6. Declared vs actual, measured

| Subject                    | Result                                                                                                                                   |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| CDK stacks vs deployed     | **no drift** — 10 `Explorer-production-*` stacks, all `*_COMPLETE`, all present in the CDK app                                           |
| Alarms in code vs deployed | **eight**, not seven; a ninth exists in code behind a flag that is off in production                                                     |
| `treatMissingData`         | exactly one deployed alarm uses `BREACHING` (`galexie-ingestion-lag`)                                                                    |
| Cost allocation tag        | all 11 stacks emit `Project`; Cost Explorer returns a **single untagged group** for July — the tag is not activated in Billing           |
| CloudTrail                 | `describe-trails` is **empty** — no trail exists; only the free 90-day Event history, which cannot be queried beyond 90 days or exported |
| Lambda log retention       | **30 days** — any investigation older than a month is impossible by construction                                                         |

Stack drift itself is unchecked (`DriftInformation: NOT_CHECKED` on every
stack); `detect-stack-drift` is a write call and belongs to the operator.

## 7. Patterns already deployed by the co-tenant project

The same account already runs the alarms this task proposes to invent:

| Alarm                     | Metric                          | Config                                          |
| ------------------------- | ------------------------------- | ----------------------------------------------- |
| `…-ledger-processor-lag`  | `ApproximateAgeOfOldestMessage` | `> 120 s`, period 60 s, 5 periods               |
| `…-*-no-invocations` (×4) | `Invocations`                   | `< 1` over 900 s, `treatMissingData: breaching` |
| `…-sdex-push-freshness`   | custom `PushAgeSeconds`         | freshness measured from data, not metadata      |
| `…-mtls-notafter`         | custom `MinDaysToNotAfter`      | certificate expiry as a metric                  |

Two of these are directly the shapes this task wants: absence-as-breach on
invocations, and freshness measured from the data rather than from a success
callback. The third (`MinDaysToNotAfter`) is worth stealing outright — it is the
declared-vs-actual comparator applied to certificate lifetime.

Caveat: their `no-invocations` alarm would fire during a planned pause too, so
it does not solve §2 — it only shows the pattern.
