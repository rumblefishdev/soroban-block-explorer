---
id: '0577'
title: 'FEATURE: Soroban Scan privacy policy page + footer link'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0437', '0451', '0384']
tags: [frontend, footer, privacy, priority-medium, effort-small]
links: []
history:
  - date: '2026-09-23'
    status: backlog
    who: stkrolikiewicz
    note: >
      Created from a chat request: publish the explorer's own privacy
      policy, text supplied in chat (dated September 23, 2026). No existing
      task covered it (searched backlog/active/blocked on develop and all
      remote branches; 0437 and 0451 are archived).
  - date: '2026-09-23'
    status: active
    who: stkrolikiewicz
    note: 'Promoted to active; implementation starting.'
  - date: '2026-09-23'
    status: active
    who: stkrolikiewicz
    note: >
      Implemented: `/privacy-policy` page with the text verbatim (static
      JSX), one route constant shared by the router and the footer, footer
      link through the router in the same tab. 1 new test, mutation-checked.
      Open until the SPA deploy.
---

# FEATURE: Soroban Scan privacy policy page + footer link

## Summary

The footer's `Privacy Policy` link points at the generic rumblefish.dev
policy. Publish the explorer's own policy as an in-app page at
`/privacy-policy` and point the footer link at it.

## Status: Active

**Current state:** implemented, PR open. Complete after the SPA deploy.

## Context

- The text was supplied in chat as Markdown, dated September 23, 2026. It
  is published verbatim. Wording changes belong to its author, not to this
  task.
- Footer: `libs/ui/src/layout/Footer.tsx`. `Privacy Policy` sits in
  `RESOURCES`, whose links open in a new tab (task 0437). `Cookie Settings`
  next to it re-opens the HubSpot banner (task 0451).
- `libs/ui` cannot import the router. Its `href`-based links reach
  react-router through `useLinkComponent` (task 0384), which keeps
  navigation client-side.
- Routes: `web/src/router/index.tsx`, one lazy page module per route under
  `web/src/pages/`.

## Implementation Plan

### Step 1: Page

`web/src/pages/PrivacyPolicyPage.tsx`: the text as static JSX. No Markdown
dependency for one page. Lazy-loaded like every other page, so the text
stays out of the main bundle.

### Step 2: Route

`PRIVACY_POLICY_URL = '/privacy-policy'` in `libs/ui/src/layout/links.ts`,
next to `PRICES_API_URL`. The router and the footer both read it, so a
rename cannot leave the footer pointing at a 404.

### Step 3: Footer

Point `Privacy Policy` at the route. It opens in the same tab through
`useLinkComponent`, not in a new tab like the external links.

### Step 4: Tests and docs

Extend `Footer.test.tsx`: the link targets `/privacy-policy` and opens in
the same tab. Add the route to the route inventory in
`docs/architecture/frontend/frontend-overview.md`.

## Acceptance Criteria

- [x] `/privacy-policy` renders the supplied text verbatim inside the app
      shell, in both themes and at phone width
- [x] Footer `Privacy Policy` opens it in the same tab, without a page
      reload
- [x] FE typecheck / lint / tests green — 16 test files in `libs/ui`, 45 in
      `web`
- [x] **Docs updated** — `docs/architecture/frontend/frontend-overview.md`
      route inventory gains `/privacy-policy`.
- [x] **API types regenerated** — N/A — no `crates/api/**`, `Cargo.*` or
      `libs/api-types/**` changes.

## Implementation Notes

- New: `web/src/pages/PrivacyPolicyPage.tsx` (lazy route module).
- Changed: `web/src/router/index.tsx`, `libs/ui/src/layout/links.ts`,
  `Footer.tsx`, `layout/index.ts`, `libs/ui/src/index.ts`, the footer
  test, and the docs route inventory.
- Verbatim check: the rendered `<main>` text, whitespace-normalised, has
  the same SHA-256 as the supplied Markdown with its syntax stripped
  (13,775 characters each).
- Checked in the dev server: footer click navigates client-side (no
  reload), a deep link loads the page, dark and light themes, 375px wide
  without horizontal overflow.

## Design Decisions

### From Plan

1. **Static JSX, no Markdown dependency** — one page does not justify a
   parser and its dependency tree. A new version of the text is a text edit.

2. **`PRIVACY_POLICY_URL` in `libs/ui`** — the footer lives in `libs/ui`
   and cannot import app routes, so the lib owns the path, like
   `PRICES_API_URL`.

### Emerged

3. **`internal` flag on `FooterNavItem`** — a relative `href` cannot mean
   "route of this app": `/api/` is relative too, but it is a separate SPA
   that needs a full page load. The flag picks the router link and drops
   `target="_blank"` for that item only.

4. **Semantic HTML styled from one `sx`** — headings, paragraphs and lists
   are plain elements styled by selector on the card, not one
   `Typography` per paragraph.

5. **Existing `PageHeader`** — the title and the "Last updated" line become
   the page's `h1` and subtitle, like every other page. Same words.

6. **The email address is a `mailto:` link** — additive, the text is
   unchanged.

7. **Absolute child path** — `'/privacy-policy'` is valid under the `/`
   parent route, so the router uses the shared constant as-is.

## Issues Encountered

- **`web:typecheck` red in the fresh worktree**, on MUI palette
  augmentation in files this task does not touch: `libs/ui/dist` held
  `.d.ts.map` files without their `.d.ts`. Moved `dist` to the main
  checkout's `.trash/`, ran `nx reset`, re-ran serially: green. Not caused
  by this change.

**Modified tests:** `Footer.test.tsx` gains one case. No existing
assertion changed.
