---
id: '0285'
title: 'FEATURE: Kill raw execute-api endpoint (disableExecuteApiEndpoint)'
type: FEATURE
status: active
related_adr: ['0048']
related_tasks: ['0277']
tags: [phase-future, effort-small, priority-low, security, defense-in-depth]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: fmazur
    note: 'Spawned from 0277 future work.'
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Still open, and the code that looks like the fix is inert.**
      `api-gateway-stack.ts:92` reads
      `...(config.enableApiMtls && { disableExecuteApiEndpoint: true })` — so the
      flag is set **only when API mTLS is on**, and `infra/envs/production.json:44`
      has `"enableApiMtls": false`. The raw `execute-api` URL therefore still
      answers today, exactly as this task's summary describes (edge-locked to
      403, but reachable).
      **The coupling is the finding, not the flag.** A defense-in-depth switch
      that costs nothing has been chained to an unrelated mTLS rollout, so it
      cannot be taken on its own — you get both or neither. Decide deliberately:
      either decouple it (`disableExecuteApiEndpoint` unconditionally, since
      nothing should reach the API except through the edge) or write down why it
      must ride along with mTLS.
      Note for whoever schedules this: flipping `enableApiMtls` is a production
      switch with blast radius well beyond this task. Not to be bundled in
      casually — see the arming preconditions in `infra/src/lib/types.ts:248-258`,
      which require the Turnstile / API-key secrets to be real before arming.
  - date: '2026-11-17'
    status: active
    who: karolkow
    note: >
      Activated after the 2026-11-17 security audit: the production
      execute-api address is public in the repo's load-test results and
      bypasses Cloudflare's per-IP rate limit while sharing the stage
      throttle with real users. Decoupled from mTLS on the owner's word.
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

## Design Decisions

### From Plan

1. **Decoupled from mTLS, set unconditionally.** `disableExecuteApiEndpoint: true`
   in every environment, no config flag: both environments serve through a
   custom domain (base-path mappings `api.sorobanscan.com` and
   `api-testnet.sorobanscan.com`, checked 2026-11-17), so no case needs the
   address open.

### Emerged

2. **The flag takes effect on deploy without a manual redeploy.** AWS applies
   it only on a stage update; CDK hashes the whole `CfnRestApi` into the
   deployment's logical ID, so the synth already carries a new deployment.
3. **Canary gating left as it is.** The origin-lock canary still probes the
   execute-api URL only when an API lock is on. With the address always closed
   the gate (and the matching `validateConfig` check) could go, but the canary
   is off in both environments; only its comments were corrected.
4. **Load tests go through Cloudflare only.** The guide's "direct to origin"
   path is gone, so a run above Cloudflare's per-IP rate limit needs that rule
   raised in soroban-infrastructure for the run.
