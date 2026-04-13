---
id: '0118'
title: 'Indexer: publish LastProcessedLedgerSequence CloudWatch metric'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0036']
tags: [priority-low, effort-small, layer-indexing, rust]
milestone: 1
links: []
history:
  - date: 2026-04-13
    status: backlog
    who: FilipDz
    note: >
      Spawned from 0036. The "indexed vs network tip" dashboard widget was deferred
      because it requires the Ledger Processor Lambda to publish a custom CloudWatch
      metric. Task 0036 implemented all other widgets and alarms; this task completes
      the remaining AC.
---

# Indexer: publish LastProcessedLedgerSequence CloudWatch metric

## Summary

After each successful ledger processing the Ledger Processor Lambda should publish
a `LastProcessedLedgerSequence` custom metric to CloudWatch. This unblocks the
"indexed vs network tip" dashboard widget defined in task 0036.

## Context

Task 0036 added a CloudWatch dashboard with 11 widgets. One AC was deferred:

> Dashboard includes indexed vs network tip gap widget.

The widget cannot be built without a data source for the last indexed ledger sequence.
The Lambda already knows this value after processing each ledger; it just needs to
emit it to CloudWatch via `put_metric_data`.

## Implementation Plan

### Step 1 — Cargo dependency

Add `aws-sdk-cloudwatch` to `crates/indexer/Cargo.toml`:

```toml
aws-sdk-cloudwatch = { version = "1", default-features = false, features = ["rustls"] }
```

### Step 2 — Publish metric after each ledger

In `crates/indexer/src/handler/process.rs`, after a successful `process_ledger` call:

```rust
cloudwatch_client
    .put_metric_data()
    .namespace("SorobanBlockExplorer/Indexer")
    .metric_data(
        MetricDatum::builder()
            .metric_name("LastProcessedLedgerSequence")
            .value(ledger_sequence as f64)
            .unit(StandardUnit::None)
            .build()?,
    )
    .send()
    .await?;
```

The `CloudWatch` client should be created once and injected (same pattern as the
existing S3 / DB clients).

### Step 3 — IAM permission

Add `cloudwatch:PutMetricData` to the Ledger Processor Lambda execution role in
`infra/src/lib/stacks/compute-stack.ts`.

### Step 4 — Dashboard widget

In `infra/src/lib/stacks/cloudwatch-stack.ts` add a widget to the Ingestion section:

```ts
new cloudwatch.GraphWidget({
  title: 'Last processed ledger sequence',
  left: [
    new cloudwatch.Metric({
      namespace: 'SorobanBlockExplorer/Indexer',
      metricName: 'LastProcessedLedgerSequence',
      period: cdk.Duration.minutes(1),
      statistic: cloudwatch.Stats.MAXIMUM,
      label: 'Last indexed ledger',
    }),
  ],
  width: 12,
  height: 6,
}),
```

## Acceptance Criteria

- [ ] Ledger Processor Lambda publishes `LastProcessedLedgerSequence` to namespace
      `SorobanBlockExplorer/Indexer` after each successful ledger processing
- [ ] IAM role allows `cloudwatch:PutMetricData` for the processor Lambda
- [ ] CloudWatch dashboard includes the "last processed ledger sequence" widget
- [ ] Metric appears in CloudWatch within 1 minute of a ledger being processed
- [ ] No regression in Lambda cold start time or processing duration

## Notes

- The "vs network tip" comparison (true gap) would require querying the Stellar
  Horizon API or Galexie health endpoint — out of scope for this task. The widget
  shows the raw sequence; operators can compare manually against the network tip.
- `put_metric_data` is a cheap call (~$0.01 per 1000 metrics) and does not need
  batching at current ledger throughput.
