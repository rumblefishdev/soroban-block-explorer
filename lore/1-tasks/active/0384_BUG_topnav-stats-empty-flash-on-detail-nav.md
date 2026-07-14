---
id: '0384'
title: 'BUG: list→detail navigation is a full page reload (plain <a> row links), flashing the TopNav stats bar empty'
type: BUG
status: active
related_adr: []
related_tasks: []
tags: [frontend, ux, perf, layer-frontend, priority-medium, effort-small]
links:
  - 'libs/ui/src/identifiers/IdentifierDisplay.tsx'
  - 'libs/ui/src/identifiers/IdentifierWithCopy.tsx'
  - 'web/src/pages/accounts/AccountsTable.tsx'
  - 'web/src/router/AppShell.tsx'
history:
  - date: 2026-07-13
    status: backlog
    who: stkrolikiewicz
    note: >
      Reported from manual use: upper stats bar stays stable on list→list but
      "re-renders and refetches from scratch, showing empty stats first" when
      entering a detail page.
  - date: 2026-07-14
    status: backlog
    who: stkrolikiewicz
    note: >
      ROOT CAUSE CONFIRMED via local repro (web:serve + dev proxy). List→detail
      row links render plain `<a href>` (IdentifierDisplay component="a"), so the
      click is a FULL PAGE RELOAD, not SPA routing — the whole app remounts and
      useNetworkStats refetches from zero. list→list uses SecondaryNav
      navigate() (SPA) so it stays. Replaced the earlier hypotheses with the
      confirmed cause + fix.
  - date: 2026-07-14
    status: active
    who: stkrolikiewicz
    note: 'Activated to implement the fix (plain <a> → React Router Link).'
---

# BUG: list→detail is a full page reload, flashing the TopNav stats bar empty

## Summary

The upper stats bar (`TopNav` — TPS / Ledger / Accounts / Contracts + global
search) stays stable when navigating between **list** pages, but flashes empty
(counters roll up from 0) when entering a **detail** page. Confirmed cause: the
list-row identifier links are plain `<a href>` anchors, so clicking one does a
**full browser reload** instead of SPA client-side routing. The reload remounts
the entire app — `TopNav` re-mounts and `useNetworkStats` refetches from scratch,
hence the empty flash. Navigation between lists uses React Router `navigate()`
(SPA), so it doesn't reload and the bar stays put.

## Root Cause (CONFIRMED — 2026-07-14)

Live repro on `nx run web:serve` (dev proxy → prod API), instrumented in-page:

- Navigating list↔list via SecondaryNav preserved page state (`window.__*`
  survived; `performance.now()` kept climbing) and produced **zero** TopNav
  mount/unmount events → **SPA, no remount**. SecondaryNav calls
  `navigate(item.href)` in [AppShell](../../../web/src/router/AppShell.tsx).
- Clicking a **list-row link** (Ledgers list → a ledger) **reset the document**:
  `performance.now()` dropped (115033 ms → 48448 ms), the Navigation Timing
  entry was `type: "navigate"` (fresh load), and all injected `window` state was
  gone → **full page reload**.

Code: the shared identifier-link primitive renders a plain anchor —
[`IdentifierDisplay.tsx:130`](../../../libs/ui/src/identifiers/IdentifierDisplay.tsx)
`component={linked ? 'a' : 'span'}` with `href={getIdentifierHref(...)}`. React
Router does **not** intercept plain `<a>` clicks, so each is a hard navigation.
Every list table routes its row links through `IdentifierDisplay` /
`IdentifierWithCopy`; `AccountsTable.tsx:34` adds its own `component="a"` too.
(`PoolsTable` already wraps leg codes in `RouterLink` conditionally — precedent
for the fix.)

## Impact / Scope

Bigger than the stats bar: **every list→detail navigation is a full page
reload** — throws away the TanStack Query cache, re-downloads the bundle, resets
scroll, and white-flashes. The empty stats bar is just the most visible symptom.
Fixing the shared primitive fixes the whole class.

## Fix (pick one; keep it minimal)

- **Preferred:** make `IdentifierDisplay` / `IdentifierWithCopy` router-aware —
  render the linked variant via React Router `Link` instead of a bare `<a>`.
  `libs/ui` is router-agnostic, so thread a `LinkComponent` prop (default `'a'`)
  and pass `react-router-dom`'s `Link` from the web app (matches the existing
  `component={RouterLink}` usage in `web/src/pages/**`). Remove the extra
  `component="a"` in `AccountsTable`.
- **Smallest diff (alt):** one app-level same-origin `<a>` click interceptor at
  the shell root that calls `navigate()` and `preventDefault()` — fixes all
  plain internal anchors at once, but is less explicit than per-primitive Links.

## Related (not blocking)

Archived audit [0257] finding `F-W6-E0-5`: the header stats strip and the home
page both poll `/network/stats` under distinct query keys → 2× requests. Same
component, distinct issue; fold into the same cleanup if this touches
`TopNav`/`useNetworkStats`.

## Acceptance Criteria

- [ ] list→detail navigates client-side (no full reload) — verify
      `performance.now()` keeps climbing and the injected-state / Navigation
      Timing check no longer shows a `navigate` document load.
- [ ] TopNav stats bar no longer flashes empty entering a detail page.
- [ ] No regression to list→list, copy buttons, or open-in-new-tab / middle-click
      on identifier links (a real `<a href>` must remain for those).
- [ ] **Docs updated** — N/A: frontend routing/render fix, no ADR, does not
      change the shape of the system (schema/API/pipeline/topology).
- [ ] **API types regenerated** — N/A: no change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.
