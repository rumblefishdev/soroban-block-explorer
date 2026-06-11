---
id: '0284'
title: 'FEATURE: Origin-lock synthetic canary for the edge-secret path'
type: FEATURE
status: done
related_adr: ['0048']
related_tasks: ['0277']
tags: [phase-future, effort-small, priority-low, security, observability]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: fmazur
    note: 'Spawned from 0277 future work.'
  - date: '2026-06-11'
    status: done
    who: fmazur
    note: >
      Folded back into 0277's PR (Copilot review #2) rather than deferred.
      cloudwatch-stack.ts now sets EXECUTE_API_URL when enableApiMtls OR
      enableEdgeSecretLock is live (was mTLS-only), so the edge-secret-only
      prod config gets a real API target instead of "No origin-lock targets".
      origin-lock.ts doc comment updated. CDK build green.
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

- [x] Canary target wiring covers the edge-secret path — `EXECUTE_API_URL` is set
      when `enableEdgeSecretLock` (not just `enableApiMtls`), so a direct
      execute-api hit (403, missing `X-Edge-Secret`) is probed and alarms on a
      2xx regression. (`enableOriginLockCanary` itself stays `false` in prod
      until we choose to turn the canary on — the wiring is now correct for it.)

## Implementation Notes

- `infra/src/lib/stacks/cloudwatch-stack.ts` — `EXECUTE_API_URL` gate changed
  from `enableApiMtls` to `enableApiMtls || enableEdgeSecretLock`.
- `infra/src/lib/canaries/origin-lock.ts` — doc comment now notes the 403 comes
  from the app-layer edge-secret check too, not only mTLS `disableExecuteApiEndpoint`.
