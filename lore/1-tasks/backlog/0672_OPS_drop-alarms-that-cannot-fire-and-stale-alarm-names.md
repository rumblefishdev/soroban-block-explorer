---
id: '0672'
title: 'OPS: delete the indexer error-rate alarms, make the worker alarm Errors > 0, fix stale alarm names'
type: OPS
status: backlog
related_adr: ['0054']
related_tasks: ['0454', '0455', '0553', '0670', '0671']
tags: ['effort-small', 'priority-medium', 'monitoring', 'observability']
links:
  - infra/src/lib/stacks/cloudwatch-stack.ts
history:
  - date: '2026-11-13'
    status: backlog
    who: stkrolikiewicz
    note: >
      Created from the 2026-11-13 read-only review of CloudWatch in the shared
      AWS account 750702271865. Every figure was re-measured on 2026-11-13
      against origin/master 9b628f52.
---

# OPS: delete the indexer error-rate alarms, make the worker alarm Errors > 0, fix stale alarm names

## Summary

Three cleanups that keep the same protection:

- A. Delete the two `*-ledger-processor-error-rate` alarms. They read Lambda
  `Errors`, which the indexer never raises.
- B. The two `*-enrichment-worker-error-rate` alarms stay, but read
  `Errors > 0` instead of errors / invocations.
- C. Correct the places that still name `indexer-ch-write-failures`.

This saves 6 alarm metrics (about $0.60 a month). Nothing loses a page it
gets today.

## Stan teraz

- Done: findings verified in code and in read-only CloudWatch, 2026-11-13.
- Next: agree the change with the Prices and x402 teams (shared account), then
  A and B in one alarm-stack deploy.
- In force: ADR 0054 rules 2, 3 and 5.

## Context

**A. Indexer error rate cannot fire.**

- Lambda `Errors` Sum is 0 over 91 days and 1,503,347 production invocations.
  It is 0 over 39 days and 699,512 testnet invocations.
- Neither alarm has changed state since 2026-05-27 (production) and
  2026-10-05 (testnet). Meanwhile production had 4 Galexie incidents and
  ClickHouse-failure pages.
- Every handled failure (s3, parse, clickhouse) is logged with
  `alarm="indexer_failure"` and returned as a batch-item failure inside `Ok`
  (`handler/mod.rs:185-221`). The event-source mapping uses batch 1 with
  `ReportBatchItemFailures` (`list-event-source-mappings`,
  `compute-stack.ts:479-483`).
- So `Errors` sees only panics, timeouts, OOMs and init failures. The alarm's
  own comment says it cannot see a stall (`cloudwatch-stack.ts:324-327`).
  ADR 0054 records Errors = 0 through seven lag events (:116-117).
- Which alarm covers each kind of failure without it:
  - a handled failure: `indexer-failures`;
  - a persistent crash or a bad config: the stall alarm (0670), or
    backlog-age until then, and after 10 receives the ledger DLQ;
  - a single crash that succeeds on redelivery: not paged. There were 0 in 91
    days, and the cursor reconciles it.
- Runbook lines credit this alarm with failures it cannot see:
  - `health.md:36` ("+ error-rate alarm");
  - `testnet-reset.md:57` (an undecodable protocol is a parse failure, so it
    shows in `indexer-failures` and the stall alarm);
  - `live-tail-cutover.md:873-876`.

**B. Worker error rate: the ratio is the wrong shape.**

- Invocations per 5-minute window over 30 days:
  - production: max 155, and 3 of 6,337 windows above 100;
  - testnet: max 72, and 0 of 8,000 windows above 100.
- So one error is above 1% in nearly every window, and `Errors > 0` behaves
  the same as the ratio.
- The dashboard widget's comment already calls the ratio "unreadable at this
  worker's traffic" (`cloudwatch-stack.ts:825-830`).
- Errors in 90 days: 6 in production (all 11-05) and 6 in testnet (all
  10-15).
- Do not delete these alarms. On 10-15 the worker alarm was the only page
  while the DLQ alarm sat latched (task 0671).

**C. Stale `indexer-ch-write-failures`.**

- The alarm was replaced by per-network `indexer-failures` (c38953c0,
  56399bcf, ffb495ae).
- `describe-metric-filters` shows only the two `IndexerFailures` filters.
  `ChWriteFailures` has had no data since 2026-11-12 about 14:45.
- Still named in:
  - backlog 0454 at :236 (step 7) and :251 (acceptance criterion);
  - `docs/scf/milestone-1-evidence.md:2638` and `docs/scf/milestone-1.md:52`.

## Implementation Plan

### Step 1: delete the indexer error-rate alarm (A)

Delete `ProcessorErrorRateAlarm` (`cloudwatch-stack.ts:290-330`). Keep the
Lambda Errors widget, which is native and free. Update the three runbook
lines above.

### Step 2: worker alarm on Errors (B)

`EnrichmentWorkerErrorRateAlarm` (:490-528) reads `workerErrors` only: Sum,
5 minutes, > 0, NOT_BREACHING.

Rule 2: an error count in a 5-minute window is an event, not a standing
level.

Once nothing reads `processorErrorRateThreshold`, delete it from `types.ts`
(:382-383, :586-590) and from `infra/envs/*.json`.

### Step 3: names (C)

- 0454: point step 7 and its criterion at `indexer-failures`. The task's
  owner says whether the criterion is already met.
- SCF milestone docs record what was true when they were submitted. Add a
  dated note under each table, "replaced by `production-indexer-failures` on
  2026-11-12", and do not rewrite the table. When 0670 lands, do the same for
  `production-galexie-ingestion-lag` (`milestone-1-evidence.md:2637`,
  `milestone-1.md:51`).

### Step 4: deploy

Agree the change with the other teams, then deploy testnet and production.
Rule 5: send one message through the chain and see it in the channel.

## Acceptance Criteria

- [ ] `describe-alarms` has no `*-ledger-processor-error-rate`.
- [ ] `*-enrichment-worker-error-rate` reads one metric, AWS/Lambda Errors.
- [ ] `processorErrorRateThreshold` is gone, or still read with a reason.
- [ ] The runbooks, 0454 and the SCF notes in Steps 1 and 3 are updated.
- [ ] Rule 5: after the deploy, a message is seen in the channel.
- [ ] **Docs updated**: `docs/architecture/infrastructure/infrastructure-overview.md`
      §8.2, the alarm list.
- [ ] **API types regenerated**: N/A.

## Notes

- Fallback for A, if the team still wants a crash page: replace the ratio
  with a single `Errors > 0` alarm, as in B. That saves 2 of the 4 alarm
  metrics.
- ADR 0054 Alternatives 1 and 1b reject Grafana. Superseding that ADR is out
  of scope.
