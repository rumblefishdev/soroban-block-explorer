---
id: '0313'
title: 'Verify 304 transparency in the SPA + optional ETag CORS expose-header'
type: FEATURE
status: backlog
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

- [ ] DevTools confirms transparent `304` handling (no error, data reused) on the
      three live endpoints — or explicit client handling added if it leaks.
- [ ] Through-edge `curl` smoke shows `ETag` then `304` (CF + API GW passthrough).
- [ ] If needed, `Access-Control-Expose-Headers: ETag` added; else documented as
      unnecessary.
