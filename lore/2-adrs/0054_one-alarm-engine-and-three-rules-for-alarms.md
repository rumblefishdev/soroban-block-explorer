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

And the three rules — they apply to every alarm, including ones added later:

**2. Alarm on change, not on level, wherever the condition can persist.**
CloudWatch pages on state transition, so a level alarm that latches is silent
from its second minute and cannot signal the next incident. Standing conditions
belong on the dashboard; events belong on the pager.

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
flag). Rule 1 is deliberately scoped to AWS-side signals for this reason.

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

---

## Open / Pending (gates proposed → accepted)

- [ ] The ingestion-stall alarm verified by simulating a stall against the
      deployed alarm, not by reading the config
- [ ] One alarm landed under each of rules 2, 3 and 4, so they are demonstrated
      rather than asserted

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
