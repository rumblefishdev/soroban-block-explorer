---
id: '0405'
title: 'Launch: drop the pre-launch basic-auth gate from the production SPA'
type: FEATURE
status: active
related_adr: ['0048']
related_tasks: ['0273', '0277']
tags: [priority-high, effort-small, layer-infra, milestone-3, phase-launch]
milestone: 3
links:
  - infra/envs/production.json
  - infra/src/lib/stacks/delivery-stack.ts
history:
  - date: '2026-07-17'
    status: active
    who: stkrolikiewicz
    note: >
      Created for launch. `sorobanscan.rumblefish.dev` still answers 401
      `www-authenticate: Basic realm="Staging"` — the temporary human gate
      stood up by 0273 is the last thing keeping the explorer private. The
      change is one flag; the task exists because the repo's task gate requires
      one and because flipping it is what actually makes the site public.
---

# Launch: drop the pre-launch basic-auth gate from the production SPA

## Summary

Set `enableBasicAuth: false` in `infra/envs/production.json` and redeploy the
Delivery stack, which detaches the CloudFront viewer-request function gating the
SPA and makes `sorobanscan.rumblefish.dev` publicly reachable. This is the last
gate between the explorer and a public launch.

## Status: Active

## Context

Basic auth is **not** frontend code — nothing in `apps/web` implements it. It is
a CloudFront Function (viewer-request) on the SPA distribution, backed by a
KeyValueStore holding a single base64 `auth-token` key. It was stood up by 0273
as an explicitly temporary pre-launch gate.

Confirmed live at task creation (2026-07-17):

- `curl -sI https://sorobanscan.rumblefish.dev/` → `401`, header
  `www-authenticate: Basic realm="Staging"`.
- CloudFront function `production-soroban-explorer-basic-auth` — stage `LIVE`.
- `infra/envs/production.json:41` → `"enableBasicAuth": true`.

The `else if (config.enableBasicAuth)` branch in `delivery-stack.ts:128` creates
both the KVS and the function, so clearing the flag removes both.

Nothing consumes the credentials: no CI job, smoke test, or Playwright config
reads the `auth-token`. The only other reference is a comment in
`.github/workflows/deploy-staging.yml` — itself a fossil being retired by 0390.

## Implementation Plan

### Step 1: Flip the flag

`infra/envs/production.json` → `"enableBasicAuth": false`.

Nothing else needs to change:

- `enableWaf` stays `true`, so the `validateConfig` soft warning about an
  ungated distribution (which needs WAF **and** basic auth **and** the
  origin-secret lock all off) does not fire — the WAF, the Cloudflare edge
  secret (`enableEdgeSecretLock`), and the auth layer (`enableAuthLayer`) all
  stay armed. Only the _human_ gate goes away.
- The `enableBasicAuth` × `enableOriginSecretLock` mutual-exclusion error cannot
  fire either: `enableOriginSecretLock` is already `false`.

### Step 2: Deploy

`make -C infra deploy-production-delivery` — the Delivery stack only. Verify
with `curl -sI https://sorobanscan.rumblefish.dev/` → expect `200`, no
`www-authenticate`.

## Acceptance Criteria

- [ ] `enableBasicAuth: false` in `infra/envs/production.json`; `cdk synth`
      clean (no validation error, no ungated-distribution warning).
- [ ] After deploy, `https://sorobanscan.rumblefish.dev/` returns `200` with no
      `www-authenticate` header, and the SPA loads for an anonymous visitor.
- [ ] CloudFront function `production-soroban-explorer-basic-auth` and its KVS
      are gone from the distribution (`aws cloudfront list-functions`).
- [ ] WAF, edge-secret lock, and auth layer confirmed still armed after the
      deploy — this task removes the human gate only.
- [ ] **Docs updated** — N/A: flips an existing documented flag; no change to
      schema, API endpoints, ingestion topology, or XDR parsing. The
      basic-auth mechanism and its flag stay described where they already are.
- [ ] **API types regenerated** — N/A: no change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Notes

**Scope: flag flip only.** The machinery (`cloudfront-functions/basic-auth.ts`,
the `enableBasicAuth` flag in `types.ts`, the `delivery-stack.ts` branch, the
mutual-exclusion rule) stays. It works, it is tested, and it can be re-armed in
seconds if launch needs to be walked back or a maintenance window needs a human
gate. Deleting it is a much larger infra diff for no launch-day benefit —
deliberately deferred, not overlooked.

Post-launch, once re-arming is clearly not needed, the mechanism is a fair
deletion candidate: production is the only environment (the CI env named
`staging` _is_ production), so the flag is dead flexibility from then on.

**Possible deploy hiccup:** CloudFormation must dissociate the function from the
distribution before deleting the KVS. Both go in the same changeset, so ordering
should resolve on its own — if the KVS delete fails as still-referenced, deploy
once with the function detached, then again to drop the KVS.
