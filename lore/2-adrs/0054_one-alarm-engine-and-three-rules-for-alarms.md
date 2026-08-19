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
  - date: 2026-08-10
    status: proposed
    who: karolkow
    note: >
      Rule 4 rewritten after operator review: the pause discriminator
      (IF(received > 0, age, 0)) and its backstop are withdrawn as
      overcomplication. Decision: one knowing page per planned pause is the
      accepted cost; the bare ApproximateAgeOfOldestMessage threshold covers
      stall, planned pause and forgotten-disable with one alarm and no
      suppression logic. Measurements retained — they now set the bare
      threshold (120 s, bimodal distribution, no false-page cost).
  - date: 2026-08-11
    status: proposed
    who: karolkow
    note: >
      Defect-4 resolution converged after two operator challenges in one
      day. DIFF growth alarms withdrawn (the latch was a missing drain
      procedure, not a wrong alarm shape); the replacement
      last-chance-sentinel plug also withdrawn (memory machinery for a
      memoryless problem). Final shape, operator-proposed and
      measurement-endorsed (30 d: 100% of "transient" retries were
      connect-level dead domains, 0% genuine blips): connect-level fetch
      failures classify PERMANENT and sentinel immediately, so the DLQ
      receives only DB incidents and poison pills; level alarms stay under
      rule 2's new carve-out with re-arm answers in comments;
      docs/runbooks/dlq.md carries the drain procedure;
      --retry-sentinels repairs any host that returns from the dead.
  - date: 2026-08-19
    status: proposed
    who: karolkow
    note: >
      Rule 5 added after the delivery chain broke exactly as the consequences
      section predicted: a topic-policy addition revoked CloudWatch's publish
      right and all nine alarms were mute for 19 hours, found by accident. The
      ADR already carried the diagnosis and a suggested mitigation, but as a
      note rather than a rule, so nothing enforced it. Half of that suggested
      mitigation — an email co-subscriber — is withdrawn in the same pass: the
      denial happened above the subscribers, so email would have gone silent
      too. The external dead-man's-switch stays named and unadopted.
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

The rule has one named carve-out: **a level alarm is correct where policy
forces the steady state to zero** (the DLQs, the 5xx count). There the level
cannot latch-and-mute, because standing content is never accepted — the alarm
fires, the runbook drains/fixes, the alarm clears and is re-armed. The
latching observed in production was a missing drain procedure, not a wrong
alarm shape. Every level alarm still states its re-arm answer in a comment.

**3. Absence is `BREACHING` only where nothing else witnesses the same
absence.** Paging twice for one fault is how alarms get muted. Every alarm
states in a comment what covers its silence.

**4. A page caused by planned work the operator just performed is cheap;
suppression logic is expensive. Do not build it.** Maintenance is routine
here, and the operator who paused the indexer knows exactly why the backlog
alarm paged — one knowing page per pause is the accepted cost. What that page
buys is real: it also bounds the forgot-to-re-enable case, which any
pause-aware discriminator hides by construction (an accidentally disabled
event source is indistinguishable from a declared pause — queue growing,
nothing polling, silence forever). Prefer the bare signal. Revisit only if
knowing pages become genuinely noisy — with a count, not a feeling.

**5. The delivery path is verified on every change to it, before the change
is called done.** Rules 2-4 govern individual alarms; this one governs the
single path they all share. A deploy of the alarm stack is not finished until
one message has travelled the whole chain — CloudWatch → SNS → Chatbot →
Slack — and been seen in the channel.

Why a rule rather than a habit: on 2026-08-18 a topic-policy addition revoked
CloudWatch's right to publish to the alarm topic, and all nine alarms were
mute for 19 hours. Nothing paged about the silence; it was found by accident.
The change that broke it was a deploy, so a check bound to deploys catches
this class by construction.

Scope is the alarm stack, every time it deploys — including deploys that do
not appear to touch delivery, because this break was a side effect of an
unrelated policy statement. What counts as verification is a message that
arrives, not a diff that looks right: the diff for the breaking change was
read and approved.

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

**A second subscriber on the alarm topic (email — native, free, one line).**
Named in this ADR's own consequences as the cheap half of a delivery
mitigation; withdrawn 2026-08-19 once the outage measured it. The break was a
topic-policy denial, so the publish was rejected before any subscriber was
reached — an email subscriber would have gone silent alongside Slack. It
defends the lower half of the chain (Chatbot, Slack) while the observed break
was in the upper half. Reconsider only together with a witness that sits
outside the topic.

**Mandating that the database host publish metrics to CloudWatch.** It would add
an AWS credential to that host to carry exactly one boolean (a pending-reboot
flag). Rule 1 is deliberately scoped to AWS-side signals for this reason. For
signals like it, an external dead-man's-switch (a service paged by a MISSING
"I'm alive" ping — healthchecks.io and kin) is the indicated shape: absence-based
by construction, no AWS credential on the host, and independent of the whole
CloudWatch → Slack chain. Not adopted here; noted for 0237.

**Making hard failures throw so Lambda `Errors` increments** (and the two
existing `Errors`-based alarms start working untouched). Withdrawn as redundant:
the backlog-age alarm catches a dead consumer from the queue side and the structured
write-failure filter catches the failure itself, while throwing would change
SQS retry semantics for no additional coverage.

**Composite alarm with an actions suppressor** — the AWS-native mechanism built
exactly for maintenance suppression, and the original proposal in the umbrella
task. Not taken: the suppressor needs a "maintenance is declared" alarm to
read, and no such signal exists — it would have to be published by new
scheduled infrastructure.

**The `IF(received > 0, age, 0)` pause discriminator** — designed, measured
(pause windows show `NumberOfMessagesReceived` exactly 0, failure windows
53-54/5 min) and withdrawn 2026-08-10 as overcomplication. Two strikes: the
operator judged one knowing page per pause cheaper than logic that must be
understood, guarded and trusted later; and the discriminator manufactures its
own blind spot (accidental disable reads as pause → unbounded silence), which
then demands a second backstop alarm to patch — two alarms and a guard test
where one bare threshold does the whole job. The measurements stay valid and
now justify the bare alarm's threshold instead. Rule 4 was rewritten from
"discrimination is a precondition" to its current form as a result.

**Disabling alarm actions during the pause procedure** (`disable-alarm-actions`
before, enable after). Rejected: it depends on human memory in both directions,
and the forget-to-re-enable case creates a permanently mute alarm — the exact
defect class this ADR exists to remove.

**`DIFF(depth)` growth alarms for the DLQs** — proposed to solve the
latch-and-mute defect, withdrawn 2026-08-11 after the DLQ contents were
actually investigated. The 4 stuck messages were "forever-transient"
outside-world facts (dead-but-resolving issuer domains); the latch was a
missing drain procedure, not a wrong alarm shape. A plain level alarm works
under rule 2's carve-out — no metric math, no `FILL` gap caveat (a level
alarm needs one datapoint after an idle gap, not two consecutive), with the
drain procedure in docs/runbooks/dlq.md.

**A last-chance-sentinel intake plug for the enrichment worker** — built the
same day and withdrawn hours later at the operator's overengineering
challenge. It threaded a `last_attempt` flag through two crates plus an
env↔queue-policy contract to keep dead-end fetches out of the DLQ by adding
MEMORY (the SQS receive count) to a memoryless classifier. Superseded by a
simpler, memoryless answer the operator proposed and measurement endorsed:
30 days of "transient" retries were 100% connect-level dead domains (6 keys,
~1000 retries, one retried 668× in 83 minutes) and 0% genuine blips — so
connect-level failures now classify PERMANENT outright
(`http_transient.rs`), sentinel immediately, and the retry-budget memory is
unnecessary. 429/5xx/post-connect timeouts stay transient (the true blip
classes; measured zero occurrences). The rare returned-from-the-dead host is
repaired by `--retry-sentinels`. Return threshold: if sentinels start
landing on domains that provably recover within minutes, revisit the split.

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
  about the silence. **This is no longer a prediction.** It happened on
  2026-08-18, eight days after the sentence above was written, and stood for 19
  hours. Rule 5 is the answer taken: verification bound to the deploy, because
  a deploy is what broke it. The email co-subscriber is withdrawn — it sits
  below the break. The full answer remains an external dead-man's-switch (see
  "Considered and withdrawn"), which also witnesses CloudWatch itself; still
  not adopted, and its return condition is a break that a deploy-time check
  cannot see.

---

## Open / Pending (gates proposed → accepted)

- [ ] The ingest-backlog-age alarm verified by simulating a stall against the
      deployed alarm, not by reading the config — and one planned pause
      confirmed to produce exactly one knowing page
- [ ] The DLQ level alarms verified against an EMPTIED, idle queue — a test
      message into a drained DLQ must page (metric resumes with one
      datapoint)
- [ ] One alarm landed under each of rules 2, 3 and 4, so they are demonstrated
      rather than asserted
- [ ] Rule 5 demonstrated once: a deploy of the alarm stack followed by a
      message seen in the channel
- [x] Our alarm set cross-checked against CloudWatch's out-of-the-box
      recommended alarms (2026-08-11, SQS/Lambda/ECS/APIGW/SNS/CloudFront/S3)
      — every recommendation has a deliberate verdict; no gaps adopted, two
      forward pointers (C7 dashboard candidates, D2 synthetics question).
      Record: task 0455
      `notes/R-aws-recommended-alarms-crosscheck-2026-08-11.md`

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
