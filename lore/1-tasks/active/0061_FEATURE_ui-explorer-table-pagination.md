---
id: '0061'
title: 'UI lib: explorer table, pagination controls, cursor pagination adapter'
type: FEATURE
status: active
related_adr: []
related_tasks: []
tags: [priority-high, effort-medium, layer-frontend-shared]
milestone: 2
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-14
    status: active
    who: karolkow
    note: 'Promoted to active — parallel frontend lib work on separate worktree'
  - date: 2026-05-15
    status: active
    who: karolkow
    note: 'Components + hooks landed (631b96c, fbc677b); tests + demo outstanding'
---

# UI lib: explorer table, pagination controls, cursor pagination adapter

## Summary

Implement the core explorer table component, cursor-based pagination controls, and a SectionHeader component in `libs/ui/src/table/`. These are the foundational data display primitives reused across all list pages and detail page sub-sections throughout the explorer.

## Status: Backlog

**Current state:** Not started.

## Context

Every collection view in the explorer (transactions, ledgers, tokens, NFTs, liquidity pools) and every detail page sub-section (transactions in a ledger, invocations of a contract, transfers of an NFT) uses paginated tables. The backend provides cursor-based pagination with opaque cursor tokens. The frontend must never parse or construct cursors -- it only passes them through.

Key design constraints:

- Cursor tokens are opaque -- never parse or construct them
- No total counts available -- only next/previous cursor navigation
- URL state sync: filters, sorting, and cursor stored in URL query params
- Filter change resets cursor
- No hard reloads on filter/sort/page changes
- Semantic HTML: `<table>`, `<thead>`, `<th>`, `<td>` -- not div grids

## Implementation Plan

### Step 1: ExplorerTable component

Create `libs/ui/src/table/ExplorerTable.tsx`:

- Generic, typed table component accepting column definitions and row data
- Renders semantic HTML: `<table>`, `<thead>`, `<tbody>`, `<tr>`, `<th>`, `<td>`
- Supports sortable columns (click header to toggle sort direction)
- Dense but readable row spacing per explorer design philosophy
- Responsive: horizontal scroll on small screens for wide tables

### Step 2: Pagination controls

Create `libs/ui/src/table/PaginationControls.tsx`:

- Previous / Next buttons only (no page numbers, no total count)
- Accepts `prevCursor` and `nextCursor` props (nullable)
- Disables Previous when no prevCursor, disables Next when no nextCursor
- On click, calls navigation callback with the appropriate cursor

### Step 3: Cursor pagination adapter

Create `libs/ui/src/table/useCursorPagination.ts` hook:

- Reads current cursor from URL query params
- Provides `goNext(cursor)` and `goPrev(cursor)` functions that update URL
- Resets cursor when filters change (detects filter param changes)
- Syncs sorting direction to URL params
- No hard page reloads -- uses React Router navigation

### Step 4: SectionHeader component

Create `libs/ui/src/table/SectionHeader.tsx`:

- Renders contextual headers like "Transactions in Ledger #12345", "Recent Transactions", "Contract Invocations"
- Accepts title string and optional count/subtitle
- Consistent typography and spacing across all table sections

### Step 5: URL state sync utilities

Create `libs/ui/src/table/useTableUrlState.ts`:

- Manages filter state in URL query params
- Manages sort state in URL query params
- On filter change: updates URL params, resets cursor to null
- Bidirectional sync: URL changes update component state, component state changes update URL

### Step 6: Exports

Export all table components and hooks from `libs/ui` barrel.

## Acceptance Criteria

- [x] ExplorerTable renders semantic HTML (`<table>`, `<thead>`, `<th>`, `<td>`) — via MUI `Table`/`TableHead`/`TableCell`
- [x] Table accepts generic column definitions and typed row data — `ExplorerTableColumn<T>`, `rows: readonly T[]`
- [x] Sortable columns toggle sort direction on header click — `TableSortLabel`, desc↔asc toggle
- [x] PaginationControls show Previous/Next only, no page numbers or total counts
- [x] Previous disabled when no prevCursor; Next disabled when no nextCursor
- [x] Cursor pagination hook reads/writes cursor to URL query params — `useTableUrlState` / `useCursorPagination`
- [x] Filter changes reset cursor to null — `setFilter`/`setSort` drop `cursor` param
- [x] No hard page reloads on filter, sort, or pagination changes — `useSearchParams` `replace: true`
- [x] SectionHeader renders contextual table section titles — `TableSectionHeader`
- [x] URL state sync works bidirectionally for filters, sorting, and cursor
- [x] Components reusable across all list pages and detail page sub-sections — generic, in `libs/ui`
- [x] All components and hooks exported from `libs/ui` — barrel `table/index.ts` + `libs/ui/src/index.ts`

## Progress

Delivered on `feat/0061_ui-explorer-table-pagination`:

- Commit `631b96c` — visual primitives translated 1:1 from Figma DS
  (`siumLgKOc9LLepEfbimyp3`): `ExplorerTable`, `PaginationControls`,
  `TableSectionHeader`, `TableEmptyState`.
- Commit `fbc677b` — `useTableUrlState` + `useCursorPagination` hooks
  (spec from this task; Figma carries no logic). Adds `react-router-dom`
  peer dep on `libs/ui`.

Verified: `nx` build + typecheck + lint green for `libs/ui` and `web`.

Outstanding:

- Unit tests — `libs/ui` has no Vitest/test-target infra yet; setting it
  up (jsdom + `@testing-library/react` + test target) is a prerequisite.
- Runtime demo — wire into a `web` list page to confirm 1:1 visual parity
  in-browser.

### Docs updated (ADR 0032)

- `docs/architecture/frontend/frontend-overview.md` — N/A: already
  describes cursor pagination, URL-held filter/sort/cursor state, and
  list tables (lines 92, 186-188, 234, 261, 318-331). This task
  implements the already-documented contract; system shape unchanged.
- All other architecture docs — N/A: frontend-only presentation
  primitives, no schema / API / pipeline / infra change.

## Notes

- This is one of the most heavily reused components in the frontend. Every list page and most detail pages depend on it.
- The backend pagination contract is defined in the backend overview: opaque cursors, no total counts, deterministic ordering.
- MUI theme from task 0058 provides spacing and typography for dense table rows.
