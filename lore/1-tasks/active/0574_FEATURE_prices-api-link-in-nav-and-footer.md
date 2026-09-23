---
id: '0574'
title: 'FEATURE: link to the Prices API from the navbar and the footer'
type: FEATURE
status: backlog
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
---

# FEATURE: link to the Prices API from the navbar and the footer

## Summary

The explorer does not link to the Prices API anywhere. Add one link in the
top navigation and one in the footer, so users can find it from any page.

## Status: Backlog

**Current state:** not started. The target URL is not decided yet (see Open
questions).

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
  distribution. Its basic-auth gate is controlled by
  `enableApiSpaBasicAuth`. If that SPA is where the Prices API lives, the
  link points at `/api/`. It is not a client route of this app, so it must
  be a plain `<a>` that does a full page load, not a router link.

## Implementation Plan

### Step 1: Pin the URL

Decide the destination (see Open questions). Keep it in one constant shared
by the nav and the footer.

### Step 2: Navbar

Add a `Prices API` external link next to `Report a bug` in `SecondaryNav`,
styled the same way. Check both the inline nav and the drawer below the
`md` breakpoint.

### Step 3: Footer

Add `{ label: 'Prices API', href: … }` to `RESOURCES` in `Footer.tsx`.

### Step 4: Tests

Extend `Footer.test.tsx` to assert the link and its `href`. `SecondaryNav`
has no tests yet. Add `__tests__/SecondaryNav.test.tsx` (per the
CLAUDE.md test-placement rule) with one assertion for the new link.

## Acceptance Criteria

- [ ] Navbar shows a `Prices API` link, inline and in the mobile drawer
- [ ] Footer shows a `Prices API` link under Resources
- [ ] Both links point at the same URL, defined once
- [ ] The link does a full page load (`<a href>`), not client-side routing
- [ ] Not shipped while the target is still behind basic auth. If it is
      the 0519 `/api/*` SPA, this waits until `enableApiSpaBasicAuth` is off
      in production
- [ ] **Docs updated** — N/A — a UI link does not change any frontend data
      contract or the system shape described in `docs/architecture/**`.
- [ ] **API types regenerated** — N/A — no `crates/api/**`, `Cargo.*` or
      `libs/api-types/**` changes.

## Open questions

- What is the target URL: the 0519 `/api/` SPA on this domain, or a
  separate Prices API host or docs page?
- Open in a new tab (external, like `Report a bug`) or the same tab (same
  domain)?
- Label: `Prices API` or `API`?
