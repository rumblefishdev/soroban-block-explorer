---
id: '0226'
title: 'Frontend: Vitest + Testing Library test infrastructure (libs/ui + web pages)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0061', '0069', '0073', '0074']
tags: [priority-medium, effort-medium, layer-frontend-shared, phase-future]
milestone: 2
links: []
history:
  - date: 2026-05-15
    status: backlog
    who: karolkow
    note: 'Spawned from 0061 future work — libs/ui has no test infra.'
  - date: 2026-05-18
    status: backlog
    who: karolkow
    note: 'Scope extended to cover web/ app test infra + page tests for the 0069/0073/0074 explorer pages, which shipped without tests by decision.'
---

# Frontend: Vitest + Testing Library test infrastructure (libs/ui + web pages)

## Summary

Neither `libs/ui` nor the `web` app has a unit-test setup — no Vitest
config, no `test` target, no jsdom, no `@testing-library/react`. Stand
up the test infrastructure for both and add the first tests: the task
0061 table primitives in `libs/ui`, and the explorer pages in `web`
(transactions 0069, account detail 0073, assets list + detail 0074)
which shipped without coverage by decision.

## Context

Task 0061 delivered `ExplorerTable`, `PaginationControls`,
`TableSectionHeader`, `TableEmptyState`, `useTableUrlState`, and
`useCursorPagination` in `libs/ui/src/table/`. Behaviour was verified
manually via the `/table-playground` route, but there is no automated
coverage. The workspace already depends on `vitest`, `@vitest/coverage-v8`,
and `@nx/vitest`; what is missing is the jsdom env, React Testing Library,
a per-project Vitest config, and an Nx `test` target on `libs/ui`.

## Implementation

- Add dev deps: `jsdom`, `@testing-library/react`, `@testing-library/dom`,
  `@testing-library/jest-dom`, `@testing-library/user-event`.
- Add `libs/ui/vitest.config.ts` (jsdom environment, setup file) and a
  `src/test-setup.ts` registering `jest-dom` matchers.
- Wire an Nx `test` target for `@rumblefish/soroban-block-explorer-ui`
  (via `@nx/vitest` plugin or explicit target).
- First tests:
  - `ExplorerTable` — renders semantic `<table>/<thead>/<th>/<td>`,
    accepts typed columns, sort header click fires `onSortChange` with
    toggled direction, empty `rows` renders `emptyState`.
  - `PaginationControls` — `Previous` disabled when `prevCursor` null,
    `Next` disabled when `nextCursor` null, click fires callback with
    the cursor.
  - `useTableUrlState` — cursor/sort/filter round-trip through URL params
    (render inside `MemoryRouter`); `setSort` and `setFilter` drop the
    `cursor` param.
- `web` app test setup: `web/vitest.config.ts` (jsdom + setup file), an
  Nx `test` target for `@rumblefish/soroban-block-explorer-web`, and a
  render helper wrapping `QueryProvider` + `MemoryRouter` +
  `ExplorerThemeProvider`.
- Page tests for the explorer pages that shipped without coverage
  (tasks 0069, 0073, 0074):
  - Account detail (0073) — summary/balances render, per-section error
    isolation, 404 on a malformed `G…` id, balance → `/assets/:id` link.
  - Assets list (0074) — type-chip + code-search filters drive the
    query, cursor pagination, empty vs filtered-empty states.
  - Asset detail (0074) — summary, partial-metadata tolerance, type
    badge per asset class.
  - Pure helpers — `formatAmount`, `assetTypeMeta`, `formatFee`.
- Confirm CI picks up the new `test` targets (affected runs).

## Acceptance Criteria

- [ ] `libs/ui` has a Vitest config with jsdom + Testing Library setup
- [ ] Nx `test` target runs for `@rumblefish/soroban-block-explorer-ui`
- [ ] ExplorerTable, PaginationControls, useTableUrlState covered
- [ ] `web` has a Vitest config + Nx `test` target with a shared render
      helper (QueryProvider + router + theme)
- [ ] Account, assets-list, and asset-detail pages have query + render
      coverage; `formatAmount` / `assetTypeMeta` unit-tested
- [ ] `nx run-many -t test` green for both projects
- [ ] CI affected runs include the new targets

## Notes

- These are the first test targets in the frontend; the chosen layout
  becomes the pattern for sibling libs and the app.
