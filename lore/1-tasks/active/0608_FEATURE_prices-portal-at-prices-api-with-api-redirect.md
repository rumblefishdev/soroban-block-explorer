---
id: '0608'
title: 'Serve the Prices portal at /prices-api/, 301 from /api'
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
  - date: 2026-10-02
    status: active
    who: stkrolikiewicz
    note: >
      /pricing-api/ went live (PR #593 + Delivery deploy, 301 verified on
      production), then the name changed to /prices-api/: it matches the
      product, the repo and the API host prices-api.sorobanscan…, and
      "pricing" reads as a price list or a valuation engine. /pricing-api…
      joins /api… in the 301.
---

# Serve the Prices portal at /prices-api/, 301 from /api

## Summary

The Stellar Prices portal (its own SPA, task 0519) was served at
`https://sorobanscan.rumblefish.dev/api/`. Move it to `/prices-api/` and make
every old URL — `/api…`, and `/pricing-api…` from the first attempt — answer
`301` to the same path under `/prices-api`, query string kept, so no link
already shared breaks.

## Stan teraz

- Done: `/pricing-api/` live since 2026-10-02 ~10:00 UTC (#593, Delivery
  deploy). Rename to `/prices-api/` in code + docs; Prices half on
  `feat/0326_prices-api-path` in stellar-prices-api.
- Next: merge both PRs, then the deploy order below.
- In force: Delivery does not go out before the bundle is in the bucket under
  `prices-api/`.

## Context

CloudFront hands the viewer path to S3 unchanged (no origin path), so the
bundle's prefix in `production-soroban-explorer-api-spa` is the URL prefix.
The bundle itself bakes the prefix in (Vite `base`, router `basename`), and
the Prices backend sends the OAuth popup back to
`<portalWebOrigin>/api/?signin=…` / `?issue=…`. So the move spans both repos;
the explorer half is the hosting and the redirect.

## Implementation Plan

1. `infra/src/lib/stacks/delivery-stack.ts`: the routing function and its two
   behaviours on `/prices-api` + `/prices-api/*`; `/api` + `/api/*` and
   `/pricing-api` + `/pricing-api/*` on a second function that only
   redirects (`301`, path and query string kept). Those behaviours keep the
   S3 origin — a behaviour needs one, the function answers before it is
   reached.
2. `infra/src/lib/cloudfront-functions/`: routing function on the new prefix,
   redirect function for both old prefixes.
3. `libs/ui/src/layout/links.ts`: `PRICES_API_URL` → `/prices-api/`.
4. Docs: `docs/architecture/infrastructure/infrastructure-overview.md`,
   `docs/architecture/frontend/frontend-overview.md`.

### Deploy order (production, manual)

1. Prices: `make -C infra sync-portal-explorer` uploads the bundle under
   `prices-api/` (task 0326). Live paths keep serving what they serve.
2. Explorer: Delivery stack — `/prices-api/` goes live, `/api…` and
   `/pricing-api…` answer `301`.
3. Explorer SPA and Prices Compute (`PORTAL_HOME`), any order — until then
   both reach the portal through the `301`.
4. Later: drop the old `api/` and `pricing-api/` prefixes from the bucket.

## Acceptance Criteria

- [ ] `/prices-api/`, `/prices-api/dashboard`, `/prices-api/docs` answer
      `200` with the portal's `index.html`; bare `/prices-api` → `301`
      `/prices-api/`
- [ ] `/api`, `/api/`, `/api/dashboard?signin=failed`, `/pricing-api/docs`
      → `301` to `/prices-api/`, `/prices-api/`,
      `/prices-api/dashboard?signin=failed`, `/prices-api/docs`
- [ ] Explorer footer / nav link points at `/prices-api/`
- [ ] **Docs updated** — infrastructure-overview.md, frontend-overview.md
- [ ] **API types regenerated** — N/A — no change under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**`

## Design Decisions

### Emerged

1. **One S3 origin for all portal behaviours.** A small `apiSpaBehavior(fn)`
   helper replaced copy-pasted behaviour blocks. One `S3BucketOrigin`
   instance dropped the duplicate second origin + OAC the template carried
   for the same bucket (`Origin3`; deleted cleanly by the 2026-10-02 deploy).
2. **Redirect is its own function, not a branch of the routing one.** The
   old-prefix behaviours never serve the bundle, so they need neither SPA
   rewriting nor the basic-auth check; the target behaviour is gated.
3. **Query values re-joined as the event hands them over**, no re-encoding.
   Confirmed on production 2026-10-02: CloudFront hands them over still
   percent-encoded (`a%26b`, `%C3%A9` round-trip); parameter order may change.
4. **`/prices-api/`, not `/pricing-api/`** — see the 2026-10-02 history entry.

## Notes

- `/api*` is not used by the explorer itself: its API is on its own hostname.
