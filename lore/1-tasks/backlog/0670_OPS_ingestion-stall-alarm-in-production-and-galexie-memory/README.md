---
id: '0670'
title: 'OPS: production gets the ingestion-stall alarm; Galexie memory gets an alarm'
type: OPS
status: backlog
related_adr: ['0054']
related_tasks: ['0454', '0455', '0553', '0560', '0650', '0653', '0672', '0673']
tags:
  ['effort-small', 'priority-high', 'monitoring', 'observability', 'galexie']
links:
  - infra/src/lib/stacks/ingestion-alarms.ts
  - docs/runbooks/health.md
history:
  - date: '2026-11-13'
    status: backlog
    who: stkrolikiewicz
    note: >
      Created from the 2026-11-13 read-only review of CloudWatch in the shared
      AWS account 750702271865. Every figure was re-measured on 2026-11-13
      against origin/master 9b628f52; commands and raw numbers in
      notes/R-measured-2026-11-13.md.
---

# OPS: production gets the ingestion-stall alarm; Galexie memory gets an alarm

## Summary

Production has no alarm on the indexer's own lag: `<env>-ingestion-stall` is
created only without Galexie, so only testnet has it. Production's two queue
alarms paged 9 times from 2026-10-29 to 11-13, for 2 incidents. This task
makes the stall alarm the one ingestion alarm in both environments, deletes
`production-galexie-ingestion-lag`, `production-ingestion-backlog-age` and
`testnet-ingestion-backlog-age`, and gives Galexie its first memory alarm.

## Stan teraz

- Done: findings verified in code and in read-only CloudWatch, 2026-11-13.
- Next: agree the change with the Prices and x402 teams (shared account), then
  Step 1.
- In force: ADR 0054 rules 2–5. Prices keeps its own BREACHING no-invocations
  alarm, so it loses attribution, not coverage.

## Context

**Gap 1: no stall alarm in production.**

- `ingestion-alarms.ts` branches twice on `galexieCluster && galexieService`.
  The block at :43 creates `galexie-ingestion-lag` (:70-87). The block at
  :146 creates `galexie-ephemeral-storage`, and its `else` (:205-236) creates
  `ingestion-stall`. `app.ts:37` builds Galexie only for
  `ledgerSource: 'galexie'`: production (`infra/envs/*.json:21`).
- `describe-alarms` has no `production-ingestion-stall`, yet
  `galexie-protocol-watch.md:94` says it pages.
- From 10-29 to 11-13, galexie-lag paged 4 times and backlog-age 5. Replayed
  on 1-minute `IngestionLagSeconds`, the stall shape gives 2 incidents
  (11-06 01:16 and 14:31), 5 minutes earlier than galexie-lag. On healthy
  days the production daily max was 7–30 s.
- Backlog-age also pages on healthy ingestion: a throttled doorbell (reserved
  concurrency 1, `handler/mod.rs:17-19`) hides for the 660 s visibility
  timeout. Testnet's backlog-age paged 5 times in 30 days, its stall alarm
  once.
- Only the stall alarm catches a ledger missing on S3. That case returns `Ok`
  and acks the doorbell (`handler/mod.rs:275-292`).

**Gap 2: Galexie memory.**

- AWS/ECS `MemoryUtilization` of the 13,312 MiB task, daily max:
  - 90–92% in early August;
  - 94–97% from 09-18 to 10-02;
  - 62–83% over the last 30 days, and 64.6% on 11-13.
- It climbs 0.3–0.5 points a day between restarts; nothing reads it.
- Not established: whether any restart was an OOM kill.

## Implementation Plan

### Step 1: one stall alarm per environment

In `ingestion-alarms.ts`, move `LakeStallAlarm` out of the `else` branch.
Every environment then gets `<env>-ingestion-stall`: `IngestionLagSeconds`
Maximum, 1-minute period, > 60 s for 3 of 3, BREACHING. The description and
runbook pointer name only the lake today; make them general.

Delete `GalexieLagAlarm` and `IngestBacklogAgeAlarm`. Keep the doorbell-rate
and backlog-age dashboard widgets: they are free and show which side failed.

ADR 0054 comments. Rule 2 (level carve-out): lag over 60 s is never an
accepted standing state; the alarm re-arms when ingestion resumes. Rule 3: it
is the only BREACHING ingestion witness. Rule 4: a planned pause pages once,
and a forgotten re-enable stays in ALARM.

### Step 2: Galexie memory alarm

Add `production-galexie-memory` on AWS/ECS `MemoryUtilization`, which is free
and stays if Container Insights goes (0673). Pick a threshold the team acts
on (restart the task or raise `galexieMemory`); rule 2 allows no standing
breach. Since 08-01, "> 90% for 3 hours" would have paged 4 times and stayed
above for 682 hours; 95% pages once (note). NOT_BREACHING: no data means no
task, which is the stall alarm's job (rule 3).

### Step 3: docs and readers

- Runbooks: `health.md:32-35` and `:56-73`, `live-tail-cutover.md:498-510`
  and `:858-886`, `galexie-protocol-watch.md:70`.
- Task criteria that name `galexie-ingestion-lag`: active 0650 README:128 and
  0653:76, backlog 0560:90 and :134.
- Prices: `docs/runbooks/xdr-protocol-watch.md:31` credits
  `production-galexie-ingestion-lag`. Tell that team.
- ADR 0054: the rule-3 example (:438-447) and the stall gate (:420-428) name
  the removed alarms. Add a history entry; do not rewrite the record.
- SCF milestone docs are historical: task 0672.

### Step 4: deploy and verify

Deploy testnet, then production. Rule 5: one message seen in the channel.
Re-run ADR 0054's stall gate: disable the indexer's event-source mapping and
expect exactly one page from `production-ingestion-stall` and one recovery.

## Acceptance Criteria

- [ ] `describe-alarms` shows both `*-ingestion-stall` alarms with the shape
      above, and none of the three deleted alarms.
- [ ] Infra tests assert each environment's ingestion alarm names
      (`testnet-environment.test.ts:160-162` does it for testnet only).
- [ ] Every new or changed alarm states its rule 2/3/4 answer in a comment;
      the memory alarm's comment gives the data behind its threshold.
- [ ] A planned pause in production gives exactly one page and one recovery.
- [ ] Rule 5: after the alarm-stack deploy, a message is seen in the channel.
- [ ] The runbooks and tasks in Step 3 are updated.
- [ ] **Docs updated**: `docs/architecture/infrastructure/infrastructure-overview.md`
      §8.2, the alarm list.
- [ ] **API types regenerated**: N/A, nothing under `crates/api`.

## Notes

Trade-offs, accepted:

- The alarm name no longer says producer or consumer (`health.md:56-58`).
  The two widgets that stay show it.
- If Galexie stops while a paused indexer has already paged, there is no
  second page.
- The absence witness moves from AWS/SQS to a custom namespace, published
  best-effort (`handler/mod.rs:578-627`). A failed publish gives a false
  page, never a silent one. Any writer of that namespace, an EMF line
  included, could hide a minute with a low point; testnet has this risk now.
- Lifting reserved concurrency 1 is not an alternative. The cursor needs it,
  and ESM MaximumConcurrency has a minimum of 2 (`compute-stack.ts:484-489`).

ADR 0054 Alternatives 1 and 1b reject Grafana; moving to it (x402 task 0138)
supersedes that ADR and is out of scope. If it happens, keep this alarm in
CloudWatch: a ClickHouse-SQL rule breaks Alternative 2 (rejected permanently).
