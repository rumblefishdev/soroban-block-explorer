---
id: '0519'
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
  - date: '2026-08-27'
    status: active
    who: mkowalski
    note: >
      Renumbered 0517 → 0519: this task was numbered off the highest ID
      visible on a local `master` checkout that had not yet merged develop's
      `2bcaece1` commit, which had already claimed 0516–0518 (including
      another, unrelated `0517_FIX_event-name-read-from-wrong-topic`).
      Renamed the file and updated the frontmatter id to the next free slot
      in the shared sequence; no content otherwise changed.
  - date: '2026-08-28'
    status: active
    who: mkowalski
    note: >
      Revisited the deep-link gap flagged in this task's own Notes ("Fine as
      long as the /api SPA doesn't need deep-link support; revisit if it
      does") after live testing showed `/api/` didn't resolve. Since
      CloudFront's distribution-level `errorResponses` always resolves
      through the _default_ behavior's origin regardless of which behavior
      originated the request, it can't be scoped to `/api/*` — the fix has
      to happen at the edge before the origin request. Added a new
      `apiSpaRoutingFunctionCode` CloudFront Function
      (`cloudfront-functions/api-spa-routing.ts`), attached to `/api/*` and
      to a new exact-match `/api` behavior (CloudFront's `/api/*` pattern
      requires the literal trailing slash, so bare `/api` never matched it).
      The function always rewrites any extensionless path to
      `/api/index.html` (covers both the bucket root and deep-linked
      client-side routes) and 301-redirects bare `/api` to `/api/`; it also
      folds in the existing basic-auth check (factored out of
      `basic-auth.ts` as `basicAuthCheckSnippet` to avoid duplicating the
      KVS-lookup logic) when `enableApiSpaBasicAuth` is on — CloudFront
      allows only one viewer-request function per behavior, so routing and
      auth can't be separate functions on the same behavior. The main-site
      `BasicAuthFunction` is now only constructed when `enableBasicAuth`
      itself is on (previously it was also the vehicle for `/api/*`'s
      auth); both functions share the same `BasicAuthKvs` KeyValueStore, so
      there's still only one credential to manage. Also added the matching
      `s3:ListBucket`/`PutObject`/`DeleteObject` permissions for
      `${envName}-soroban-explorer-api-spa` to the CI/CD deploy role in
      `cicd-stack.ts`, mirroring the main SPA bucket's grant (this bucket
      had none before). Design questions resolved in chat: full deep-link
      fallback (not just the bare `/api/` root), CloudFront Function
      mechanism (not S3 website hosting, which would require dropping
      OAC), and 301-redirect (not silent rewrite) for the no-trailing-slash
      case. `cdk synth`/`typecheck`/`lint` all pass for the production
      config; synthesized template inspected directly to confirm the
      function code, KVS association, and both `/api`/`/api/*` behaviors
      are wired as intended.
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
- [x] `/api/` (bucket root) and deep-linked `/api/*` client-side routes
      resolve to `/api/index.html` via a dedicated CloudFront Function,
      rather than 403/404ing from S3
- [x] Bare `/api` (no trailing slash) redirects to `/api/` — needs its own
      exact-match behavior since `/api/*` requires the literal slash
- [x] CI/CD deploy role granted S3 permissions on the new bucket
      (`cicd-stack.ts`)
- [ ] Deployed to production and verified live (redirect + deep-link
      fallback + auth still enforced)

## Notes

Deliberately out of scope for this task (raised and deferred in chat):

- No long-TTL sub-behaviors for `/api/assets/*`-style hashed paths yet — add
  once the new SPA's build tool/base-path convention is known.

Previously deferred, now resolved (2026-08-28, see history): the
`errorResponses`/deep-link gap. `/api/*` routing now has its own CloudFront
Function rather than relying on the distribution-level `errorResponses`,
which can't be scoped per-behavior.
