---
id: '0595'
title: 'BUG: the SPA index.html has no Cache-Control, so a browser can keep an old one that points at deleted assets'
type: BUG
status: completed
related_adr: []
related_tasks: ['0106', '0593']
tags: [frontend, infra, deploy, priority-medium, effort-small]
links:
  - infra/Makefile
  - infra/src/lib/stacks/delivery-stack.ts
history:
  - date: '2026-09-29'
    status: backlog
    who: stkrolikiewicz
    note: >
      Seen during the 0593 SPA deploy on 2026-09-29. Right after the sync, a
      browser with a cached index.html got a blank page.
  - date: '2026-09-29'
    status: active
    who: stkrolikiewicz
    note: >
      Started. Cache-Control at upload and a split sync that keeps the
      previous build's assets, as the Prices portal deploy does.
  - date: '2026-09-30'
    status: completed
    who: stkrolikiewicz
    note: >
      Merged in #547 (c9f0a704). Deployed on 2026-09-30 at 06:58 UTC with the
      new recipe and the SPA content already live (52e0b07a,
      `index-BmVZ6qtC.js`, same hash), because develop's frontend (0374
      soroban providers, 0376) needs an API that is not deployed yet. 61
      uploads, 0 deletes. Headers verified live. The previous-build check
      waits for the next content deploy. Infra 8 tests (+3).
---

# BUG: the SPA index.html has no Cache-Control, so a browser can keep an old one that points at deleted assets

## Summary

After an SPA deploy, a visitor whose browser holds the previous `index.html`
gets a blank page. That document points at hashed assets the deploy has just
deleted. The browser keeps it because the response carries no
`Cache-Control`, so its freshness is the browser's own guess. [[0106]] fixed
the CloudFront edge TTL, not the browser cache.

## Context

Observed on 2026-09-29 after the 11:10 UTC deploy ([[0593]]). The browser
pane loaded its cached `index.html`. It referenced `index-CIZ7rvdJ.js`, and
the module failed with "Expected a JavaScript-or-Wasm module script but the
server responded with a MIME type of text/html". A reload with a fresh URL
loaded the new build.

Three things combine:

1. **No browser cache header.** `curl -sI https://sorobanscan.rumblefish.dev/`
   returns `Last-Modified` and no `Cache-Control`. Browsers then apply
   heuristic freshness, typically a fraction of the time since
   `Last-Modified`. So the longer a build was live, the longer an old
   `index.html` can survive the next deploy. `ShortTtlCachePolicy` (60 s
   default, 5 min max, `delivery-stack.ts`) governs only the CloudFront edge.
2. **The deploy deletes old assets at once.** `deploy-production-web` runs
   `aws s3 sync web/dist/ s3://$BUCKET/ --delete` (`infra/Makefile`), so
   the old hashed files are gone the moment the new build lands.
3. **A missing asset returns `index.html` with 200.** The distribution's
   `errorResponses` map 403 and 404 to `/index.html` with status 200. A
   missing `.js` therefore arrives as HTML, and the page fails to load
   instead of retrying.

The Prices portal already does it right on the same distribution
(stellar-prices-api `infra/Makefile`, `sync-portal-explorer`). Assets go up
with `public, max-age=31536000, immutable`, everything else with
`public, max-age=0, must-revalidate`, and it syncs without `--delete`.

## Acceptance Criteria

- [x] `index.html` (and the apex) is served with a `Cache-Control` that makes
      browsers revalidate it. Live: `/`, `/index.html` and an SPA route (served
      by the error fallback) return
      `public, max-age=0, s-maxage=60, must-revalidate`, and a request with
      `If-None-Match` gets `304`.
- [x] Hashed assets are served with a long `immutable` `Cache-Control`.
      Live: `index-BmVZ6qtC.js` and `index-CkSoW2dv.css` return
      `public, max-age=31536000, immutable`.
- [ ] Right after a deploy, the previous build's assets still load. The
      mechanism is verified: the deploy logged 0 deletes, and a `--dryrun` of
      pass 2 from a directory holding only `index.html` deleted nothing under
      `assets/`. The live check needs a deploy that changes the bundle:
      afterwards `/assets/index-BmVZ6qtC.js` must still return JavaScript.
- [x] `docs/deployment.md` states what the SPA deploy does to caching
- [x] **Docs updated** — `docs/deployment.md` (the two-pass sync). The
      delivery stack does not change, and `docs/architecture/**` does not
      describe the SPA cache headers, so nothing there changes.
- [x] **API types regenerated** — N/A: nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**` changes.

The first deploy with the new recipe (2026-09-30) re-shipped unchanged
content, so it proved the headers but not the previous-build case.

## Details

The plan, implementation notes, design decisions and issues are in
[notes/S-implementation-and-deploy.md](notes/S-implementation-and-deploy.md).

## Notes

- A response headers policy on the default behaviour could force
  `Cache-Control` at the edge instead. Upload metadata keeps the rule next to
  the deploy that creates the files, which is how the portal does it.
- The `errorResponses` fallback (point 3) is what SPA routing needs. It
  should not change here, but it explains why the failure is a blank page
  rather than a 404.
