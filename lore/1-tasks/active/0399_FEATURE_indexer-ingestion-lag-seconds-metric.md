---
id: '0399'
title: 'Indexer: emit IngestionLagSeconds CloudWatch metric (seconds-based ingestion lag)'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0127', '0129']
tags: [priority-high, effort-small, layer-indexer, milestone-3, phase-launch]
milestone: 3
links:
  - docs/scf/milestone-3-7day-report.md
history:
  - date: '2026-07-16'
    status: active
    who: stkrolikiewicz
    note: >
      Created for D3 closure. The 7-day post-launch report (AC6) and the AC3
      dashboard need a seconds-based end-to-end ingestion lag, but CloudWatch
      today has only throughput / sequence / queue-depth / Lambda-duration
      signals — no close-to-write lag. The indexer already publishes a custom
      metric with PutMetricData granted, so emitting the lag is a small addition
      in the same spot. Must ship before launch so the metric accumulates from
      day 1 of the window.
---

# Indexer: emit IngestionLagSeconds CloudWatch metric

## Summary

Add an `IngestionLagSeconds` datum (wall-clock seconds between ledger close and
the row being written) to the indexer's existing per-ledger CloudWatch publish,
alongside `LastProcessedLedgerSequence`. This provides the seconds-based
end-to-end ingestion lag that the D3 AC3 dashboard and AC6 7-day report need and
that no current metric supplies.

## Status: Active

## Context

The `ledgers` table stores no write time, so historical lag cannot be derived
from ClickHouse after the fact. CloudWatch today has:

- `AWS/SQS NumberOfMessagesSent` — Galexie "0 new ledgers" doorbell (throughput)
- `SorobanBlockExplorer/Indexer / LastProcessedLedgerSequence` — sequence number
- `…/ChWriteFailures`, `AWS/SQS ApproximateNumberOfMessagesVisible`, Lambda
  `Duration` — none of which is seconds-of-lag.

The indexer Lambda already knows `ledger.closed_at` (unix seconds) and wall-clock
`now()` at write time, already holds a `CloudWatchClient`, and already has
`cloudwatch:PutMetricData` scoped to the `SorobanBlockExplorer/Indexer`
namespace (`compute-stack.ts`). So the lag is a few lines in the same publish.

## Implementation

In `crates/indexer/src/handler/mod.rs`:

- Rename `publish_ledger_sequence_metric` → `publish_indexer_metrics`; take
  `ledger_closed_at_secs: i64` in addition to `ledger_sequence`.
- Emit a second `MetricDatum` `IngestionLagSeconds` (unit `Seconds`,
  `Environment` dimension) in the same `put_metric_data` call.
- `ingestion_lag_secs(now, closed_at) = (now − closed_at).max(0)` — clamp guards
  clock skew where a validator close time is briefly ahead of this host. Unit
  test covers positive diff + clamp.
- Call site passes `parsed.ledger.closed_at`.

Ships via `make deploy-production-compute` (indexer Lambda). No API change.

## Acceptance Criteria

- [ ] `IngestionLagSeconds` visible in CloudWatch `SorobanBlockExplorer/Indexer`
      after a production deploy, tracking seconds since ledger close.
- [ ] `cargo check` + unit test green.
- [ ] Deployed before public launch so the metric accumulates from day 1 (AC6).
- [ ] **Docs updated** — N/A: adds one operational CloudWatch metric; no change
      to schema, API endpoints, ingestion topology, or XDR parsing.
- [ ] **API types regenerated** — N/A: no change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Notes

Follow-up (optional, not this task): add an `IngestionLagSeconds` widget to the
CloudWatch dashboard in `infra/src/lib/stacks/cloudwatch-stack.ts` to surface it
for AC3 (this task only emits the metric; the report reads it via
`get-metric-statistics`).
