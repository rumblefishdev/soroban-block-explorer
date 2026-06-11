---
id: '0284'
title: 'FEATURE: Origin-lock synthetic canary for the edge-secret path'
type: FEATURE
status: backlog
related_adr: ['0048']
related_tasks: ['0277']
tags: [phase-future, effort-small, priority-low, security, observability]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: claude
    note: 'Spawned from 0277 future work.'
---

# Origin-lock synthetic canary for the edge-secret path

## Summary
Recurring synthetic check that direct-origin access stays blocked — for the shipped X-Edge-Secret
lock (an acceptance criterion of 0277 not yet satisfied).

## Context
`validateConfig` was updated so `enableOriginLockCanary` accepts `enableEdgeSecretLock`, but the
canary target wiring (`cloudwatch-stack.ts`, `canaries/origin-lock.ts`) still keys on mTLS/origin-
secret. Direct execute-api currently returns 403 (good) but isn't monitored.

## Implementation
- Wire the canary to probe the API origin (execute-api) expecting 403, gated on `enableEdgeSecretLock`.
- Alarm on regression.

## Acceptance Criteria
- [ ] Canary enabled for the edge-secret path; alarms on direct-origin success
