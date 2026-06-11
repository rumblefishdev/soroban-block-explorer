---
id: '0285'
title: 'FEATURE: Kill raw execute-api endpoint (disableExecuteApiEndpoint)'
type: FEATURE
status: backlog
related_adr: ['0048']
related_tasks: ['0277']
tags: [phase-future, effort-small, priority-low, security, defense-in-depth]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: claude
    note: 'Spawned from 0277 future work.'
---

# Kill raw execute-api endpoint

## Summary

Defense-in-depth: set `disableExecuteApiEndpoint=true` so the raw `execute-api` URL stops answering
entirely (today it is edge-locked → 403, but still reachable).

## Context

In 0277 the flag is coupled to `enableApiMtls` (unused). The edge-secret lock 403s the raw endpoint
but doesn't remove it. The canary (0284) probes execute-api, so coordinate.

## Implementation

- Decouple `disableExecuteApiEndpoint` from `enableApiMtls`; ensure the CF custom-domain base-path
  mapping is live first (else 403 your own edge).
- Coordinate with 0284 (canary target).

## Acceptance Criteria

- [ ] Raw execute-api no longer resolves/answers; CF path unaffected
