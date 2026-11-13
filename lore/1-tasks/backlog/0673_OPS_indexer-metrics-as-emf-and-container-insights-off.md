---
id: '0673'
title: 'OPS: indexer metrics as EMF, drop LastProcessedLedgerSequence, replace Container Insights with a sidecar'
type: OPS
status: backlog
related_adr: ['0054']
related_tasks: ['0399', '0553', '0653', '0670']
tags:
  ['effort-medium', 'priority-low', 'monitoring', 'cost', 'indexer', 'galexie']
links:
  - crates/indexer/src/handler/mod.rs
  - infra/src/lib/stacks/ingestion-stack.ts
history:
  - date: '2026-11-13'
    status: backlog
    who: stkrolikiewicz
    note: >
      Created from the 2026-11-13 read-only review of CloudWatch in the shared
      AWS account 750702271865. Every figure was re-measured on 2026-11-13
      against origin/master 9b628f52.
---

# OPS: indexer metrics as EMF, drop LastProcessedLedgerSequence, replace Container Insights with a sidecar

## Summary

Three changes to how metrics are emitted. Alarm protection stays the same,
and the cost drops by about $14 a month.

- A. The indexer calls PutMetricData once per ledger, and that pushes the
  whole account past the 1M free API requests. Emit the same metrics as EMF
  lines on stdout instead (about $2.3 a month).
- B. Stop publishing `LastProcessedLedgerSequence`. Only one widget reads it
  (about $0.60 a month).
- C. Turn Container Insights off on `production-ingestion`. One alarm reads
  it; feed that alarm from a sidecar instead (about $11 a month).

## Stan teraz

- Done: findings verified in code and in read-only CloudWatch, 2026-11-13.
- Next: after 0670 (a stall alarm then watches the switch everywhere), agree
  the change with the Prices and x402 teams.
- In force: same namespace, metric names and dimension. ADR 0054 rule 5 on
  every alarm-stack deploy.

## Context

**A. PutMetricData per ledger.**

- AWS/Usage CallCount for PutMetricData was 1,163,124 in the 30 days from
  10-14 to 11-12. It has run at 41,531 a day since 11-07.
- `IngestionLagSeconds` SampleCount was 518,348 in production plus 518,389 in
  testnet over 30 days, so the indexer made 1,036,737 calls: 89%.
- The call is made per ledger, inside the loop (`handler/mod.rs:392-408`).
  One `put_metric_data` sends both datums (:618-623).
- The October bill (`ce get-cost-and-usage`) has `EUC1-CW:Requests` at
  1,142,240 requests for $1.42: the first 1M is free, then $0.01 per 1,000.
  At the November rate that is about $2.46 a month.
- Feasible: the log group is STANDARD class, the runtime is
  `provided.al2023` and `LogFormat` is `Text` (verified).
- The cost of the EMF lines, estimated by the review and not re-checked: 0.2
  to 0.35 GB a month at $0.63/GB, about $0.13–0.22.

**B. LastProcessedLedgerSequence.**

- No alarm reads it. Both dashboards (`get-dashboard`) have one widget for
  it (`cloudwatch-stack.ts:731-744`) and none for `IngestionLagSeconds`.
- `handler/mod.rs:584-585` says `IngestionLagSeconds` "feeds the D3 AC3
  dashboard", which is false today.
- Other readers: `live-tail-cutover.md:579-591`, a one-off check, and
  `indexing-pipeline-overview.md:297`, a description.

**C. Container Insights.**

- `ingestion-stack.ts:151` sets `containerInsightsV2: ENABLED`. `list-metrics`
  gives 38 `ECS/ContainerInsights` series, all in `production-ingestion`: at
  $0.30 each, about $11.40 a month.
- Only `production-galexie-ephemeral-storage` reads them: used / reserved ×
  100 > 60 (`ingestion-alarms.ts:161-203`). No dashboard does.
- Turning it off also removes the log group
  `/aws/ecs/containerinsights/production-ingestion/performance` (1 day, 2.2 MB).

## Implementation Plan

### Step 1: EMF (A)

Replace `publish_indexer_metrics` with one EMF JSON line per ledger. Keep the
`SorobanBlockExplorer/Indexer` namespace, the `IngestionLagSeconds` name, the
unit Seconds and the `Environment` dimension.

These keep working untouched: `*-ingestion-stall`, the dashboards, the SCF
command (`milestone-3-7day-report.md:173`) and the string guard
(`declared-vs-emitted.spec.ts:156-184`), which only needs the name in
`crates/`.

Verify on testnet before removing anything. `list-metrics` must show the
series, and SampleCount must still be about 17,280 a day. If extraction is
wrong, the metric goes missing and the stall alarm pages: loud, not silent.

Then remove `cw_client` and the PutMetricData grant
(`compute-stack.ts:541-551`).

### Step 2: drop the sequence (B)

Stop emitting it, and replace the widget with `IngestionLagSeconds` Maximum,
which makes `handler/mod.rs:584-585` true. Point `live-tail-cutover.md:579-591`
at `SELECT max(sequence) FROM ledgers`, which the same runbook's B-2 already
queries. During a stall the lag goes absent, like the sequence, because it is
published only after a persist; it climbs only during catch-up.

### Step 3: sidecar instead of Container Insights (C)

- Add a non-essential container to the Galexie task. Every 5 minutes it reads
  `${ECS_CONTAINER_METADATA_URI_V4}/task`, takes `EphemeralStorageMetrics`
  `.Utilized` and `.Reserved` (MiB), and prints one EMF line.
- Point the alarm at those values with the same math, threshold, periods and
  NOT_BREACHING.
- Do not use `df`. Fargate reserves disk that the metrics leave out and `df`
  counts, per the AWS page "Fargate task ephemeral storage", read 2026-11-13.
  The 60% threshold and the ~30% baseline were calibrated on the metric's own
  denominator.
- Turn Container Insights off.

The memory alarm (0670) reads the free AWS/ECS series and is not affected.

## Acceptance Criteria

- [ ] `IngestionLagSeconds` arrives through EMF with an unchanged SampleCount
      (testnet, then production); the PutMetricData grant is removed.
- [ ] A month later, account PutMetricData (AWS/Usage) is under 1M a month.
- [ ] `LastProcessedLedgerSequence` is gone. Both dashboards plot
      `IngestionLagSeconds`.
- [ ] `production-galexie-ephemeral-storage` reads the sidecar's metric and
      has data. `list-metrics` shows no `ECS/ContainerInsights` series.
- [ ] Rule 5: after the alarm-stack deploy, a message is seen in the channel.
- [ ] **Docs updated**: `indexing-pipeline-overview.md:297` and
      `docs/architecture/infrastructure/infrastructure-overview.md` (metrics,
      Container Insights).
- [ ] **API types regenerated**: N/A.

## Notes

- The Step 3 deploy changes the task definition, so Galexie restarts. Expect
  about 25 minutes of catch-up and one stall page (`health.md:66-73`). Plan
  it with 0653.
- No IAM `cloudwatch:namespace` condition guards EMF: a writer of the log group can write the metric.
- If the account's other publishers grow, the explorer's 1.04M calls could
  cost up to about $10 a month: the saving grows with them.
- ADR 0054 Alternatives 1 and 1b reject Grafana. Superseding that ADR is out
  of scope.
