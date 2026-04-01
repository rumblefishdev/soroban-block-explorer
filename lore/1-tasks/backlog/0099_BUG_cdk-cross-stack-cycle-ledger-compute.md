---
id: '0099'
title: 'CDK: fix cross-stack cyclic dependency between LedgerBucket and Compute'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0033', '0021']
tags: [priority-high, effort-small, layer-infra]
milestone: 1
links: []
history:
  - date: 2026-04-01
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from 0021. Pre-existing cycle discovered during cdk synth.'
---

# CDK: fix cross-stack cyclic dependency between LedgerBucket and Compute

## Summary

`cdk synth` fails with `DependencyCycle` error. LedgerBucketStack and ComputeStack have a circular dependency: Compute uses `ledgerBucket.bucket` (Compute → LedgerBucket), while `ledgerBucket.addEventNotification(processorFunction)` in ComputeStack creates an implicit LedgerBucket → Compute dependency (S3 Lambda permission resource lives in the bucket's stack).

## Context

This cycle existed since task 0033 introduced the S3 event notification. It was discovered during task 0021 when running `cdk synth`. The cycle is independent of MigrationStack — it reproduces on the original code without migration changes.

Additionally, `compute.addDependency(migration)` cannot be added to enforce migration-before-compute ordering until this cycle is resolved.

## Possible Fixes

1. **Merge LedgerBucketStack into ComputeStack** — eliminates cross-stack reference entirely
2. **Use CfnBucketNotification** — low-level construct that avoids the implicit cross-stack permission
3. **Move S3 notification to a separate "glue" stack** — depends on both Compute and LedgerBucket
4. **Pass bucket name as string** instead of `IBucket` to ComputeStack, use `Bucket.fromBucketName()` — breaks the Compute → LedgerBucket reference

## Acceptance Criteria

- [ ] `cdk synth` completes without DependencyCycle error
- [ ] S3 PutObject notification still triggers the Processor Lambda
- [ ] `compute.addDependency(migration)` can be added in app.ts
