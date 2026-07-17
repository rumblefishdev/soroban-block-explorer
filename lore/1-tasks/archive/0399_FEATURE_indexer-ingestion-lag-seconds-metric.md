---
id: '0399'
title: 'Indexer: emit IngestionLagSeconds CloudWatch metric (seconds-based ingestion lag)'
type: FEATURE
status: completed
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
  - date: '2026-07-17'
    status: completed
    who: stkrolikiewicz
    note: >
      Closed. Shipped as specified — 2 files, +138/-15 (`bebd48c5`, PR #345,
      merged 07-17 11:54Z), no deviation from the plan. Verified live on
      production: `SorobanBlockExplorer/Indexer / IngestionLagSeconds`
      (Environment=production) reports avg 2.87–3.11 s / max 5 s over the two
      hours before closure, so the metric accumulates from day 1 of the AC6
      window. Dashboard-widget follow-up left unspawned (see Future Work).
---

# Indexer: emit IngestionLagSeconds CloudWatch metric

## Summary

Add an `IngestionLagSeconds` datum (wall-clock seconds between ledger close and
the row being written) to the indexer's existing per-ledger CloudWatch publish,
alongside `LastProcessedLedgerSequence`. This provides the seconds-based
end-to-end ingestion lag that the D3 AC3 dashboard and AC6 7-day report need and
that no current metric supplies.

## Status: Completed

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

- [x] `IngestionLagSeconds` visible in CloudWatch `SorobanBlockExplorer/Indexer`
      after a production deploy, tracking seconds since ledger close.
      Verified 2026-07-17 via `list-metrics` (the datum is registered with the
      `Environment=production` dimension) and `get-metric-statistics`
      (avg 2.87–3.11 s, max 5 s over the preceding two hours) — a live-emitting
      metric, not just a registered name.
- [x] `cargo check` + unit test green — CI green on PR #345; `lag_tests`
      covers the positive diff (30 s) and the clock-skew clamp (→ 0).
- [x] Deployed before public launch so the metric accumulates from day 1 (AC6).
      Prod datapoints exist from 2026-07-17; the frontend is still behind the
      pre-launch basic-auth gate at closure, so day 1 is not yet spent.
- [x] **Docs updated** — N/A: adds one operational CloudWatch metric; no change
      to schema, API endpoints, ingestion topology, or XDR parsing.
- [x] **API types regenerated** — N/A: no change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Implementation Notes

Landed in `bebd48c5` (PR #345, merged 2026-07-17 11:54Z) — 2 files, +138/-15,
confined to `crates/indexer/src/handler/mod.rs`. Shipped exactly as specified:

- `publish_ledger_sequence_metric` → `publish_indexer_metrics`, now taking
  `ledger_closed_at_secs: i64` alongside `ledger_sequence`; call site passes
  `parsed.ledger.closed_at` (`mod.rs:328`).
- `ingestion_lag_secs(now, closed_at) = (now − closed_at).max(0)` (`mod.rs:543`).
- Both datums go out in one `put_metric_data` call — `LastProcessedLedgerSequence`
  (`StandardUnit::None`) and `IngestionLagSeconds` (`StandardUnit::Seconds`),
  sharing the `Environment` dimension.

Deployed via `make -C infra deploy-production-compute` (indexer Lambda). No API
change, so no codegen.

## Issues Encountered

- **ID collision on 0399.** The ID was briefly held by a second task (the CH
  schema-drift one), which was renumbered 0399 → 0400 in `3602f4cf` before this
  task's PR merged. No impact on the implementation; noted so a future session
  reading `git log --grep=0399` is not confused by the `lore-0357`-scoped
  renumber commit appearing in the same result set.

No broken or modified tests: `lag_tests` is new and additive.

## Design Decisions

### From Plan

1. **Clamp negative lag to 0.** A validator's ledger close time can sit briefly
   ahead of the Lambda host's clock; without the clamp that surfaces as a
   spurious negative lag. The clamp is the plan's, and the unit test pins both
   directions.

2. **Second datum in the existing call, not a new publish.** The indexer already
   held a `CloudWatchClient` and already had `cloudwatch:PutMetricData` scoped to
   the namespace, so the lag cost one `MetricDatum` rather than any new wiring.

### Emerged

3. **`unwrap_or(ledger_closed_at_secs)` on `SystemTime` failure.** The plan did
   not say what to do if the clock read fails. Falling back to the close time
   makes `ingestion_lag_secs` return exactly 0 — a visibly implausible reading
   rather than a fabricated one, and it keeps the publish best-effort. The
   alternative (skip the datum) would have punched gaps in the AC6 window.

## Future Work

Not spawned as a task — raise one if AC3 needs it before launch:

- Add an `IngestionLagSeconds` widget to the CloudWatch dashboard in
  `infra/src/lib/stacks/cloudwatch-stack.ts`. This task only emits the metric;
  the AC6 report reads it via `get-metric-statistics`, so the widget is a
  surfacing convenience for AC3, not a blocker. Confirmed absent from
  `cloudwatch-stack.ts` at closure.
