---
id: '0660'
title: 'Portal hub at /api/: serve the Prices portal at /api/prices-api/'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0659', '0608', '0631', '0519']
tags: [infra, cloudfront, prices-portal, priority-medium, effort-medium]
links: []
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
---

# Portal hub at /api/: serve the Prices portal at /api/prices-api/

## Summary

The product portals gather under `/api/`: `/api/prices-api/` now,
`/api/x402-facilitator/` and `/api/sorobanscan/` later. `/api/` itself becomes
a hub that lists them. Every old portal URL (`/prices-api…`, `/pricing-api…`
and the pre-0608 `/api/<route>`) answers `301` to the same path under
`/api/prices-api`, query string kept. Done on the current domain, before 0663.

## Stan teraz

- Done: nothing yet.
- Next: the Prices build with the new base path (0631 before it if it can be).
- In force: one hop for every old URL, never a chain.

## Context

Today (`infra/src/lib/stacks/delivery-stack.ts`, 0608):

- `/prices-api` and `/prices-api/*` → `apiSpaBehavior(apiSpaFunction)`, bundle
  under the `prices-api/` prefix of `${envName}-soroban-explorer-api-spa`.
- `/api`, `/api/*`, `/pricing-api`, `/pricing-api/*` →
  `apiPathRedirectFunction`, a `301` to the same path under `/prices-api`.
- The old `api/` and `pricing-api/` bucket prefixes are still there (0631,
  backlog). The new `api/prices-api/` prefix would sit inside the old `api/`
  one.
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

Decide where the hub lives: a route of the explorer SPA, or a static page in
the portal bucket. `PRICES_API_URL` → `/api/prices-api/`.

## Acceptance Criteria

- [ ] `/api/prices-api/` serves the portal; Discord sign-in completes.
- [ ] `/prices-api/dashboard?x=1`, `/pricing-api/dashboard?x=1` and
      `/api/dashboard?x=1` answer one `301` to `/api/prices-api/dashboard?x=1`.
- [ ] `/api/` shows the hub.
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      (portal behaviors and redirects) and
      `docs/architecture/frontend/frontend-overview.md` (`/prices-api` routes).
- [ ] **API types regenerated** — N/A: nothing under `crates/api/**`.
