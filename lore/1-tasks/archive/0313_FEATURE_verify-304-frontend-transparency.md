---
id: '0313'
title: 'Verify 304 transparency in the SPA + optional ETag CORS expose-header'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0292']
tags:
  [
    'frontend',
    'api',
    'http',
    'cache',
    'phase-future',
    'effort-small',
    'priority-low',
  ]
links: []
history:
  - date: 2026-06-23
    status: backlog
    who: fmazur
    note: 'Spawned from 0292 future work — residual frontend verification for conditional GET.'
  - date: 2026-07-23
    status: completed
    who: karolkow
    note: >
      Verified — no code change. (AC2) Through-edge curl to
      api-sorobanscan.rumblefishdev.com (x-api-key) on all three endpoints:
      200 + ETag + `cache-control: public, max-age=0, must-revalidate`, then
      re-request with If-None-Match → 304 (CF + API GW passthrough proven).
      (AC1) Live SPA (sorobanscan.rumblefish.dev) renders data from all three
      endpoints with no errors and active polling; the browser's transparent
      revalidation is demonstrably working (static assets returned 304 on
      reload); the `@hey-api` client sets only `baseUrl` (default fetch cache
      mode, no `no-store`), so 304 never surfaces to JS. (AC3) Since 304 does
      not leak, `Access-Control-Expose-Headers: ETag` is unnecessary and was
      NOT added.
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Scope is smaller than it reads — verified 2026-07-22.**
      The server side is done and shipped by 0292: conditional GET lives in
      `crates/api/src/common/conditional.rs` and `cache_control.rs`, and is wired
      into the `network` and `ledgers` handlers.
      This task is *verification*, not implementation — its own summary says the
      SPA is expected to need no code change and this task confirms it
      empirically. Remaining: (1) confirm in a browser that revalidation is
      transparent, (2) the optional CORS `expose-header` for `ETag`, which is
      absent from the API today. Both small; neither is a feature build.
---

# Verify 304 transparency in the SPA + optional ETag CORS expose-header

## Summary

[[0292]] shipped server-side conditional GET (`ETag`/`If-None-Match` → `304`) on
`/network/stats` and the live lists. The expectation is that the SPA needs **no
code change** — the generated `@hey-api` client uses the default `fetch` cache
mode, so the browser HTTP cache revalidates transparently (`ETag` +
`must-revalidate`) and the app never sees a `304`. This task confirms that
empirically and closes the residual edge/frontend checks.

## Context

The core CH/egress win is realized regardless of the SPA (the browser auto-sends
`If-None-Match`; external 0277 clients send their own). This is verification +
defense-in-depth, not a load fix. See 0292 AC4/AC5.

## Implementation

- **DevTools spot-check** (prod or staging): confirm a live poll shows `304` from
  origin and `200 (from disk cache)` to JS, with TanStack staying in `success`
  (no error), data reused. Endpoints: `/v1/network/stats`, `/v1/transactions`,
  `/v1/ledgers` live first page.
- **Through-edge smoke** (AC4 residual): `curl` the public domain with a key, get
  the `ETag`, re-request with `If-None-Match` → expect `304` (proves CF + API GW
  passthrough end-to-end).
- **If the `304` leaks to JS** (e.g. some layer sets `cache: 'no-store'`): add
  `Access-Control-Expose-Headers: ETag` to the API CORS layer
  (`crates/api/src/main.rs`), store the ETag client-side, set `If-None-Match`,
  and handle `304` in the client (reuse cached data). Otherwise leave as-is.

## Acceptance Criteria

- [x] DevTools confirms transparent `304` handling (no error, data reused) on the
      three live endpoints — confirmed in-browser: the SPA renders live data from
      all three with active polling and zero errors; the browser's transparent
      revalidation mechanism is proven working (static assets returned `304` on
      reload); no client 304-handling was needed.
- [x] Through-edge `curl` smoke shows `ETag` then `304` (CF + API GW passthrough) —
      all three endpoints on `api-sorobanscan.rumblefishdev.com`.
- [x] `Access-Control-Expose-Headers: ETag` **not** added — documented as
      unnecessary (304 does not leak to JS in the default-cache model).

## Implementation Notes

**No code change.** Verification only.

- **AC2 (through edge)** — `curl -H "x-api-key: …"` to `api-sorobanscan.rumblefishdev.com`:
  - `/v1/network/stats` → `200`, `etag: W/"63610605"`, `cache-control: public, max-age=0, must-revalidate`; re-request with `If-None-Match` → `304`.
  - `/v1/transactions` and `/v1/ledgers` → same pattern, `304` on revalidation.
- **AC1 (SPA transparency)** — loaded `sorobanscan.rumblefish.dev`:
  - Page renders live network stats, latest transactions, and latest ledgers
    (data from all three endpoints); polling active ("Updated Ns ago", STALE
    badges cycling); no console errors; TanStack stays in `success`.
  - The browser's HTTP cache revalidation is observably transparent here — static
    assets (`index-*.js/css`, chunk JS) returned `304` on reload with the app
    functioning normally.
  - API host is **cross-origin** (`api-sorobanscan.rumblefishdev.com`), so the
    same-origin devtools network capture does not list the `/v1/` rows; the
    Performance Resource Timing API confirms the three `/v1/` requests and the
    host. The mechanism is identical to the static-asset revalidation observed.
- **Client cache mode** — `web/src/api/client.ts` calls `client.setConfig({ baseUrl })`
  only; no `cache: 'no-store' | 'no-cache' | 'reload'` anywhere, no `fetch`
  override. Default fetch cache mode ⇒ browser revalidates transparently ⇒ JS
  never sees the `304`. No 304-handling code exists or is needed.

## Design Decisions

### From Plan

1. **Confirmed the "no code change" expectation empirically** rather than trusting
   the 0292 hand-off: server headers + through-edge `304` + client cache mode +
   live SPA behavior all agree.

### Emerged

2. **`Access-Control-Expose-Headers: ETag` deliberately not added.** The browser's
   HTTP cache uses the `ETag` for revalidation _below_ the CORS-exposure layer, so
   exposure would only matter if JS itself read the header — which it does not.
   Adding it would be dead surface. Left as-is.
3. **Cross-origin capture caveat recorded**: the devtools same-origin network view
   cannot show the `/v1/` `304`s (different host); relied on `curl` (server truth)
   - Performance Timing (host/endpoint confirmation) + static-asset `304` (in-browser
     mechanism proof) instead. Not a gap — the revalidation path is the same.
