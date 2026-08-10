---
id: '0054'
title: 'One alarm engine for AWS signals, and three rules an alarm must satisfy'
status: proposed
deciders: [karolkow]
related_tasks: ['0455', '0454', '0428', '0237']
related_adrs: []
tags: [observability, ops, infrastructure, policy]
links:
  - infra/src/lib/stacks/cloudwatch-stack.ts
history:
  - date: 2026-08-04
    status: proposed
    who: karolkow
    note: >
      Written from the lore-0455 umbrella, then deliberately narrowed after a
      devil's-advocate pass. The first draft also ruled on where every kind of
      check should live and mandated that the database host publish metrics.
      Neither was supported by the evidence gathered, so both were withdrawn —
      see "Considered and withdrawn". What remains is only what production
      measurement actually established.
  - date: 2026-08-10
    status: proposed
    who: karolkow
    note: >
      Recreated from the parked draft. Since it was written, part of what it
      prescribes has landed independently: the write-failure filter now keys on
      a structured field with a CI guard comparing declared filters against the
      strings crates/ emits, and every alarm carries a treatMissingData
      justification comment (rule 3 in practice). The stall and DLQ-growth
      alarms that demonstrate rules 2 and 4 follow as their own commits. Moves
      to accepted once the ingestion-stall alarm is verified against the
      deployed alarm, not the config.
  - date: 2026-08-10
    status: proposed
    who: karolkow
    note: >
      Review round (devil's-advocate + AWS-native cross-check). Rule 2 was
      challenged as possibly meaning repeated reminders — it does not, and now
      says so explicitly; the growth pattern matches AWS's own DLQ guidance
      (NumberOfMessagesSent misses redrive moves, RATE/DIFF on visible depth is
      the documented answer). Two real holes found and addressed: DIFF over an
      idle queue's metric gaps may swallow the first failure after a drain
      (verification gate added), and an accidentally-disabled event source is
      indistinguishable from a planned pause (rule 4 now requires a slow
      backstop). Delivery-chain single-witness recorded; alternatives expanded
      with the natively-offered mechanisms deliberately not taken.
---

# ADR 0054: One alarm engine for AWS signals, and three rules an alarm must satisfy

**Related:**

- [Task 0455: observability umbrella](../1-tasks/active/0455_OPS_observability-umbrella-declared-vs-actual-and-silent-failure/README.md)

---

## Context

Sixteen open tasks describe monitoring gaps, and each proposes not just a check
but its own way of telling a human. Task 0237 alone specifies a Slack webhook
(or msmtp + SES), a cron, a **24-hour cooldown sentinel file**, a logrotate
config and a new environment assertion. Built as each task is written, the set
comes to five schedulers, four delivery paths and three secrets to rotate, for
two people.

The 0237 cooldown sentinel names the problem exactly: a file touched to suppress
repeat notifications for a day is a **hand-rolled alarm state machine, in bash**
— dedup, latching and re-arm, rebuilt from scratch for one signal.

Production measurement on 2026-08-04 established three further facts, and they
are what this ADR actually rests on:

- An alarm that latches goes mute. Two live instances: one DLQ alarm sat in
  ALARM for 15 days and cleared only when its messages aged out of retention;
  another has been ALARM for 32 days while its queue grew.
- Absence reads as health. Indexer Lambda `Errors` was 0 on every day of a
  30-day window that contained seven ingestion lag events.
- Maintenance looks exactly like failure. **Three of those seven lag events were
  declared pauses.** An alarm on backlog alone would have paged three times in
  one month for our own work.

These are properties of the engine and of this system, and they have to be
handled once rather than re-solved by every notifier.

---

## Decision

**1. For anything running in AWS, CloudWatch is the alarm engine and
SNS → Chatbot → Slack is the delivery path.** Components emit metrics; they do
not notify humans. Thresholds, suppression and routing are declared together in
`cloudwatch-stack.ts` so they can be reviewed as a set. Do not add a second
notifier for an AWS-side signal.

"AWS-side signal" means a metric emitted by an AWS resource in this account.
The database host (Hetzner) and the frontend are outside it, each needing its
own decision — see "Considered and withdrawn" for why the host is excluded on
purpose.

Every alarm's `alarmDescription` is written for the person it wakes up: what is
happening, and where the runbook is. A metric is a thin interface — the number
says _whether_, the description must carry _what now_.

And the three rules — they apply to every alarm, including ones added later:

**2. Alarm on change, not on level, wherever the condition can persist.**
CloudWatch pages on state transition, so a level alarm that latches is silent
from its second minute and cannot signal the next incident. Standing conditions
belong on the dashboard; events belong on the pager.

To be explicit about what this rule is NOT: it is not repeated reminders. One
notification per NEW event, silence otherwise — a standing condition never
re-pages, and no re-notification machinery gets built (CloudWatch has none
natively; the EventBridge-plus-Lambda workaround is rejected below). This is
the AWS-documented shape for the DLQ case: `NumberOfMessagesSent` does not
count redrive moves, so AWS guidance is a RATE/DIFF over
`ApproximateNumberOfMessagesVisible` — growth, not level.

Implementation caveat, found before it bit: SQS stops publishing metrics for a
queue idle for hours, and `DIFF` over a series with gaps may swallow the first
failure after a drain. The growth alarms must be verified against an emptied,
idle queue (gate below), and `FILL(depth, 0)` considered if the gap behaviour
proves lossy.

**3. Absence is `BREACHING` only where nothing else witnesses the same
absence.** Paging twice for one fault is how alarms get muted. Every alarm
states in a comment what covers its silence.

**4. An alarm that cannot tell planned work from failure does not ship.**
Maintenance is routine here. The discrimination is a precondition, not a later
refinement — and the first place to look for it is a signal already present in
the data, before reaching for a suppression mechanism. The worked example: a
disabled event-source-mapping polls nothing, so `NumberOfMessagesReceived` is
exactly 0 during a pause and high during a failure. That turned an undeployable
alarm into a deployable one with no new signal and no new infrastructure.

The discriminator has a known blind spot: it reads "nobody is polling" as
"planned pause", and an ACCIDENTALLY disabled event source looks exactly the
same — queue growing, nothing polling, both the stall alarm and the producer
alarm silent, forever. So an alarm relying on such a discriminator ships with a
**slow backstop**: a second alarm on the bare signal with a threshold measured
in hours, which pages even during a planned pause. That is correct behaviour —
a pause outliving the backstop threshold deserves a "did you forget me" ping,
and it bounds the silent-failure window instead of leaving it open-ended.

---

## Considered and withdrawn

Recorded so they are not re-proposed without new evidence.

**A rule assigning every kind of check to a home (CI / scheduled Lambda / test /
the host itself).** It sounded orderly and was mostly unearned: once the
scheduled comparator below was dropped, the "scheduled Lambda" category had a
single candidate. Decide these case by case until there is enough of a pattern
to name.

**A scheduled comparator diffing declared state against production.** Dropped
because the evidence contradicts it. The comparison for infrastructure drift
already exists — `make diff-production` reports it — and the same drift was read
by a human on 2026-06-22, measured again on 2026-07-27, and was still pending on
2026-08-04. Running it on a timer would have produced the same report more
often, to the same effect. The gap is not detection.

Two further problems, either of which would have sunk it: undeployed work is
indistinguishable from drift without a second system to tell them apart, and
this account has a demonstrated 15-to-32-day tolerance for ignored alarms.

**Mandating that the database host publish metrics to CloudWatch.** It would add
an AWS credential to that host to carry exactly one boolean (a pending-reboot
flag). Rule 1 is deliberately scoped to AWS-side signals for this reason. For
signals like it, an external dead-man's-switch (a service paged by a MISSING
"I'm alive" ping — healthchecks.io and kin) is the indicated shape: absence-based
by construction, no AWS credential on the host, and independent of the whole
CloudWatch → Slack chain. Not adopted here; noted for 0237.

**Making hard failures throw so Lambda `Errors` increments** (and the two
existing `Errors`-based alarms start working untouched). Withdrawn as redundant:
the stall alarm catches a dead consumer from the queue side and the structured
write-failure filter catches the failure itself, while throwing would change
SQS retry semantics for no additional coverage.

**Composite alarm with an actions suppressor** — the AWS-native mechanism built
exactly for maintenance suppression, and the original proposal in the umbrella
task. Not taken because the suppressor needs a "maintenance is declared" alarm
to read, and no such signal exists — it would have to be published by new
scheduled infrastructure, whereas the `IF(received > 0, age, 0)` discriminator
uses a signal already in the data. Revisit if pauses ever get a first-class
declaration.

**Disabling alarm actions during the pause procedure** (`disable-alarm-actions`
before, enable after). Rejected: it depends on human memory in both directions,
and the forget-to-re-enable case creates a permanently mute alarm — the exact
defect class this ADR exists to remove.

**Periodic re-notification for alarms stuck in ALARM** (EventBridge schedule +
Lambda re-publishing to SNS). CloudWatch has no native re-notify; this bolt-on
rebuilds the reminder machinery rule 2 deliberately avoids, and reminders about
a known standing condition are noise by the rule's own definition. Rejected.

---

## Rationale

Choosing one engine means dedup, latching, re-arm, recovery notification,
history and grouping are solved for every future signal — including ones nobody
has thought of yet — instead of being rebuilt, worse, per notifier.

The saving is concrete. Under this ADR, 0428 becomes a freshness metric instead
of waiting for a prod-ClickHouse pager that does not exist, and 0454's alarms
become three edits to one file. The work goes down.

Rules 2–4 exist because each was paid for. Rule 2 cost 15 days of a mute alarm
and a second incident inside that window. Rule 3 cost a 30-day error metric that
read zero through seven lag events. Rule 4 was nearly missed entirely: the
obvious backlog alarm looked correct and would have been muted within a month.

---

## Alternatives Considered

### Alternative 1: A managed observability platform (Datadog / Grafana Cloud / Axiom)

**Cons:**

- Neither incident that produced this work would have been caught by one. Both
  needed an alarm on **absence**, not a log search.
- Shipping logs off the database box is the same egress line that drove a cost
  investigation; the proxy access log alone holds 11.5M entries.
- Adds a vendor, a bill and a secret to rotate, for two people.

**Decision:** REJECTED for now — revisit when more than two people are on call,
or when retention beyond CloudWatch's is required.

### Alternative 1b: Self-hosted Prometheus + Grafana (e.g. on the database box)

**Cons:**

- A third system to run for two people, and Alertmanager is yet another alarm
  state machine to configure and keep honest.
- Partially inside the system it observes — the box already runs the database
  it would be watching.

**Decision:** REJECTED — same revisit conditions as Alternative 1.

### Alternative 1c: A paging layer above CloudWatch (PagerDuty / Opsgenie)

Escalation chains, on-call rotations, acknowledgement, reminders.

**Cons:**

- All of that presumes a rotation; there are two people and no formal on-call.
- Another vendor, bill and secret.

**Decision:** REJECTED until an on-call rotation exists.

### Alternative 2: ClickHouse as the observability sink

**Cons:**

- Observability living inside the system it observes goes quiet exactly when it
  is needed.

**Decision:** REJECTED for the critical path, permanently.

### Alternative 3: Per-component notifiers (the status quo)

**Cons:**

- Five schedulers, four delivery paths, three secrets — each separately
  plausible, unmaintainable in total.
- Every notifier re-solves latching and dedup, badly.
- Nothing can answer "is anything wrong right now".

**Decision:** REJECTED — this is the defect, not the fix.

---

## Consequences

### Positive

- Adding an AWS-side check becomes "publish a number", not "build a notifier".
- Latching, re-arm and recovery notification are solved once.
- Rules 2–4 give a reviewer three concrete questions to ask of any new alarm.

### Negative

- Signals that originate outside AWS have no ruling here, so each still needs
  its own decision. That is the honest state, not an oversight.
- A metric is a thin interface: a check reports a number, not a sentence.
  Context has to live in the alarm description, written for whoever is woken up.
- CloudWatch retention bounds investigations (Lambda log groups keep 30 days).
  Accepted for now; revisit with Alternative 1.
- One engine means one delivery chain: SNS → Chatbot → Slack has no witness, so
  a broken link anywhere in it silences everything at once — and nothing pages
  about the silence. Cheap mitigation: a second subscriber on the topic (email
  is native and free) plus a Chatbot test message after every CloudWatch
  deploy. The full answer is an external dead-man's-switch (see "Considered and
  withdrawn"), which also witnesses CloudWatch itself. Which mitigation to take
  is an open decision, not yet made.

---

## Open / Pending (gates proposed → accepted)

- [ ] The ingestion-stall alarm verified by simulating a stall against the
      deployed alarm, not by reading the config
- [ ] The DLQ-growth alarms verified against an EMPTIED, idle queue — a test
      message into a drained DLQ must page, or the `DIFF`-over-metric-gaps
      caveat in rule 2 is real and `FILL` is required
- [ ] One alarm landed under each of rules 2, 3 and 4, so they are demonstrated
      rather than asserted
- [ ] Our alarm set cross-checked against CloudWatch's out-of-the-box
      recommended alarms for AWS/SQS, AWS/Lambda and AWS/ECS — anything they
      recommend that we lack gets a deliberate yes/no

---

## Delivery Checklist (per ADR 0032)

- [ ] `docs/architecture/technical-design-general-overview.md` — N/A, no change
      to system shape
- [ ] `docs/architecture/database-schema/database-schema-overview.md` — N/A
- [ ] `docs/architecture/backend/backend-overview.md` — N/A
- [ ] `docs/architecture/frontend/frontend-overview.md` — N/A
- [ ] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` — N/A
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md` — **required**;
      its observability section describes CloudWatch and X-Ray but not these
      rules. Update when this ADR moves to `accepted`
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` — N/A
- [ ] This ADR is linked from each updated doc at the relevant section

---

## References

- [Task 0455](../1-tasks/active/0455_OPS_observability-umbrella-declared-vs-actual-and-silent-failure/README.md)
  — the umbrella, and the production measurements behind rules 2, 3 and 4
- [AWS: SQS dead-letter queue troubleshooting](https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-dead-letter-queues-troubleshooting.html)
  — `NumberOfMessagesSent` does not count redrive moves (why rule 2's DLQ
  instance watches visible-depth growth)
- [AWS re:Post: monitoring SQS DLQs in production](https://repost.aws/questions/QUqS_YG85LTH2qAOfmxtyh2g/what-are-the-best-practices-for-monitoring-sqs-dead-letter-queues-in-production)
  — the `RATE(ApproximateNumberOfMessagesVisible) > 0` pattern
- [AWS Observability Best Practices: alarms](https://aws-observability.github.io/observability-best-practices/tools/alarms/)
  — composite alarms and metric math as the anti-fatigue tools
- [CloudWatch best-practice alarm recommendations](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/Best-Practice-Alarms.html)
  — the per-service recommended-alarm list behind the cross-check gate
