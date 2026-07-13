---
id: '0384'
title: 'BUG: TopNav network-stats bar flashes empty (rolls from 0) on some navigations, inconsistent with list→list'
type: BUG
status: backlog
related_adr: []
related_tasks: []
tags: [frontend, ux, layer-frontend, priority-low, effort-small]
links:
  - 'web/src/router/AppShell.tsx'
  - 'libs/ui/src/layout/TopNav.tsx'
  - 'libs/ui/src/format/AnimatedNumber.tsx'
history:
  - date: 2026-07-13
    status: backlog
    who: stkrolikiewicz
    note: >
      Reported from manual use: the upper stats bar (with global search) stays
      stable when navigating list→list, but "re-renders and refetches from
      scratch, showing empty stats first" when entering a detail page. Filed as
      a follow-up. Root cause NOT yet confirmed on a running app — static trace
      contradicts the list→detail framing (see Static Analysis); needs the repro
      checklist to disambiguate before a fix is chosen.
---

# BUG: TopNav network-stats bar flashes empty on some navigations

## Summary

The upper stats bar (`TopNav` — TPS / Ledger / Accounts / Contracts counters +
global search) is reported to stay stable when moving between **list** pages
(desired), but to "re-render and refetch from scratch, showing empty stats
first" when **entering a detail** page. The empty-then-fill flash is
inconsistent with the list-view behavior, which is already correct.

## Context

`TopNav` lives in `libs/ui` and is purely presentational — it takes
`stats?: NetworkStats` and renders `—` (dash) for any `undefined` field, then
`AnimatedNumber` (`@number-flow/react`) odometer-rolls the digits. The data
comes from `useNetworkStats()` (stable, param-free query key) called once in
`AppShell`, the single layout route wrapping every page via `<Outlet/>`.

## Static Analysis (confirmed from code — 2026-07-13)

This is the important part: **list→detail should be structurally identical to
list→list for `TopNav`.** From the code, navigating between any two non-home
routes cannot blank the bar:

- `AppShell` is the layout route at `path: '/'`; list **and** detail pages are
  its children ([router index.tsx](../router/index.tsx) → all under one
  `<AppShell/>`). React Router keeps the layout element mounted across sibling
  navigations → `useNetworkStats()` stays subscribed and `TopNav` stays mounted.
- Query key is param-free (`getNetworkStatsOptions()`), so it does not change per
  route. `livePolicy` sets only `staleTime: 4s` (no `gcTime` → default 5-min
  retention). `invalidateResource('network')` is **exported but never called**.
  ⇒ no cache eviction / background-only refetch on navigation; cached data keeps
  rendering — no blank.
- `TopNav` is gated by `{!isHome && <TopNav/>}` in
  [AppShell.tsx](../../../web/src/router/AppShell.tsx) — it mounts/unmounts
  **only** when crossing the home boundary.
- On a _fresh_ mount, `AnimatedNumber` rolls its counters in from empty/zero —
  which is exactly the reported "empty stats at first" symptom.
- `StrictMode` is ON (`web/src/main.tsx`) → in dev, components double-mount,
  which can add a spurious flash that does NOT exist in a production build.

**Conclusion:** the reported list→detail-specific blank is not explained by the
shell/query wiring. The real trigger is almost certainly one of the hypotheses
below, and the "list→detail" framing needs on-app confirmation.

## Hypotheses (ranked — confirm before fixing)

1. **Trigger is `home→detail`, not `list→detail`.** Crossing the home boundary
   mounts `TopNav` fresh (it's hidden on home), so NumberFlow rolls in from
   empty. Most consistent with the code. _Confirm:_ go list→detail with no home
   in between → predict NO blank; go home→detail → predict the blank.
2. **A detail page throws during render / lazy-load**, bubbling to the root
   `errorElement` (`RouteErrorBoundary`), which replaces the **entire**
   `AppShell` (incl. `TopNav`); on recovery/next nav the shell remounts fresh →
   blank + refetch. Detail-specific if particular detail routes error.
   _Confirm:_ watch the console during the exact nav that blanks.
3. **Dev-only StrictMode / HMR artifact.** Observation may have been under
   `npm run dev`. _Confirm:_ repro on `vite preview` (prod build).

## Repro Checklist (do this first — ~5 min on a running app)

Needs a running web app with real stats (dev proxy → prod API requires
`DEV_API_KEY`; the worktree also needs `node_modules` provisioned).

- [ ] list→list (e.g. Transactions list → Ledgers list): confirm bar stays put.
- [ ] list→detail **without touching home** (row click on a list): does it blank?
- [ ] home→detail and home→list: does it blank? (isolates H1)
- [ ] Open devtools console/network during the blanking nav (isolates H2).
- [ ] Repeat on a prod build via `vite preview` (isolates H3).
- [ ] Record which navigation(s) actually blank → pick the matching fix below.

## Candidate Fixes (pick after repro)

- **H1:** keep `TopNav` always mounted; hide on home via CSS/visibility instead
  of unmounting (drop the `!isHome` unmount), or seed NumberFlow's initial value
  so it doesn't roll from empty.
- **H2:** give detail routes a nested `errorElement` (inside `<Outlet/>`) so a
  child error can't tear down the whole shell — worth doing regardless of this
  bug.
- **H3:** no code fix (dev-only); note in the task and close.

## Related (not blocking)

Archived audit [0257] finding `F-W6-E0-5` / `F-I-3`: the header stats strip and
the home page both poll `/network/stats` under distinct query keys → 2× requests
every ~12s. Same component + data path, distinct issue; could be folded into the
same cleanup if this task touches `TopNav`/`useNetworkStats`.

## Acceptance Criteria

- [ ] Repro checklist run; the actual triggering navigation(s) documented.
- [ ] Stats bar no longer flashes empty on that navigation (or H3 confirmed
      dev-only and documented).
- [ ] No regression to the correct list→list behavior.
- [ ] **Docs updated** — N/A: frontend render/lifecycle fix, no ADR, does not
      change the shape of the system (schema/API/pipeline/topology).
- [ ] **API types regenerated** — N/A: no change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.
