---
id: '0108'
title: 'Galexie ECS container health check'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0034', '0039']
tags: [priority-high, effort-small, layer-infra]
links: []
history:
  - date: 2026-04-08
    status: backlog
    who: fmazur
    note: 'Task created — spawned from 0039 TODO in ingestion-stack.ts'
  - date: 2026-04-08
    status: active
    who: fmazur
    note: 'Activated task'
  - date: 2026-04-08
    status: completed
    who: fmazur
    note: >
      Implemented health checks on both containers (live + backfill).
      pgrep -x stellar-core verified locally against stellar/stellar-galexie image.
      1 file changed (ingestion-stack.ts). Lint + build passing.
---

# Galexie ECS container health check

## Summary

Add container-level health checks to Galexie ECS task definitions (live + backfill) so that ECS detects process crashes and restarts the task automatically. Without a health check, the existing `circuitBreaker: { rollback: true }` on the live service cannot act on container failures.

## Context

TODO at `infra/src/lib/stacks/ingestion-stack.ts:206`. Task 0034 deferred this until the image runs on staging.

Galexie spawns `stellar-core` as a child process (Captive Core). A `pgrep -x stellar-core` check detects core crashes while the parent `stellar-galexie` process is still alive. Verified locally: `pgrep` (`procps`) is available in the `stellar/stellar-galexie` image (Ubuntu 24.04 base). Hang detection (data freshness) is out of scope and belongs in CloudWatch alarms (task 0036).

## Implementation Plan

### Step 1: Add health check to live container (ingestion-stack.ts)

Add `healthCheck` to the `liveContainer` definition:

```typescript
healthCheck: {
  command: ['CMD-SHELL', 'pgrep -x stellar-core || exit 1'],
  interval: cdk.Duration.seconds(30),
  timeout: cdk.Duration.seconds(5),
  retries: 3,
  startPeriod: cdk.Duration.seconds(120),
},
```

- `CMD-SHELL` — runs via container shell
- `pgrep -x` — exact match on process name, verified available in image
- `startPeriod: 120s` — Captive Core needs time to catch up on startup
- `retries: 3` — tolerates transient process restarts during catchup

### Step 2: Add health check to backfill container

Same health check on the `backfillContainer`. Backfill is one-shot but still benefits from crash detection during long-running historical imports.

### Step 3: Remove the TODO comment

Delete the TODO block at lines 206–210.

## Acceptance Criteria

- [x] Health check added to live container definition
- [x] Health check added to backfill container definition
- [x] TODO comment at ingestion-stack.ts:206 removed
- [x] Build passes (typecheck, lint, build — all 4 projects)

## Implementation Notes

**File changed:** `infra/src/lib/stacks/ingestion-stack.ts`

- Added `healthCheck` property to both `liveContainer` and `backfillContainer` definitions
- Command: `pgrep -x stellar-core || exit 1`
- Removed 5-line TODO comment block

**Verification:** `pgrep` availability confirmed by running `docker run --rm --entrypoint bash stellar/stellar-galexie -c "which pgrep"` → `/usr/bin/pgrep`.

## Design Decisions

### From Plan

1. **`pgrep -x stellar-core` over `/metrics` endpoint**: Galexie's admin server (`admin_port`) reads config only from TOML file, not env vars. Enabling `/metrics` would require mounting a config file into the container — unnecessary complexity for process liveness. `pgrep` is simpler and available in the image.

### Emerged

2. **Same health check for backfill**: Backfill uses the same Galexie binary and spawns `stellar-core` identically. No reason to differentiate.
