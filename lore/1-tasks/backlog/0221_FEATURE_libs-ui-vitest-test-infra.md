---
id: '0221'
title: 'libs/ui: Vitest + Testing Library test infrastructure and table primitive tests'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0061']
tags: [priority-medium, effort-small, layer-frontend-shared, phase-future]
milestone: 2
links: []
history:
  - date: 2026-05-15
    status: backlog
    who: karolkow
    note: 'Spawned from 0061 future work — libs/ui has no test infra.'
---

# libs/ui: Vitest + Testing Library test infrastructure and table primitive tests

## Summary

`libs/ui` ships components and hooks but has no unit-test setup — no
Vitest config, no `test` target, no jsdom, no `@testing-library/react`.
Stand up the test infrastructure and add the first component/hook tests,
starting with the task 0061 table primitives.

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
- Confirm CI picks up the new `test` target (affected runs).

## Acceptance Criteria

- [ ] `libs/ui` has a Vitest config with jsdom + Testing Library setup
- [ ] Nx `test` target runs for `@rumblefish/soroban-block-explorer-ui`
- [ ] ExplorerTable, PaginationControls, useTableUrlState covered
- [ ] `nx run @rumblefish/soroban-block-explorer-ui:test` green
- [ ] CI affected runs include the new target

## Notes

- This is the first test target in `libs/`; the chosen layout becomes the
  pattern for sibling libs.
