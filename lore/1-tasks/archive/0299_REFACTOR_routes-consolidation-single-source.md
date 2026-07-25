---
id: '0299'
title: 'REFACTOR: consolidate duplicated route tables — IdentifierDisplay href prop, single source of truth'
type: REFACTOR
status: completed
related_adr: []
related_tasks: ['0243', '0263', '0264', '0270']
tags: [frontend, refactor, routing, effort-medium, priority-low, phase-future]
links:
  - 'Origin notes: archive/0263, archive/0264, archive/0270 (senior-review follow-up, deferred to avoid scope-blow)'
history:
  - date: '2026-06-10'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0243 (CH read-path) review. The asset-routing contract
      change (route_token) surfaced the long-standing duplicate route-table
      smell again. Deliberately NOT done on the 0243 branch — unrelated to the
      CH migration, ~40 callsites, and conflicts with the 0257 audit line which
      also edits routes.ts. Captured here as its own task.
  - date: '2026-07-01'
    status: active
    who: karolkow
    note: >
      Promoted to active. Bundled with 0332 onto a single branch
      (feat/0299_0332_routes-consolidation-and-wim-read).
  - date: '2026-07-07'
    status: completed
    who: karolkow
    note: >
      Un-bundled from 0332 (unrelated CH read-path work) onto its own branch
      refactor/0299_routes-consolidation-single-source. Chose Option B (shared
      source in libs/ui) but landed a leaner 2-export shape after review:
      `routeSegments` (segment words) + `getIdentifierHref` (the one builder) —
      the per-type `identifierPaths` table was collapsed away. Also (beyond the
      original scope, agreed during review): moved `routeForHit` out of libs/ui
      into web/src/search typed via generated `SearchHit` (dropped hand-rolled
      `HitLike`; libs/ui stays zero-dep), pointed transaction-detail breadcrumbs
      at `routes.*`, and un-exported the dead `ELLIPSIS_CHAR`. All AC met;
      typecheck + 192 FE tests green (pre-commit gate). Commit 7d41e97c.
---

# REFACTOR: consolidate duplicated route tables → single source of truth

## Summary

There are **two** route-builder tables in the frontend that both encode the
same URL conventions (`/accounts/:id`, `/assets/:id`, `/nfts/:c/:t`, …). Drop
the duplicate, make `IdentifierDisplay` href-driven, and keep one source of
truth so a URL-shape change is a one-file edit.

## Context

- **App table:** `web/src/router/routes.ts` — `routes.{transaction,account,
asset,contract,nft,pool,…}` + `NAV_LINKS`. The real routing source.
- **UI-lib copy:** `libs/ui/src/identifiers/routes.ts` — a second `routes`
  Record + `getIdentifierHref` + `routeForHit`. Exists **only because
  `libs/ui` cannot import `web/src/router`** (dependency direction is
  `web → libs/ui`, not the reverse), so the lib carries its own copy so
  `IdentifierDisplay` can build hrefs.

`IdentifierDisplay` already accepts an optional `href` prop
(`libs/ui/src/identifiers/IdentifierDisplay.tsx`: `href ?? getIdentifierHref(
type, value)`) — the seam for this refactor exists.

Identified during the 0263/0264 senior review and kept as a future task to
avoid blowing those branches' scope; the 0270 work already deleted
`web/src/search/routeForHit.ts` and consolidated `routeForHit` INTO the
libs/ui table (one half of the cleanup). This task is the remaining half.

## Design decision to resolve FIRST (the wrinkle)

"Single source of truth in `web/src/router/routes.ts`" does **not** work
directly, because `libs/ui` cannot import from `web/`. Pick one:

- **Option A — UI lib goes href-only.** `IdentifierDisplay` stops building
  URLs entirely; every caller passes `href` (computed in `web/` from
  `web/src/router/routes.ts`). `routeForHit` moves to `web/`. Delete the
  libs/ui route table. App is the single source.
- **Option B — extract a shared route-pattern module** that BOTH `web/` and
  `libs/ui` import (e.g. a tiny `libs/.../routes` package). One table, both
  consumers depend on it.

A is less new structure but touches ~40 `<IdentifierDisplay>` callsites
(today **0** pass `href`). B keeps callers untouched but adds a shared
package. Decide before implementing.

## Implementation (after the decision)

- Resolve the dependency-direction decision (A vs B) above.
- (If A) Add `href` to all ~40 `<IdentifierDisplay>` callsites in `web/src`,
  computed via `web/src/router/routes.ts`; move `routeForHit` to `web/`;
  delete `libs/ui/src/identifiers/routes.ts` route table + `getIdentifierHref`.
- (If B) Create the shared route-pattern module; point both tables at it;
  delete the duplicate definitions.
- Keep the NFT composite branch + the asset `route_token ?? identifier`
  routing semantics intact (per 0243 — do NOT regress to "uniform
  identifier"; see archive/0280 Scope A rationale).
- Update `routeForHit.test.ts` + any href-dependent component tests.

## Acceptance Criteria

- [x] Exactly one place defines each entity's URL shape; the other is deleted
      or re-exports it. → `routeSegments` + `getIdentifierHref` in libs/ui;
      `web/router/routes.ts` and `router/index.tsx` re-use them.
- [x] `IdentifierDisplay` no longer carries an independent route table (it is
      href-driven, or imports the shared module). → uses `getIdentifierHref`.
- [x] All `<IdentifierDisplay>` callsites render correct links (no broken
      hrefs); `routeForHit` + NFT composite + asset `route_token` semantics
      unchanged.
- [x] FE tests green (`routeForHit.test.ts` + component tests). → 192 tests.
- [x] No new `libs/ui → web/` import (dependency direction preserved). →
      `routeForHit` moved the other way (into web); libs/ui stays zero-dep.

## Future Work

- None beyond this task; it IS the deferred follow-up. If Option B is chosen
  and the shared module grows, revisit packaging.
