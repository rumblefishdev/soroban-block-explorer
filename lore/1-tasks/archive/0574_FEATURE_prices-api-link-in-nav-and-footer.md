---
id: '0574'
title: 'FEATURE: link to the Prices API from the navbar and the footer'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0519', '0407', '0494']
tags: [frontend, nav, footer, priority-medium, effort-small]
links: []
history:
  - date: '2026-09-23'
    status: backlog
    who: stkrolikiewicz
    note: >
      Created from a chat request: add a Prices API link to both the
      navbar and the footer. No existing task covered it (searched
      backlog/active/blocked on develop, all remote branches, and open
      GitHub issues).
  - date: '2026-09-23'
    status: active
    who: stkrolikiewicz
    note: 'Promoted to active; implementation starting.'
  - date: '2026-09-23'
    status: active
    who: stkrolikiewicz
    note: >
      Implemented: `/api/` link in the navbar (inline + drawer) and the
      footer, one shared constant, nav collapse breakpoint md → lg (the
      inline nav now needs ~1115px). 2 tests (1 new file). Open until
      deployed after 0519's basic-auth flip.
  - date: '2026-09-24'
    status: completed
    who: stkrolikiewicz
    note: >
      PR #481 merged 2026-09-23 (e0479d29). The SPA carrying the links went
      out at 08:06 UTC, 25 min after the basic-auth flip finished
      (ApiSpaRoutingFunction UPDATE_COMPLETE 07:40:45 UTC). Still live after
      the 12:13 UTC SPA deploy for 0577. Archived.
---

# FEATURE: link to the Prices API from the navbar and the footer

## Summary

The explorer does not link to the Prices API anywhere. Add one link in the
top navigation and one in the footer, so users can find it from any page.

## Status: Completed

**Current state:** live on production since 2026-09-23 08:06 UTC.

> ⚠️ Shipped from `develop`. Until `develop` reaches `master`, a
> `production-*` tag cut from `master` rebuilds the SPA without these links.

## Context

- Navbar: `libs/ui/src/layout/SecondaryNav.tsx`. Internal sections come from
  `NAV_ITEMS` in `web/src/router/AppShell.tsx`. External links do not go in
  `navItems`. They get their own element with an `ArrowOutward` icon, like
  `ReportBugLink` (task 0407). That element renders both inline and in the
  mobile drawer.
- Footer: `libs/ui/src/layout/Footer.tsx`. External links live in the
  `RESOURCES` list (GitHub, Stellar docs, …). The `FOOTER_EXPLORER_LINKS`
  list in `AppShell.tsx` is for internal routes only.
- Task 0519 adds a separate SPA under `/api/*` on the same CloudFront
  distribution, gated by `enableApiSpaBasicAuth`. That SPA is the Stellar
  Prices API portal (prices-api task 0305), so the link points at `/api/`.
  It is not a client route of this app, so it must be a plain `<a>` that
  does a full page load, not a router link.

## Implementation Plan

### Step 1: Pin the URL

`/api/` (see Design Decisions). Keep it in one constant shared by the nav
and the footer.

### Step 2: Navbar

Add a `Prices API` external link next to `Report a bug` in `SecondaryNav`,
styled the same way. Check both the inline nav and the drawer below the
collapse breakpoint.

### Step 3: Footer

Add `{ label: 'Prices API', href: … }` to `RESOURCES` in `Footer.tsx`.

### Step 4: Tests

Extend `Footer.test.tsx` to assert the link and its `href`. `SecondaryNav`
has no tests yet. Add `__tests__/SecondaryNav.test.tsx` (per the
CLAUDE.md test-placement rule) with one assertion for the new link.

## Acceptance Criteria

- [x] Navbar shows a `Prices API` link, inline and in the mobile drawer
- [x] Footer shows a `Prices API` link under Resources
- [x] Both links point at the same URL, defined once (`layout/links.ts`)
- [x] The link does a full page load (`<a href>`), not client-side routing
- [x] Not shipped while the target is still behind basic auth: deploy after
      0519's `enableApiSpaBasicAuth: false` is live in production — flip
      done 07:40:45 UTC, SPA with the links 08:06 UTC (2026-09-23)
- [x] **Docs updated** — `docs/architecture/frontend/frontend-overview.md`
      §5: `/api` is not an explorer route; linked with a plain anchor.
- [x] **API types regenerated** — N/A — no `crates/api/**`, `Cargo.*` or
      `libs/api-types/**` changes.

## Implementation Notes

- `libs/ui/src/layout/links.ts` — `PRICES_API_URL = '/api/'`.
- `SecondaryNav.tsx` — `ReportBugLink` generalised to
  `ExternalNavLink({ href, label, size })`; `Prices API` renders before
  `Report a bug`, inline after the separator and stacked in the drawer.
- `Footer.tsx` — `Prices API` first in `RESOURCES` (new tab, like its
  neighbours).
- Tests: `Footer.test.tsx` moved to `__tests__/` (CLAUDE.md rule) and gained
  one case; new `__tests__/SecondaryNav.test.tsx`. ui 88/88, web 376/376.
- Browser check: inline links clear at 1200px, drawer at 1024px, footer.
- Deploy: `Explorer-production-Delivery` shows the basic-auth removal
  (`ApiSpaRoutingFunction`) finishing at 07:40:45 UTC. The SPA's
  `index.html` was written at 08:06:13 UTC, 4 min after the #481 merge. That
  is the bucket's last write that morning: it keeps no object versions. The
  live bundle `index-CG4OxREA.js` (2026-09-24) still carries
  `PRICES_API_URL = "/api/"`, and `/api/` answers 200 without credentials.

## Design Decisions

### From Plan

1. **Plain anchor outside `navItems`**: `navItems` clicks go through the
   router, which would render `/api/` as the explorer's 404.

### Emerged

2. **Target `/api/`, relative**: the Stellar Prices API portal is served
   there on this distribution (prices-api task 0305). Relative, so it follows
   whichever host serves both SPAs.
3. **Label `Prices API`**: the portal titles itself "Stellar Prices API".
4. **New tab with the ↗ icon**: same treatment as `Report a bug` and every
   footer resource. The portal is a different app.
5. **Nav collapse breakpoint `md` → `lg`**: measured, the inline nav needed
   1011px before this task and 1114px after, so links overlapped from 900px
   (existing bug) up to 1114px (common 1024px). Below 1200px the nav is now
   the hamburger drawer.

## Issues Encountered

- **Misnamed commit on develop (`2f24beec`)**: promotion stashed another
  session's uncommitted 0519 work in the shared main checkout and switched it
  to develop. That session's `git commit` then recorded only this task's
  backlog → active rename, under the message
  `feat(lore-0519): make the /api SPA public, keep the auth KVS`, and the
  promotion push carried it to origin. Content is correct (the rename was
  intended); only the message is wrong. The 0519 work was intact in a
  worktree. Lesson: do task work in a separate worktree, never switch a
  checkout another session may be using.
