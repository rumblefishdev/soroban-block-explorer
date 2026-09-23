---
id: '0577'
title: 'FEATURE: Soroban Scan privacy policy page + footer link'
type: FEATURE
status: backlog
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
---

# FEATURE: Soroban Scan privacy policy page + footer link

## Summary

The footer's `Privacy Policy` link points at the generic rumblefish.dev
policy. Publish the explorer's own policy as an in-app page at
`/privacy-policy` and point the footer link at it.

## Status: Backlog

**Current state:** not started. The text is supplied and final.

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

- [ ] `/privacy-policy` renders the supplied text verbatim inside the app
      shell, in both themes and at phone width
- [ ] Footer `Privacy Policy` opens it in the same tab, without a page
      reload
- [ ] FE typecheck / lint / tests green
- [ ] **Docs updated** — `docs/architecture/frontend/frontend-overview.md`
      route inventory gains `/privacy-policy`.
- [ ] **API types regenerated** — N/A — no `crates/api/**`, `Cargo.*` or
      `libs/api-types/**` changes.
