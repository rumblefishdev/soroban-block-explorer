---
id: '0061'
title: 'UI lib: explorer table, pagination controls, cursor pagination adapter'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0226']
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
  - date: 2026-05-15
    status: active
    who: karolkow
    note: 'Verified in-browser via local demo; reverted playground route (59effa9); spawned 0226'
  - date: 2026-05-15
    status: completed
    who: karolkow
    note: >
      All 12 acceptance criteria met. 6 files in libs/ui/src/table/
      (4 components + 2 hooks) + barrel; react-router-dom peer dep.
      13 commits on feat/0061_ui-explorer-table-pagination. Unit tests
      deferred to 0226 (no libs/ui test infra). Key calls: opaque
      cursors, URL-as-state, sort caret without the DS Active pill,
      theme header uppercase corrected to match Figma.
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

## Implementation Notes

Delivered on `feat/0061_ui-explorer-table-pagination`. New module
`libs/ui/src/table/` (barrel + re-export from `libs/ui`):

- `ExplorerTable.tsx` — generic `ExplorerTable<T>`, semantic MUI
  `Table`, typed `ExplorerTableColumn<T>`, sortable headers, striped
  rows, empty-state slot.
- `PaginationControls.tsx` — cursor Previous/Next (no page numbers /
  totals), disabled by nullable cursor; `PagerButton` inner component.
- `TableSectionHeader.tsx` — title + badge + description + action slots.
- `TableEmptyState.tsx` — four Figma kinds (transactions/ledgers/
  tokens/nft).
- `useTableUrlState.ts` — cursor/sort/filter ↔ URL query params,
  `replace: true`; sort/filter changes drop the cursor.
- `useCursorPagination.ts` — thin cursor adapter over the above.

`react-router-dom` added as a `libs/ui` peer dep (hooks need router
context). Verified: `nx` build + typecheck + lint green for `libs/ui`
and `web`; behaviour confirmed in-browser via a local, uncommitted
page-level demo (1:1 with Figma node 2-1696) — sort toggles desc/asc
and syncs `?sort`/`?dir`, `Next` writes `?cursor` and enables
`Previous`, sort/filter changes drop the stale cursor.

## Issues Encountered

- **Worktree had no `node_modules`** — `web` resolved `libs/ui` from the
  main checkout (missing the new `table/` exports). Fixed by running
  `npm install` in the worktree.
- **Stale vite dep cache** surfaced spurious "Invalid hook call" /
  `ReferenceError` after edits. Cleared `web/node_modules/.vite`; not a
  code defect.
- **Spawned task id collision** — `develop` already carried `0221`; the
  test-infra follow-up was renumbered `0221 → 0226`.

## Design Decisions

### From Plan

1. **Cursor tokens stay opaque** — components only pass them through;
   never parsed or constructed (task constraint, backend contract).
2. **URL is the state store** — filter/sort/cursor live in query params
   via `useSearchParams`; `replace: true` avoids history spam.
3. **Hooks are spec-driven, not Figma-driven** — Figma carries no
   logic; `useTableUrlState` / `useCursorPagination` come from the task
   spec (Steps 3 & 5).

### Emerged

4. **Sort caret without the DS "Active" pill** — Figma is internally
   inconsistent: DS `Table header` (8935:1142) has an Active state with
   a yellow pill + CaretDown, but the page node 2-1696 shows only a
   neutral CaretUpDown. Per review feedback the icon now reflects sort
   state (neutral when unsorted, directional caret when sorted) but
   drops the yellow pill. Deliberate middle ground between the two
   Figma variants.
5. **Theme header override corrected** — task 0058's `MuiTableCell`
   head override forced `uppercase`; Figma headers are sentence-case.
   Removed `uppercase` and aligned the head style (14/500/text.primary)
   to Figma. Touches a 0058 file but is a genuine Figma-fidelity fix.
6. **`TableEmptyState` kept to four Figma kinds** — an `accounts` kind
   was briefly added then removed; it has no Figma source and no
   accounts list route.
7. **Playground demo not shipped** — a `/table-playground` route was
   committed then reverted (`59effa9`); the page-level demo stays a
   local, uncommitted file.

## Future Work

- **Unit test infrastructure for `libs/ui`** → backlog task 0226.
  `libs/ui` has no Vitest config / test target; standing it up plus the
  first table-primitive tests is its own piece of work.

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
