---
id: '0288'
title: 'FEATURE: Runtime secrets via Secrets Lambda Extension (retire SECRETS_REVISION)'
type: FEATURE
status: backlog
related_adr: ['0048']
related_tasks: ['0277']
tags: [phase-future, effort-medium, priority-low, security, secrets, lambda]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: claude
    note: 'Spawned from 0277 future work.'
---

# Runtime secrets via Secrets Lambda Extension

## Summary

Read edge/jwt/turnstile/api-keys from Secrets Manager at runtime (Secrets Lambda Extension, already
used for mTLS/CH) instead of resolving into the Lambda env at deploy — removes the redeploy-to-rotate
limitation (`SECRETS_REVISION` lever) and the plaintext-in-env exposure.

## Context

0277 injects secrets via `secretValue.unsafeUnwrap()` (dynamic references resolved at deploy). A
secret rotation needs a template bump (`SECRETS_REVISION`) or CFN reports "no changes". The values
also land in the Lambda env config (GetFunctionConfiguration).

## Implementation

- Fetch secrets at cold start via the extension (like `MTLS_SECRET_NAME`); pass names, not values.
- App-side change in `config.rs`/`main.rs`; remove `SECRETS_REVISION`.

## Acceptance Criteria

- [ ] Secrets fetched at runtime; rotation takes effect without a redeploy; no secret in Lambda env
