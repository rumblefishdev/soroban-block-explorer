---
id: '0517'
title: 'FEATURE: separate /api SPA bucket + CloudFront behavior, basic-auth gated'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0273', '0302']
tags: [infra, cloudfront, s3, security, priority-medium, effort-small]
links: []
history:
  - date: '2026-08-27'
    status: active
    who: mkowalski
    note: >
      Created directly from a chat request (no prior backlog entry). User wants
      a second SPA, built and deployed independently from the main frontend,
      served from the existing CloudFront distribution under the `/api/*`
      path. It should be password-protected for now via the CloudFront
      Function + KeyValueStore basic-auth mechanism already used for
      `enableBasicAuth` (task 0273), reusing that same function code and KVS
      resource, but gated by its own independent config flag
      (`enableApiSpaBasicAuth`) so it does not force basic auth onto the main
      site's behaviors. Design questions resolved in chat: same distribution
      (new behavior, not a new distribution), shared basicAuthFunctionCode
      construct, new per-env config flag, single short-TTL behavior for now
      (no split-out long-TTL asset sub-paths until the new SPA's build output
      layout is known).
---

# FEATURE: separate /api SPA bucket + CloudFront behavior, basic-auth gated

## Summary

Add a new S3 bucket (`${config.envName}-soroban-explorer-api-spa`) and a new
CloudFront behavior (`/api/*`) to `infra/src/lib/stacks/delivery-stack.ts`, so
a second, independently-built SPA can be deployed under that path prefix on
the existing distribution. Gate it with the existing CloudFront Function
basic-auth mechanism, behind a new independent config flag.

## Status: Active

**Current state:** design agreed, implementation starting.

## Context

The current `DeliveryStack` serves exactly one SPA (the main block explorer
frontend) from one S3 bucket via one CloudFront distribution, with an
optional CloudFront Function basic-auth gate (`config.enableBasicAuth`,
task 0273) or an origin-secret lock (`config.enableOriginSecretLock`,
ADR 0048) — mutually exclusive, since CloudFront allows only one
viewer-request function per behavior.

A second, separate SPA is being introduced, to be served at `/api/*` on the
same domain/distribution. It needs its own S3 origin bucket, and — for
now — needs to sit behind HTTP basic auth regardless of whether the main
site's `enableBasicAuth` is on. Reusing the same `basicAuthFunctionCode`
CloudFront Function source and KeyValueStore avoids standing up a second,
separately-credentialed auth mechanism for no reason; a new
`enableApiSpaBasicAuth` flag controls whether that shared function gets
attached to the `/api/*` behavior, independently of whether it's attached to
the main behaviors.

## Implementation Plan

### Step 1: Config

Add `enableApiSpaBasicAuth: boolean` to `EnvironmentConfig` in
`infra/src/lib/types.ts`, documented like the other delivery-stack flags. Set
it in `infra/envs/production.json`.

### Step 2: S3 bucket

Add `apiSpaBucket` in `delivery-stack.ts`, same shape as the existing
`spaBucket` (block public access, S3-managed encryption, RETAIN+no
autoDelete in production).

### Step 3: Shared basic-auth function

Restructure the existing `if (enableOriginSecretLock) {...} else if
(enableBasicAuth) {...}` block so the basic-auth `KeyValueStore` +
`cloudfront.Function` are constructed whenever `enableBasicAuth ||
enableApiSpaBasicAuth` is true (not only `enableBasicAuth`), and track
separately which function (if any) attaches to the main behaviors vs. the
new `/api/*` behavior.

### Step 4: CloudFront behavior

Add an `additionalBehaviors['/api/*']` entry: origin = `apiSpaBucket` via
OAC, `shortTtlCachePolicy`, same `responseHeadersPolicy`, and the shared
basic-auth function attached only when `enableApiSpaBasicAuth` is true.

### Step 5: Outputs + docs

Add a `CfnOutput` for the new bucket name. Update
`docs/architecture/infrastructure/infrastructure-overview.md`'s CloudFront/S3
section per ADR 0032.

## Acceptance Criteria

- [x] `enableApiSpaBasicAuth` config flag added and documented
- [x] New S3 bucket provisioned with OAC-only access
- [x] `/api/*` CloudFront behavior added, independently gated
- [x] `cdk synth` succeeds for the production config
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      CloudFront section updated to mention the second SPA bucket/behavior.
- [x] **API types regenerated** — N/A, no `crates/api`/`Cargo.*`/`libs/api-types`
      changes.

## Notes

Deliberately out of scope for this task (raised and deferred in chat):

- No long-TTL sub-behaviors for `/api/assets/*`-style hashed paths yet — add
  once the new SPA's build tool/base-path convention is known.
- No change to the global `errorResponses` (403/404 → `/index.html`) —
  those still resolve through the _default_ behavior's origin (the main SPA
  bucket), so a client-side-routed deep link under `/api/*` would currently
  get the wrong SPA's `index.html` on a hard refresh. Fine as long as the
  `/api` SPA doesn't need deep-link support; revisit if it does.
