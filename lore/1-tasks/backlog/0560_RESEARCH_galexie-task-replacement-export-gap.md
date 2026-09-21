---
id: '0560'
title: 'RESEARCH: every Galexie task replacement costs 25–60 min of ledger export — measure where the time goes before choosing persistent state'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0367', '0548', '0001', '0108']
tags: [galexie, ingestion, infra, fargate, priority-medium, effort-medium]
links:
  - https://docs.aws.amazon.com/AmazonECS/latest/developerguide/ebs-volumes.html
history:
  - date: '2026-09-17'
    status: backlog
    who: stkrolikiewicz
    note: >
      Filed from a read-only investigation run from the stellar-prices-api
      side (CloudWatch, CloudTrail, ECS, `/ecs/production/galexie-live` logs)
      after the 2026-09-10 60-minute export gap starved both consumers. 0367
      listed "Persistent BucketList state (EFS)" as a follow-up candidate
      pending confirmation; it was never created. This is that task, reframed:
      measure first, because 0367's own numbers suggest the disk may not be
      the bottleneck.
---

# RESEARCH: Galexie task replacement export gap

## Summary

Every time the `production-galexie-live` task is replaced — by a deploy or by
the platform — ledger export to S3 stops for 25–60 minutes. Both consumers
starve: the explorer indexer and stellar-prices-api. The obvious fix is
persistent BucketList state, but nobody has measured where the time actually
goes, and 0367 timed the state download at ~4 min of a gap that reached 62.
Decompose the gap first, then decide whether any storage option is worth
building.

## Context

`production-galexie-live` (Fargate, 1 task, 100 GiB ephemeral storage, volumes
`data`/`tmp` = host/ephemeral) starts every new task with an empty disk. Captive
stellar-core must restore ~16 GB of BucketList state and then catch up, and
exports nothing to S3 meanwhile. Downstream, the prices `ledger-processor` gets
no S3 events and `rollup-freshness-1m` goes to ALARM.

This is known but unowned. Archived **0367** records "~20-min warmup is paid on
every Galexie restart/deploy" and lists "Persistent BucketList state (EFS)"
under candidates for follow-up backlog tasks (not auto-created, pending
confirmation). Active **0548** only budgets "~20 min of warmup".

### Evidence

Account 750702271865, eu-central-1. All times UTC. Note that the AWS CLI prints
CloudWatch/ECS times in the local zone (+02:00).

Export gaps ≥15 min, from `AWS/SQS NumberOfMessagesSent{QueueName=production-ledger-ingest}`
at 5-min resolution (normal = 54 / 5 min), full 62-day window
2026-07-17 → 2026-09-17:

| gap (UTC)              | length     | cause                                                               |
| ---------------------- | ---------- | ------------------------------------------------------------------- |
| 2026-08-12 03:55–04:20 | 25 min     | not investigated                                                    |
| 2026-08-27 00:35–01:15 | 40 min     | not investigated (see Notes — 0449 has a matching task replacement) |
| 2026-09-10 22:35–23:35 | **60 min** | task replaced by the platform, see below                            |
| 2026-09-14 11:20–11:50 | 30 min     | matches the deploy of task definition rev 8 (11:15 UTC)             |

Older data exists only hourly (gaps of ~1 h straddling an hour boundary are
invisible there); it shows the 2026-07-08 17:00 → 07-09 08:00 outage already
covered by 0367.

The 2026-09-10 incident in detail:

- 22:31:56 last normal upload; 22:31:59 Galexie logs `Received shutdown signal`,
  core shuts down cleanly (`got signal 2`). No error, OOM or panic before it.
  `context canceled` follows the signal; it is not the cause.
- ECS: `stopped 1 running tasks` 22:31:59, `started 1 tasks` 22:32:48 (49 s
  later).
- CloudTrail 20:00–00:00 UTC: **zero** `StopTask` / `UpdateService` / `RunTask`
  / `RegisterTaskDefinition` — nobody triggered it. The health check is
  `pgrep -x stellar-core` and core was closing ledgers to the last second, so it
  had no reason to fail. What is left is a platform-initiated replacement
  (Fargate task retirement). **Not provable from the CLI:** ECS keeps
  `stoppedReason` for ~1 h and the Health API needs a support plan
  (`SubscriptionRequiredException`). Someone with console access can check the
  AWS Health Dashboard history.
- New task: 22:33:11 `Starting Galexie`, resumes at ledger 64369144 (exactly
  where the old one stopped); first `Uploaded … successfully` at **23:35:16** →
  62 min, vs the ~20 min recorded in 0367. Unexplained.
- Backlog drained as a burst (363 messages in 5 min); everything OK by 00:24.
  No data lost.
- `production-galexie-ingestion-lag` fired 22:39, 12 min before the first prices
  alarm — it named the cause, but it lives in a different stack and SNS topic
  than the `prices-production-*` alarms.

## Implementation Plan

### Step 1: Decompose the 62 minutes

From the 2026-09-10 log stream (task `867d994c…2b0a3a989ad3`, 22:33 → 23:35):
state download vs state apply vs ledger catch-up. 0367 measured the 16 GB
download at ~4 min — if that holds, persistent disk saves little and the
bottleneck is elsewhere. **Do this first; it decides whether any storage option
is worth building.** Compare with the 25/30/40-min restarts to explain the
spread.

### Step 2: Verify the core assumption

Does captive core in Galexie actually resume from state found on disk, or does
it rebuild regardless? Test off-production (local, with a volume: run, stop,
start, time it).

### Step 3: Only then choose

- **Fargate + EFS** (0367's idea) — truly persistent. Open question: whether
  NFS meets stellar-core's ≥5k IOPS requirement (many small random reads;
  per-operation latency matters, not MB/s).
- **Fargate + EBS from snapshots** — plain EBS does NOT persist on Fargate. AWS
  docs: "Volumes that are attached to tasks that are managed by a service aren't
  preserved and are always deleted upon task termination", and an existing
  volume cannot be attached. Only "new volume from a snapshot" works, so it
  needs a snapshot routine; catch-up = snapshot age; snapshot volumes lazy-load
  (`volumeInitializationRate`). The note in 0001 saying Fargate has no EBS
  support at all is outdated — supported since platform 1.4.0.
- **EC2 + EBS** — the disk genuinely survives restarts, provisioned IOPS; the
  cost is owning the instance.
- **Hot standby (second Galexie instance)** — no disk change. 0001 says Galexie
  is built to run as parallel independent instances and uploads use
  `overwrite=false`. Not examined further.
- **Accept the gap** — a legitimate outcome if Step 1 shows no option moves the
  number much.

### Step 4: Related candidates from 0367, still open

A ledger-advance health check instead of `pgrep` (0108); and cross-stack alarm
wording so consumers are pointed at `production-galexie-ingestion-lag` first
(prices side: stellar-prices-api task 0288).

## Acceptance Criteria

- [ ] The 62 min of 2026-09-10 are split into download / apply / catch-up, with
      the spread across the four measured restarts explained or recorded as
      unexplained
- [ ] It is established by experiment whether captive core resumes from on-disk
      state
- [ ] A recorded decision: one of the options above, or "accept ~1 h per
      replacement" with the reason
- [ ] If accepted as-is: the expected gap is documented for consumers (explorer
      indexer, stellar-prices-api)
- [ ] **Docs updated** — N/A for the research itself; if the decision changes
      Galexie's storage or topology, the implementing task updates
      `docs/architecture/**` per
      [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — N/A — no `crates/api/**` change.

## Notes

- RESEARCH tasks are directories once active; convert this file to
  `0560_RESEARCH_…/README.md` with `notes/` when it is promoted.
- The 2026-08-27 gap (00:35–01:15 UTC) lines up with the task replacement 0449
  recorded independently: task `createdAt` 02:33 on 2026-08-27, no deploy that
  night, "almost certainly Fargate platform retirement". 0449 does not state
  the zone; if it is the CLI's local time (+02:00) that is 00:33 UTC, two
  minutes before the gap opens — confirm in Step 1. If it holds, that is two
  platform-initiated replacements in 62 days, so the gap is not only a
  deploy-time cost that scheduling can avoid.
- Cost context: Galexie is ~86% of the explorer's AWS bill (0449), so options
  that add a second instance or an EC2 host are not cheap relative to the whole.
- Related, other repo (stellar-prices-api): 0288 (alarm descriptions), 0243
  (`current_prices` freshness, born from the 09-14 gap).
- Source: 0001 `notes/R-ecs-fargate-sizing.md` (archive).
