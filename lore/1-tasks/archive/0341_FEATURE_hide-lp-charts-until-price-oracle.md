---
id: '0341'
title: 'Hide LP-detail charts card (TVL/Fee/Volume) behind flag until price-oracle lands'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0199', '0215']
tags: [layer-frontend, priority-low, effort-small]
milestone: 2
links: []
history:
  - date: '2026-07-02'
    status: active
    who: stkrolikiewicz
    note: 'Task created — gate PoolCharts behind a build-time flag until 0199/0215 ship real series.'
  - date: '2026-07-02'
    status: completed
    who: stkrolikiewicz
    note: >
      Shipped `CHARTS_ENABLED = false` in PoolCharts.tsx (early `return null`).
      1 file changed (+ task doc). Pre-commit lint + typecheck + 104 web tests
      green; pushed direct to develop (835aca67, fast-forward). Re-enable is
      future work under 0199/0215.
---

# Hide LP-detail charts card (TVL/Fee/Volume) behind flag until price-oracle lands

## Summary

Add a build-time flag that hides the whole `PoolCharts` card on the liquidity
pool detail page until the price-oracle API ships (tasks 0199 + 0215). All three
metric series (`tvl` / `volume` / `fee_revenue`) come back NULL today, so the
card only ever renders a "pending" placeholder — hiding it removes dead UI until
the data exists.

## Status: Completed

**Final state:** `CHARTS_ENABLED = false` in
`web/src/pages/pool-detail/PoolCharts.tsx`; `PoolCharts` early-returns `null`,
so the card never renders, `LazySection` never mounts, and `usePoolChart` never
fires. Shipped to `develop` (835aca67). Re-enable (flip flag + drop the
pending-oracle placeholder) is future work under 0199/0215, not this task.

## Context

`web/src/pages/pool-detail/PoolCharts.tsx` currently renders an inline
"Chart data not yet available — pending the price-oracle integration (task
0199)" placeholder, because the chart endpoint returns null for every series
(blocked on the team price oracle per 0199/0215; endpoint ownership per ADR
0043). We want the card removed entirely, not placeholdered, until real data
lands.

## Implementation Plan

### Step 1: Build-time flag

Add `const CHARTS_ENABLED = false;` in `PoolCharts.tsx` and early-`return null`
from `PoolCharts` when it's off. Returning null means `LazySection` never
mounts and `usePoolChart` never fires.

### Step 2: Re-enable (future, not this task)

When 0199/0215 deliver non-null series, flip the flag to `true` and drop the
now-dead const + pending-oracle placeholder branch.

## Acceptance Criteria

- [x] Charts card does not render on the LP detail page while the flag is off
- [x] No chart API request fires (`usePoolChart` not called)
- [ ] **Docs updated** — N/A: frontend-only visibility toggle; no schema,
      endpoint, pipeline, or data-contract change (endpoint still exists, just
      not called).
- [ ] **API types regenerated** — N/A: no change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Implementation Notes

- Single-file behavioural change: `web/src/pages/pool-detail/PoolCharts.tsx`.
  Added module const `CHARTS_ENABLED = false` + `if (!CHARTS_ENABLED) return
null;` as the first line of the exported `PoolCharts`. Docblocks updated to
  explain the kill-switch and point re-enable at 0199/0215.
- The call site (`LiquidityPoolDetailPage.tsx`) is unchanged: `PoolCharts`
  returning `null` leaves an empty `SectionErrorBoundary sectionName="pool-charts"`
  wrapper, which renders nothing — a harmless no-op, not worth a second edit.
- Verified with `nx run-many -t lint typecheck` + the pre-commit hook's full
  web suite (104 tests). No visual/browser check: the deployed API is armed and
  returns 401 without a `DEV_API_KEY`, so keyed preview data wouldn't load
  (see Issues) — behaviour is guaranteed structurally by the early return.

## Design Decisions

### From Plan

1. **Build-time const, not env/runtime flag.** Re-enabling is coupled to the
   0199/0215 code (dropping the placeholder branch), so a `const` flipped in the
   same future PR is more honest than a `VITE_` runtime toggle nobody would set.

### Emerged

2. **Inferred literal `= false`, not `: boolean`.** First tried
   `const CHARTS_ENABLED: boolean = false` to dodge an always-true/unreachable
   lint on the guard — but this repo's ESLint enables
   `@typescript-eslint/no-inferrable-types` (errors on the annotation) and does
   NOT enable `no-unnecessary-condition` (the guard lints clean). So the plain
   inferred literal is both lazier and the only one that passes.
3. **Flag lives inside `PoolCharts`, not at the page call site.** Self-contained
   single-file diff; the empty error-boundary wrapper it leaves behind is inert.
4. **Pushed direct to develop (no PR)** per explicit request, deviating from the
   usual "code changes go via PR" convention. Landed via
   `rebase --onto origin/develop bb1ca19d` so only this commit replayed onto the
   current develop tip (the worktree's pre-existing 0339 commit was excluded).

## Issues Encountered

- **Port 4200 taken + hardcoded in `vite.config.ts`.** Another worktree's dev
  server held 4200 and the shared config hardcodes it. Ran this instance on 4210
  via `vite web --port 4210 --strictPort` with a matching
  `VITE_API_BASE_URL=http://localhost:4210` so the dev proxy stayed same-origin.
  Config left untouched. Local-only files: `.claude/launch.json` (untracked),
  `web/.env.development` (gitignored).
- **Deployed API armed (401).** `api-sorobanscan.rumblefishdev.com` requires a
  dev `x-api-key`; the `DEV_API_KEY` slot was empty and no key was on disk. The
  other documented hosts don't resolve. Result: no data-backed browser preview;
  verification fell back to lint/typecheck/tests + code inspection.

## Future Work

- Re-enable the card (flip `CHARTS_ENABLED` → `true`, delete the const + the
  pending-oracle placeholder branch). Covered by 0199/0215 delivering non-null
  series — no separate backlog task spawned.

## Notes

Re-enable is a one-line flag flip, coupled to 0199/0215 delivering non-null
series. Related: 0199 (LP analytics, blocked-on-oracle), 0215 (FE-impact
catalog of the blocked endpoint), ADR 0043 (Lambda 2 owns USD-denominated
fields).
