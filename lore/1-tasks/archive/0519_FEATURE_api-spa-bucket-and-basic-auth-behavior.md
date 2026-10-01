---
id: '0519'
title: 'FEATURE: separate /api SPA bucket + CloudFront behavior, basic-auth gated'
type: FEATURE
status: completed
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
  - date: '2026-09-23'
    status: completed
    who: stkrolikiewicz
    note: >
      Basic auth taken off the /api portal SPA (PR #480, merge 8e41e528):
      `enableApiSpaBasicAuth: false` in production.json. The portal's
      backend routes already live on prices-api.sorobanscan.rumblefish.dev,
      so this only exposes the static page; the main site was already public
      (enableBasicAuth off since 0405). `BasicAuthKvs` is now provisioned
      unconditionally — with both flags off the old condition would have
      deleted the store and its out-of-band-seeded credentials. `cdk diff
      --strict` showed only ApiSpaRoutingFunction changing and CloudFormation
      drift detection was IN_SYNC; deployed Explorer-production-Delivery from
      the branch (80 s) and verified anonymously: /api → 301 /api/; /api/,
      /api/dashboard, /api/docs → 200 with the portal index.html; hashed
      asset 200; / still 200; KVS READY. NOTE: commit 2f24beec on develop
      carries this task's message ("make the /api SPA public, keep the auth
      KVS") but contains only the 0574 task move — see Issues Encountered;
      the real change is #480. Archived.
---

# FEATURE: separate /api SPA bucket + CloudFront behavior, basic-auth gated

## Summary

Add a new S3 bucket (`${config.envName}-soroban-explorer-api-spa`) and a new
CloudFront behavior (`/api/*`) to `infra/src/lib/stacks/delivery-stack.ts`, so
a second, independently-built SPA can be deployed under that path prefix on
the existing distribution. Gate it with the existing CloudFront Function
basic-auth mechanism, behind a new independent config flag.

The gate was lifted on 2026-09-23 (PR #480): the `/api` SPA is public. The
flag and the shared KVS stay, so re-arming is a flag flip + Delivery deploy.

## Status: Completed

**Current state:** deployed; `/api` portal SPA public since 2026-09-23 (basic
auth off, KVS kept).

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
- [x] Deployed to production and verified live — redirect + deep-link
      fallback verified 2026-09-23; auth was enforced (401, realm "API") until
      #480 turned it off on purpose that day
- [x] Basic auth removed from `/api/*` without deleting the shared KVS
      (PR #480)

## Issues Encountered

- **Mislabeled commit on `develop` (2026-09-23).** A parallel session
  promoting 0574 stashed the uncommitted #480 work and switched the shared
  main checkout to `develop`; this task's `git commit` ran 12 s later and
  committed that session's staged 0574 task move under this task's message.
  Result: `2f24beec` on `origin/develop` ("make the /api SPA public, keep the
  auth KVS") holds only the 0574 file move. The work was recovered from the
  stash in a dedicated worktree and shipped as #480 (merge `8e41e528`).
  Left unrewritten: fixing the message means force-pushing `develop`, and
  `feat/0574_prices-api-link-in-nav-and-footer` is built on top of it.
- **`pnpm: command not found`** from `make -C infra deploy-production-delivery`
  in a terminal on nvm's node 24, which has corepack but no `pnpm` shim.
  Fix: `corepack enable pnpm`. Not a repo problem.
- **Delivery synth bundles every Compute Rust Lambda**, although Delivery has
  no stack dependencies — ~6 min in a fresh worktree before CloudFormation
  starts. Slow, harmless.

## Design Decisions

### From Plan

1. **One shared KVS, independent flags**: the main site and `/api/*` gate
   separately (`enableBasicAuth` / `enableApiSpaBasicAuth`) against one set
   of credentials; `/api` routing and auth share one viewer-request function
   because CloudFront allows one per behavior (see 2026-08-28 history).

### Emerged

2. **KVS provisioned unconditionally (#480)**: turning off the `/api` gate
   with the main gate already off would have dropped the store with its
   out-of-band credentials, so re-arming would need a manual re-seed.
   Production is the only environment and already had the store, so this
   adds no resource anywhere.
3. **Deployed from the PR branch before merge**, after `cdk diff --strict`
   and drift detection showed nothing else pending on Delivery; merged right
   after, so `develop` matches production.

## Notes

Deliberately out of scope for this task (raised and deferred in chat):

- No long-TTL sub-behaviors for `/api/assets/*`-style hashed paths yet — add
  once the new SPA's build tool/base-path convention is known. **Now known
  (2026-09-23):** the portal ships hashed `/api/assets/index-<hash>.js`, so a
  long-TTL `/api/assets/*` behavior is implementable; not filed as a task yet.

Previously deferred, now resolved (2026-08-28, see history): the
`errorResponses`/deep-link gap. `/api/*` routing now has its own CloudFront
Function rather than relying on the distribution-level `errorResponses`,
which can't be scoped per-behavior.
