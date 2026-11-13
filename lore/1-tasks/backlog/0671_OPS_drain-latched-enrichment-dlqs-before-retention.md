---
id: '0671'
title: 'OPS: drain both enrichment DLQs before their messages age out (in ALARM since 2026-11-05)'
type: OPS
status: backlog
related_adr: ['0054']
related_tasks: ['0520', '0455', '0672']
tags: ['effort-small', 'priority-high', 'monitoring', 'enrichment', 'sqs']
links:
  - docs/runbooks/dlq.md
  - infra/src/lib/stacks/cloudwatch-stack.ts
history:
  - date: '2026-11-13'
    status: backlog
    who: stkrolikiewicz
    note: >
      Created from the 2026-11-13 read-only review of CloudWatch in the shared
      AWS account 750702271865. Every figure re-measured on 2026-11-13.
---

# OPS: drain both enrichment DLQs before their messages age out

## Summary

`production-enrichment-dlq-depth` and `testnet-enrichment-dlq-depth` have
been in ALARM since 2026-11-05. While an alarm is latched it cannot page the
next arrival. The messages are the ones that failed on 11-05, before the
gateway fix shipped. The queues keep messages for 14 days, so around 11-19
they will empty by themselves: the alarms will clear with nothing fixed, and
task 0520 will lose its test material. This task drains them per
`docs/runbooks/dlq.md`. No alarm or infra change is needed.

**Due by about 2026-11-19.**

## Stan teraz

- Done: state verified in read-only CloudWatch and SQS, 2026-11-13.
- Next: Step 1, testnet first.
- In force: ADR 0054 rule 2 allows a level alarm on a DLQ only because it
  gets drained (:159-164). `dlq.md` names "cleared because the messages aged
  out" as the failure this rule exists to stop.

## Context

Alarm state, from `describe-alarms` and alarm history (+01:00):

- Production has been in ALARM since 11-05 09:50:06, testnet since
  11-05 14:33:43.

Queue contents, from SQS and AWS/SQS on 2026-11-13:

- Depth: production 16 and testnet 43. The daily max has not changed since
  11-05.
- The dips to 13 and 33 on 11-10 and 11-11 were console polls: the messages
  came back.
- `MessageRetentionPeriod` is 1,209,600 s (14 days).
- The oldest messages are 709,853 s old in production and 692,921 s in
  testnet, so they entered on 11-05.

What happened on 11-05, from 1-minute depth:

- Production was redriven at 09:39 (30 → 0), and 16 messages were back by
  09:48.
- Testnet was redriven at 09:39 (104 → 0), and 85 were back by 09:49.
- Testnet was redriven again at 14:22: 22 back at 14:31 and 43 at 14:32.
- Each return paged once.

The worker was deployed after that (`get-function-configuration`
LastModified):

- testnet on 2026-11-06 13:13 UTC;
- production on 2026-11-12 13:37 UTC, the release that carries #705 and #709.

Depth has not reached 0 since, so there has been no redrive on the new code.
That redrive is 0520's step 3: "fetches must succeed through
`ipfs.filebase.io`".

The latch has already cost a page:

- On 2026-10-15, from 20:00 to 23:20 (+02:00), the testnet DLQ grew from 104
  to 424.
- `testnet-enrichment-dlq-depth` was in ALARM the whole time. Its next state
  change was ALARM → OK on 10-26.
- The DLQ alarm sent nothing. Only `testnet-enrichment-worker-error-rate`
  paged, at 22:38, 22:45, 23:05 and 23:11.

## Implementation Plan

### Step 1: inspect

For every message, read the body and `ApproximateReceiveCount` (`dlq.md`,
"When the alarm fires", steps 1–2).

0520 recorded the earlier contents as:

- production: 30 of 30 IPFS fetches;
- testnet: 46 IPFS fetches and 57 5xx from two issuer domains' `stellar.toml`.

Find which of these classes the remaining 16 and 43 belong to.

### Step 2: redrive

Testnet first, then production. Read the worker log after each.

### Step 3: resolve what comes back

Each message that returns ends in one of three ways: an existing task, a new
task, or an evidenced one-off (`dlq.md`). Then purge it.

Before purging, copy the keys 0520 still needs into 0520's notes.

### Step 4: confirm the re-arm

Both alarms are OK and both queues are at 0.

## Acceptance Criteria

- [ ] Both DLQs reach 0 and both alarms reach OK through a redrive or a
      recorded purge, not through retention.
- [ ] Every message class is attributed in 0520's notes or here.
- [ ] `dlq.md` says to redrive once the fix for the cause is deployed, if
      that step turns out to be missing.
- [ ] **Docs updated**: N/A, operational work that changes no shape.
- [ ] **API types regenerated**: N/A.

## Notes

- Do not add an alarm for this. Rule 2 rejects reminder machinery (ADR 0054,
  "Periodic re-notification ... Rejected"). The answer is the drain.
- Until the drain, worker crashes page only through
  `*-enrichment-worker-error-rate`, so keep that alarm (task 0672).
- The 2026-11-13 review said testnet "grew from 22 to 43 without a new
  notification". That is not a latch: both numbers came from one redrive
  (14:31, 14:32), and it paged once at 14:33. The 10-15 episode above is the
  real evidence.
- ADR 0054 Alternatives 1 and 1b reject Grafana. Superseding that ADR is out
  of scope.
