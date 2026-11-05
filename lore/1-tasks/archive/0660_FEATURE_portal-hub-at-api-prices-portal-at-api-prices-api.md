---
id: '0660'
title: 'Portal hub at /api/: serve the Prices portal at /api/prices-api/'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0659', '0608', '0631', '0519']
tags: [infra, cloudfront, prices-portal, priority-medium, effort-medium]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/699'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/701'
  - 'https://github.com/rumblefishdev/stellar-prices-api/pull/410'
history:
  - date: '2026-11-03'
    status: backlog
    who: stkrolikiewicz
    note: >
      Created under 0659. Team decision after conference feedback: the
      product portals gather under /api/ (prices-api now, x402-facilitator and
      sorobanscan later).
  - date: '2026-11-03'
    status: active
    who: stkrolikiewicz
    note: >
      Started. 0631 not done yet and no longer a blocker. The hub is a route
      of the explorer SPA.
  - date: '2026-11-04'
    status: completed
    who: stkrolikiewicz
    note: >
      Shipped in production-2026.11.04-1-Delivery and -2-web (#701), after
      the Prices bundle sync and a testnet rehearsal. Hub, portal, one-hop
      301s and a Discord sign-in verified on production. Prices Compute
      (PORTAL_HOME) rides with 0242's deploy, tracked in stellar-prices-api
      0339.
---

# Portal hub at /api/: serve the Prices portal at /api/prices-api/

## Summary

The product portals gather under `/api/`: `/api/prices-api/` now,
`/api/x402-facilitator/` and `/api/sorobanscan/` later. `/api/` itself becomes
a hub that lists them. Every old portal URL (`/prices-api…`, `/pricing-api…`
and the pre-0608 `/api/<route>`) answers `301` to the same path under
`/api/prices-api`, query string kept. Done on the current domain, before 0663.

## Stan teraz

- Done: live on production since 2026-11-04 (Delivery and SPA), testnet too.
- Left, in stellar-prices-api 0339: Prices Compute with the new `PORTAL_HOME`
  (until then the OAuth return reaches the portal through the 301), then
  dropping the dead `prices-api/` bucket prefix.
- In force: one hop for every old URL, never a chain.

## Context

Before this task (`infra/src/lib/stacks/delivery-stack.ts`, 0608):

- `/prices-api` and `/prices-api/*` → `apiSpaBehavior(apiSpaFunction)`, bundle
  under the `prices-api/` prefix of `${envName}-soroban-explorer-api-spa`.
- `/api`, `/api/*`, `/pricing-api`, `/pricing-api/*` →
  `apiPathRedirectFunction`, a `301` to the same path under `/prices-api`.
- The old `api/` and `pricing-api/` bucket prefixes were still there (0631,
  dropped 2026-11-04 before this deploy).
- The SPA links the portal through `PRICES_API_URL` in
  `libs/ui/src/layout/links.ts`.

## Implementation Plan

### Step 1: 0631, if it can go first

Recommended, not required: nothing reads the old prefixes (the `/api*` and
`/pricing-api*` behaviors answer 301 before S3). Dropped now, `api/` holds only
dead files and one delete clears it. Dropped after this task, the delete must
exclude `api/prices-api/`, or it takes the live portal with it.

### Step 2: Prices side (stellar-prices-api)

Build the portal with base `/api/prices-api/`, sync it to `api/prices-api/`.
Find every place the backend or the portal names `/prices-api/` (the post-login
return path, links) and move it.

### Step 3: CloudFront

- `/api/prices-api` + `/api/prices-api/*` → the portal behavior, listed before
  `/api/*` (behaviors match in order).
- `/api` + `/api/` → the hub.
- Redirect function: `/prices-api…`, `/pricing-api…` and every other `/api/…`
  → `301` to `/api/prices-api/…`, query kept.

### Step 4: Hub and links

The hub is a route of the explorer SPA (`ApiHubPage`): header, theme, consent
and GTM come with it. `PRICES_API_URL` → `/api/prices-api/`.

## Deploy order

1. Prices: sync the new bundle to `api/prices-api/`. Nothing serves it yet.
2. Explorer: Delivery (CDK) deploy, then the SPA. The SPA links the new path,
   so it never goes first; `/prices-api/` keeps working through the 301.
3. Prices Compute: `PORTAL_HOME` → `/api/prices-api/`. Never before 2: today
   `/api/prices-api/?signin=…` would land on `/prices-api/prices-api/`.

The old `prices-api/` prefix is then dead, like the 0631 ones.

## Acceptance Criteria

- [x] `/api/prices-api/` serves the portal; Discord sign-in completes (on
      production, 2026-11-04, through the `/prices-api/?signin=…` 301).
- [x] `/prices-api/dashboard?x=1`, `/pricing-api/dashboard?x=1` and
      `/api/dashboard?x=1` answer one `301` to `/api/prices-api/dashboard?x=1`.
- [x] `/api/` shows the hub.
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      (portal behaviors and redirects) and
      `docs/architecture/frontend/frontend-overview.md` (`/prices-api` routes).
- [x] **API types regenerated** — N/A: nothing under `crates/api/**`.

## Design Decisions

### From Plan

1. **The hub is an SPA route**, not a static page in the portal bucket: header,
   theme, consent and GTM come with it. The `/api/` behavior points at the SPA
   origin; a bare `/api` matches no behavior and falls to the default one, so
   it renders the hub too.

### Emerged

2. **Testnet rehearsal first.** `Explorer-testnet-Delivery --exclusively` and
   `deploy-testnet-web` from `develop`, the same diff as production, checked
   before the tags.
3. **Prices Compute held.** Prices deploys Compute from `develop`, which also
   carries stellar-prices-api#390 (0242); its decision D9 deploys Compute only
   after 0286 phase-3 stage D. The 301 keeps sign-in working meanwhile.

## Issues Encountered

- `make deploy-testnet-web` hung after a finished `nx build` (the Nx process
  never exited). `NX_TUI=false NX_DAEMON=false` got it through.
