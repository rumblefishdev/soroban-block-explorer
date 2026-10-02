---
id: '0608'
title: 'Serve the Prices portal at /pricing-api/, 301 from /api'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0519']
tags: ['infra', 'cloudfront', 'prices-portal', 'effort-small']
links: []
history:
  - date: 2026-10-01
    status: active
    who: stkrolikiewicz
    note: >
      Task created. The Prices portal lives at /api/ on
      sorobanscan.rumblefish.dev, a name that reads as the explorer's own
      API; it moves to /pricing-api/ and the old path answers 301. Prices
      side: stellar-prices-api task 0326.
---

# Serve the Prices portal at /pricing-api/, 301 from /api

## Summary

The Stellar Prices portal (its own SPA, task 0519) is served at
`https://sorobanscan.rumblefish.dev/api/`. Move it to `/pricing-api/` and make
every old `/api…` URL answer `301` to the same path under `/pricing-api`, query
string kept, so no link already shared breaks.

## Stan teraz

- Done: code + docs (infra 13 tests, ui 95, lint + typecheck green); the
  Prices half is on `feat/0326_portal-at-pricing-api` in stellar-prices-api.
- Next: review + merge both PRs, then the deploy order below.
- In force: Delivery does not go out before the bundle is in the bucket under
  `pricing-api/`.

## Context

CloudFront hands the viewer path to S3 unchanged (no origin path), so the
bundle's prefix in `production-soroban-explorer-api-spa` is the URL prefix:
today `api/`. The bundle itself bakes the prefix in (Vite `base`, router
`basename`), and the Prices backend sends the OAuth popup back to
`<portalWebOrigin>/api/?signin=…` / `?issue=…`. So the move spans both repos;
the explorer half is the hosting and the redirect.

## Implementation Plan

1. `infra/src/lib/stacks/delivery-stack.ts`: the routing function and the two
   behaviours move to `/pricing-api` + `/pricing-api/*`; `/api` + `/api/*` get
   a second function that only redirects (`301`, path and query string kept).
   The `/api` behaviours keep the S3 origin — a behaviour needs one, the
   function answers before it is reached.
2. `infra/src/lib/cloudfront-functions/`: routing function on the new prefix,
   new redirect function.
3. `libs/ui/src/layout/links.ts`: `PRICES_API_URL` → `/pricing-api/`.
4. Docs: `docs/architecture/infrastructure/infrastructure-overview.md`,
   `docs/architecture/frontend/frontend-overview.md`.

### Deploy order (production, manual)

1. Prices: `make -C infra sync-portal-explorer` uploads the `/pricing-api/`
   bundle under `pricing-api/` (task 0326). `/api/` keeps serving the old one.
2. Explorer: Delivery stack — `/pricing-api/` goes live, `/api…` turns into
   `301`.
3. Explorer SPA and Prices Compute (`PORTAL_HOME`), any order — until then
   both reach the portal through the `301`.
4. Later: drop the old `api/` prefix from the bucket.

## Acceptance Criteria

- [ ] `/pricing-api/`, `/pricing-api/dashboard`, `/pricing-api/docs` answer
      `200` with the portal's `index.html`; bare `/pricing-api` → `301`
      `/pricing-api/`
- [ ] `/api`, `/api/`, `/api/dashboard?signin=failed` → `301` to
      `/pricing-api`, `/pricing-api/`, `/pricing-api/dashboard?signin=failed`
- [ ] Explorer footer / nav link points at `/pricing-api/`
- [ ] **Docs updated** — infrastructure-overview.md, frontend-overview.md
- [ ] **API types regenerated** — N/A — no change under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**`

## Design Decisions

### Emerged

1. **One S3 origin for all four portal behaviours.** A small
   `apiSpaBehavior(fn)` helper replaced two copy-pasted behaviour blocks. One
   `S3BucketOrigin` instance means the synthesized template drops the
   duplicate second origin + OAC for the same bucket (`Origin3`). Verified by
   diffing the synthesized Delivery template before and after: no other change.
2. **Redirect is its own function, not a branch of the routing one.** The
   `/api` behaviours never serve the bundle, so they need neither SPA
   rewriting nor the basic-auth check; the target behaviour is gated.
3. **Query values re-joined as the event hands them over**, no re-encoding.
   Exact for what the portal sends (`signin=…`, `issue=…`, `utm_source=…`).
   Check one percent-encoded value on production after the deploy.

## Notes

- `/api*` is not used by the explorer itself: its API is on its own hostname.
