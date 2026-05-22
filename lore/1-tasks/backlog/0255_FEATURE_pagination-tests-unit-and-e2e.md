---
id: '0255'
title: 'Pagination tests — unit (`useCursorPagination`, `usePageHandlers`) + Playwright e2e for 13 paginated pages'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0238', '0254', '0226']
tags: [priority-medium, effort-medium, layer-frontend, phase-future]
milestone: 2
links: []
history:
  - date: 2026-05-22
    who: karolkow
    status: backlog
    note: >
      Spawned from 0238 Future Work. Adds the test rigor that 0238
      explicitly deferred (manual QA only). Unit tests are blocked
      on 0226 (vitest infra in libs/ui). Playwright CLI can land
      independently.
---

# Pagination tests — unit + e2e

## Summary

Add the test coverage that task 0238 (URL-cursor pagination migration)
deferred. Two tracks:

1. **Unit tests** for the new libs/ui pagination primitives —
   `useCursorPagination`, `usePageHandlers`, `useTableUrlState`
   (cursorParam). Blocked on **0226** (vitest infra for libs/ui).
2. **Playwright CLI smoke** covering the 13 paginated pages end-to-
   end against a dev server. Not blocked — can land immediately.

Both together close the AC gap in 0238 ("Manual QA on all 11+ pages
— deferred") and protect the URL-cursor pattern from silent
regression as new pages adopt it.

## Status: Backlog

Track 1 (unit) is blocked on 0226. Track 2 (e2e) is ready to start
any time.

## Context

Task 0238 migrated 13 paginated pages from `useInfiniteQuery +
useState pageIndex + useInfinitePager` to URL-cursor `useQuery +
useCursorPagination`. New libs/ui primitives:

- `useCursorPagination({ filterKeys, cursorParam, resetKey })` —
  URL `?cursor=` reader/writer with prev-cursor stack + `resetKey`
  scope identifier + `MAX_HISTORY = 50` cap.
- `usePageHandlers(page, goNext)` — derives `canNext` + `handleNext`
  from the API response's `page` field.
- `useTableUrlState` — gained `cursorParam` option for multi-section
  routes (LP detail, contract tabs).

These ship with zero unit tests. Task 0238 acknowledged the gap
("`libs/ui` has no test infra (0226)") and accepted manual QA as
the merge gate. That gate was further deferred to PR review with
the migration landing as PR #211.

Risk: any future change to the primitives (e.g. when 0254 lands
backend `prev_cursor` and the in-memory stack goes away) can
silently regress every paginated page. Tests are the antidote.

## Implementation Plan

### Track 1 — unit tests (blocked on 0226)

Add `vitest` + `@testing-library/react` once 0226 lands.

Test files:

- `libs/ui/src/table/useCursorPagination.test.ts`
- `libs/ui/src/table/usePageHandlers.test.ts`
- `libs/ui/src/table/useTableUrlState.test.ts` (cursorParam path)

Cases per primitive:

**`useCursorPagination`**:
- Mount with no URL cursor → `cursor: null`, `canPrev: false`.
- Mount with pasted `?cursor=ABC` deep link + no `resetKey` change
  → cursor preserved on first render (regression test for the
  `useRef` skip-initial-mount fix).
- `goNext("ABC")` → URL updates, stack pushed.
- `goPrev()` after `goNext` → URL reverts, stack popped.
- `goPrev()` with empty stack → no-op.
- `setFilter("q", "abc")` → stack cleared, cursor dropped from URL.
- `resetKey` flip (`poolId: "A"` → `"B"`) → stack cleared, cursor
  dropped.
- `resetKey` set but not flipped across re-renders → no reset.
- `MAX_HISTORY = 50` cap → 51st `goNext` truncates oldest entry.
- `cursorParam: "cursor_p"` → reads / writes the namespaced key,
  default `cursor` untouched.

**`usePageHandlers`**:
- `page === undefined` → `canNext: false`, `handleNext` no-op.
- `page.has_more === true`, `page.cursor === "X"` → `canNext: true`,
  `handleNext()` calls `goNext("X")`.
- `page.has_more === true`, `page.cursor === null` → `canNext:
  false` (backend contract violation, but FE degrades gracefully).
- `page.has_more === false` → `canNext: false` regardless of cursor.

**`useTableUrlState`** (cursorParam new path):
- Two hooks on the same route, `cursorParam: "cursor_p"` and
  `cursorParam: "cursor_t"` → independent URL keys, no collision.
- `setSort` clears the namespaced cursor only, not the other.
- `setFilter` clears the namespaced cursor only, not the other.

### Track 2 — Playwright CLI smoke (not blocked)

Per the team's `[[feedback_playwright_mcp_vs_cli]]`: MCP for
exploration, CLI for regression / CI. This track is CLI.

Add a Playwright spec file (likely under `web/e2e/` or wherever the
project keeps Playwright; check existing infra first).

Scenarios per paginated route:

| Route | Scenarios |
|-------|-----------|
| `/ledgers` | Next 3×, Prev 3×, refresh on N=2, share link → new context opens same page |
| `/ledgers/:sequence` | Inner Next 2×, prev/next ledger nav resets cursor |
| `/transactions` | Filter change resets cursor, refresh on filtered cursor, lowercase op normalized |
| `/assets` | Next + Prev, refresh, share link |
| `/assets/:id` | Switch asset → cursor drops |
| `/nfts` | Next + Prev, refresh |
| `/nfts/:id` | Switch NFT → cursor drops |
| `/accounts/:id` | Switch account → cursor drops |
| `/liquidity-pools` | Next + Prev, filter change resets |
| `/liquidity-pools/:id` | Both `?cursor_p=` and `?cursor_t=` independent; switch pool → both drop |
| `/contracts/:id` | Tab switch between Events / Invocations; `?cursor_e=` and `?cursor_i=` namespaced; switch contract → both drop |

Common assertions per scenario:
- URL contains expected cursor key.
- Page count of rendered rows matches API `data.length`.
- Refresh preserves URL state.
- No console errors.

### Track 3 — wire into CI

Add the Playwright job to `.github/workflows/` (likely extends the
existing TypeScript pipeline). Gate on the same `Detect changes`
filter as the TS job so it skips for backend-only PRs.

## Acceptance Criteria

- [ ] Track 1 — once 0226 lands: vitest specs for
      `useCursorPagination`, `usePageHandlers`, `useTableUrlState`
      cover the cases listed above. Coverage target: 100% of the
      three files' exported surface.
- [ ] Track 2: Playwright CLI spec for all 13 paginated routes, run
      green locally against `npx nx serve web` + a backend dev
      server.
- [ ] Track 3: CI job runs Playwright on every PR touching `web/`
      or `libs/ui/`, fails the PR on any scenario regression.
- [ ] Docs updated: `docs/architecture/frontend/frontend-overview.md`
      pagination section references the test suite as the regression
      net.

## Dependencies

- **0226** (libs/ui vitest test infra) — blocks Track 1.
- **0254** (backend `prev_cursor`) — when it lands, the unit tests
  for the in-memory prev-stack go away and are replaced by tests
  that assert `prev_cursor` is read from `data.page`.

## Reused (no new infra)

- `vitest` + `@testing-library/react` (once 0226 ships).
- Playwright — verify project already has it under `node_modules`
  before planning install (per the team's CLI preference).
- Existing dev-server setup (`pnpm nx serve web`).

## Notes

- Task 0238 final AC: "Manual QA on all 11+ pages (deferred —
  Playwright dev server smoke not run in worktree; gate moved to PR
  review / 0226 vitest infra follow-up)" — this task closes that
  deferral.
- Memory profile sanity (per-cursor cache + `gcTime`) — fold into
  Playwright spec: open 10+ cursors, assert `gcTime` evicts old
  entries (or skip if hard to assert deterministically).
