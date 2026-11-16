---
id: '0670'
title: 'Route /api/x402-facilitator/ to the x402 portal, send unknown /api/* paths to the hub'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0660', '0608', '0664', '0659', '0519']
tags: [infra, cloudfront, x402-portal, priority-medium, effort-small]
links: []
history:
  - date: '2026-11-16'
    status: active
    who: akot
    note: >
      Task created and implemented in one PR. Asked for by stellar-x402 task
      0106 (its API key portal, design decisions 1 and 6): the page is served
      here, beside the Prices portal, first as a hello-world page behind Basic
      Auth. Not deployed yet.
---

# Route /api/x402-facilitator/ to the x402 portal, send unknown /api/\* paths to the hub

## Summary

`https://sorobanscan.com/api/x402-facilitator/` serves the API key portal of
stellar-x402 from the API SPA bucket, beside the Prices portal, behind HTTP
Basic Auth with a password of its own while it is built. The other `/api/*`
paths stop reading as Prices ones: only the routes the Prices portal had at
`/api/` still answer `301` to `/api/prices-api/…`, the rest go to the hub
with a `302`.

## Stan teraz

- Done: code, tests and docs, in the PR. Nothing deployed.
- Next: after the merge, the manual steps in "Deploy order", in that order.
- In force: the gate's key is written before the deploy, so the page is never
  served without a password. The hub tile for x402 is a PR of its own.

## Context

Task 0660 gathered the portals under `/api/` and named `/api/x402-facilitator/`
as the next one. Before this task every `/api/*` path that was not the Prices
portal or the hub answered `301` to the same path under `/api/prices-api`, so
`/api/x402-facilitator/` led to `/api/prices-api/x402-facilitator/` (checked on
production, 2026-11-16).

stellar-x402 builds the page and uploads it with `aws s3 sync` to
`s3://<env>-soroban-explorer-api-spa/api/x402-facilitator/`. This repo gives
only the routing. The portal's backend is on a host of its own
(`x402-portal.sorobanscan.com`) and is not part of this task.

## Implementation Plan

1. `api-spa-routing.ts` takes the path prefix and the Basic Auth key, instead
   of `/api/prices-api` and `auth-token` written in. The Prices portal's
   function keeps the source it has.
2. `delivery-stack.ts`: a second function from that source for
   `/api/x402-facilitator`, gated by `enableX402PortalBasicAuth` with the key
   `auth-token-x402` of the existing KeyValueStore; behaviors
   `/api/x402-facilitator` and `/api/x402-facilitator/*` above `/api/*`.
3. `/api/*`: the Prices portal's old routes → `301` to the same path under
   `/api/prices-api`, query kept; anything else → `302` to `/api/`.
4. `enableX402PortalBasicAuth`: `true` in production, `false` on testnet.

### Deploy order (production, manual)

1. Write `base64(user:password)` under `auth-token-x402` in
   `production-soroban-explorer-basic-auth` (the command is in
   `delivery-stack.ts`). The store exists; no behavior reads the key yet.
2. `make deploy-production-delivery`.
3. Upload `index.html` to
   `s3://production-soroban-explorer-api-spa/api/x402-facilitator/index.html`.
4. Check: `401` without the password; with it `200` and the uploaded page's
   body (a missing file also answers `200`, with the explorer's page);
   `/api/dashboard` → `301` `/api/prices-api/dashboard`; `/api/anything` →
   `302` `/api/`; the Prices portal as before.

## Acceptance Criteria

- [ ] `/api/x402-facilitator/` answers `401` without credentials and `200`
      with them; bare `/api/x402-facilitator` → `301` with the trailing slash
      (on production, after the deploy)
- [x] Without the key in the store the gate answers `503`, never the page
      (unit test; the key name is its own, not `auth-token`)
- [x] `/api/docs`, `/api/dashboard`, `/api/login`, `/api/quick-start`,
      `/api/privacy-policy`, `/api/openapi.json` → `301` to the same path
      under `/api/prices-api`, query kept; any other `/api/*` → `302` `/api/`
      (unit tests; on production after the deploy)
- [x] The Prices portal's function source is unchanged: byte-identical to
      `develop`'s for every combination of gate and host move
- [x] No credentials in code, tests, template or docs
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      and `docs/architecture/frontend/frontend-overview.md`
- [x] **API types regenerated** — N/A: nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**`

## Design Decisions

### From Plan

1. **One source, one function per portal.** CloudFront allows one
   viewer-request function per behavior, and each portal has its own prefix,
   gate and host move.
2. **A key of its own in the shared KeyValueStore** (`auth-token-x402`): the
   x402 password changes without touching the main site's or the Prices
   portal's, and no second store has to be created and seeded.

### Emerged

3. **`/api/*` got a function of its own, `api-hub-redirect.ts`.** It needs two
   host moves, the Prices portal's for the old routes and the main SPA's for
   the hub, which differ while `pricesPortalOnLegacyHost` is on.
   `api-path-redirect.ts` serves `/prices-api…` and `/pricing-api…` only.
4. **The old-route list is the six the portal's router had before 0608**,
   checked against `web/portal/src/app/app.tsx` and `public/` of
   stellar-prices-api at the commit before its move (`16dcff9b~1`) and at its
   `develop` of 2026-11-16: the same routes. Matched on the first path
   segment, so `/api/docs/x` still goes to the portal.
5. **The redirect to the hub drops the query string**: the hub reads none.
6. **The x402 portal follows the main SPA's host move**, not the Prices
   portal's: on the old host it answers `301` to `sorobanscan.com`.
7. **The flag gates the password, not the route.** On testnet the behaviors
   exist with no gate and no files, so the path shows the explorer's
   not-found page (the distribution's `errorResponses`).
8. **`/api/assets/*` now goes to the hub.** Before, it went to
   `/api/prices-api/assets/*`; those hashed files are from before 0608 and
   are gone.
9. **Unknown paths answer `302`, not `301`** (review of PR #734). A browser
   keeps a `301` for good, and a path that is unknown today can be the next
   portal's: `/api/x402-facilitator` answered `301` to the Prices portal until
   this task, and the browsers that opened it then still go there. The six
   old routes keep `301`: they will never mean anything else.
10. **`api-path-redirect.ts` still strips `/api`** (review of PR #734),
    though no behavior under `/api/` points at it after the deploy. The
    function is updated in place, minutes before the distribution moves
    `/api/*` to `api-hub-redirect.ts`; without `api` in its pattern,
    `/api/docs` would answer `301` to `/api/prices-api/api/docs` in that
    window. Dropping it is a deploy of its own.

## Notes

- Hub tile for the x402 portal: a separate PR, when the portal opens.
- Follow-up, after this task's deploy: drop `api` from the pattern in
  `api-path-redirect.ts` (decision 10).
- A browser that opened `/api/x402-facilitator` before the deploy keeps the
  old `301` to the Prices portal; no change here undoes it. Test in a fresh
  profile.
- Turning the gate off at launch: `enableX402PortalBasicAuth: false` and a
  Delivery deploy.
