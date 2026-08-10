---
id: '0405'
title: 'Launch: drop the pre-launch basic-auth gate from the production SPA'
type: FEATURE
status: completed
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
  - date: '2026-07-17'
    status: completed
    who: stkrolikiewicz
    note: >
      Closed. PR #351 merged and deployed via `make -C infra
      deploy-production-delivery` — **the explorer is public as of today**.
      Verified in a browser, not just curl: anonymous load, `200` with no
      `www-authenticate`, live ledger data rendering, no console errors. The
      function and KVS are gone (`list-functions` / `list-key-value-stores` both
      empty); WAF, edge-secret lock and auth layer stay armed (the API still
      401s a raw request). Machinery deliberately kept — re-armable in seconds.
---

# Launch: drop the pre-launch basic-auth gate from the production SPA

## Summary

Set `enableBasicAuth: false` in `infra/envs/production.json` and redeploy the
Delivery stack, which detaches the CloudFront viewer-request function gating the
SPA and makes `sorobanscan.rumblefish.dev` publicly reachable. This is the last
gate between the explorer and a public launch.

## Status: Completed

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

- [x] `enableBasicAuth: false` in `infra/envs/production.json`; `cdk synth`
      clean (no validation error, no ungated-distribution warning). `cdk diff`
      was confined to the gate: destroy `BasicAuthFunction` + `BasicAuthKvs`,
      drop `FunctionAssociations` from the three cache behaviors, remove the
      `BasicAuthKvsArn` output. No IAM, nothing else.
- [x] After deploy, `https://sorobanscan.rumblefish.dev/` returns `200` with no
      `www-authenticate` header, and the SPA loads for an anonymous visitor.
      Verified in a browser, not just by curl: the page renders live data
      (latest transactions 15 s old, "Updated 6s ago"), no console errors. That
      also proves the Turnstile → JWT handshake still works, since the API
      answers a raw `curl` with `401 authentication required`.
- [x] CloudFront function `production-soroban-explorer-basic-auth` and its KVS
      are gone from the distribution — `list-functions` and
      `list-key-value-stores` both return empty; `DefaultCacheBehavior`
      `FunctionAssociations.Quantity` is 0.
- [x] WAF, edge-secret lock, and auth layer confirmed still armed after the
      deploy — this task removes the human gate only. Distribution still carries
      `WebACLId` → `production-soroban-explorer-cf`; the API still 401s an
      unauthenticated caller.
- [x] **Docs updated** — N/A: flips an existing documented flag; no change to
      schema, API endpoints, ingestion topology, or XDR parsing. The
      basic-auth mechanism and its flag stay described where they already are.
- [x] **API types regenerated** — N/A: no change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Implementation Notes

One line — `infra/envs/production.json` → `"enableBasicAuth": false` (PR #351,
merged 2026-07-17). Deployed with `make -C infra deploy-production-delivery`.
The explorer is public as of this task.

`api.sorobanscan.rumblefish.dev` answers `000` (connection failure) and that is
**not** a regression — the legacy API domain is off (`enableLegacyApiDomain:
false`). The SPA bundle targets `api-sorobanscan.rumblefishdev.com` via
Cloudflare, which is what actually serves it.

## Issues Encountered

- **Whole-app synth is broken in a space-containing worktree.** `make -C infra
synth-production` dies with `error: unexpected argument 'SSD' found` — the
  Rust Lambda bundling for the Compute stack does not quote the path, so
  `/Volumes/Extreme SSD 2TB/…` splits on the space. Unrelated to this change and
  not fixed here. Workaround: synth/diff the Delivery stack alone
  (`--exclusively`) — it bundles no Rust. The `deploy-production-delivery` make
  target is unaffected: Delivery's only dependency is `CloudFrontWaf`, not
  Compute.

- **ID collision on 0405.** A parallel session spawned a CI task on the same ID
  while this one was in flight. That session resolved it itself in `61392140`,
  renumbering its task 0405 → 0406 — matching the `3602f4cf` precedent (the ID
  stays with the task that has the PR). Second such collision in one day; the
  shared ID sequence has no allocation lock, so concurrent sessions race it.

## Design Decisions

### From Plan

1. **Flag flip, not deletion.** The basic-auth machinery stays: it works, it is
   tested, and it can be re-armed in seconds if launch needs walking back. A
   full deletion is a much larger infra diff for no launch-day benefit.

### Emerged

2. **Verified in a browser, not just with curl.** A `200` on the HTML only
   proves the gate is gone, not that the site works — the API sits behind a
   separate auth layer that a raw request cannot pass. Loading the page and
   seeing live ledger data is what actually demonstrates an anonymous visitor
   can use the explorer.

## Future Work

Not spawned — raise a task post-launch if wanted:

- Delete the basic-auth mechanism entirely (`cloudfront-functions/basic-auth.ts`,
  the `enableBasicAuth` flag, the `delivery-stack.ts` branch, the
  mutual-exclusion rule). Production is the only environment, so once re-arming
  is clearly not needed the flag is dead flexibility.
- Fix the unquoted bundling path so `synth-production` works from a worktree
  whose path contains a space.

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
