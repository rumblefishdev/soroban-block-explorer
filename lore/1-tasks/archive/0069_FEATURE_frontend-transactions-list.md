---
id: '0069'
title: 'Frontend: Transactions list page'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0227']
tags: [priority-medium, effort-medium, layer-frontend-pages]
milestone: 2
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-15
    status: active
    who: karolkow
    note: 'Task activated'
  - date: 2026-05-15
    status: active
    who: karolkow
    note: >
      Implemented the page against the Figma design (file Designs node
      114-16433 + Design System siumLgKOc9... ). 6 files under
      web/src/pages/transactions/. build + typecheck + lint green.
      Two-control filter bar (Figma) instead of the spec's three filters.
  - date: 2026-05-15
    status: completed
    who: karolkow
    note: >
      Completed. 6 files (web/src/pages/transactions/* + page). build +
      typecheck + lint green; layout, table, pagination and all states
      verified in the dev preview against a local mock API. Figma gaps in
      libs/ui shared states deferred to task 0227.
---

# Frontend: Transactions list page

## Summary

Implement the Transactions list page (`/transactions`) showing a paginated, filterable table of all indexed transactions, sorted most-recent-first by default.

## Status: Completed

**Current state:** Page implemented and verified against a local mock API.

## Context

This page is the primary browsing surface for all network transaction activity. It supports rapid scanning with filters, cursor-based pagination, and URL-synced state. The table must remain usable on large datasets without assuming total counts.

### API Endpoint Consumed

| Endpoint            | Query Params                                                                                 | Purpose                                |
| ------------------- | -------------------------------------------------------------------------------------------- | -------------------------------------- |
| `GET /transactions` | `limit`, `cursor`, `filter[source_account]`, `filter[contract_id]`, `filter[operation_type]` | Paginated, filterable transaction list |

### Table Columns

| Column          | Display                                    | Notes                                                |
| --------------- | ------------------------------------------ | ---------------------------------------------------- |
| Hash            | Truncated, linked to `/transactions/:hash` | Identifier component (task 0062)                     |
| Ledger Sequence | Linked to `/ledgers/:sequence`             | Identifier component (task 0062)                     |
| Source Account  | Truncated, linked to `/accounts/:id`       | Identifier component (task 0062)                     |
| Operation Type  | Human-readable label                       | e.g., "Payment", "Create Account", "Invoke Contract" |
| Status          | Badge (success/failed)                     | StatusBadge component (task 0063)                    |
| Fee             | Display value                              | XLM amount                                           |
| Timestamp       | Relative ("2 min ago")                     | RelativeTimestamp component (task 0063)              |

### Filters

| Filter         | Type            | Notes                               |
| -------------- | --------------- | ----------------------------------- |
| Source Account | Text input      | Filters by `filter[source_account]` |
| Contract ID    | Text input      | Filters by `filter[contract_id]`    |
| Operation Type | Dropdown/select | Filters by `filter[operation_type]` |

- Filters are additive (AND logic)
- All filters reflected in URL query params
- Filter change resets cursor (back to first page)
- No hard reloads on filter or pagination changes

### Pagination

- Cursor-based, opaque tokens
- Previous / Next only, no page numbers, no total count
- Default sort: most recent first

## Implementation Plan

### Step 1: Transactions list query hook

Create `apps/web/src/pages/transactions/useTransactionsList.ts`:

- Fetches `GET /transactions` with limit, cursor, and filter params
- Stale time: 60 seconds
- Query key: `['transactions', { cursor, filters }]`

### Step 2: Filter controls

Create `apps/web/src/pages/transactions/TransactionFilters.tsx`:

- Source account text input
- Contract ID text input
- Operation type dropdown (values from domain types)
- All values synced to URL query params via `useTableUrlState` (task 0061)
- Filter change resets cursor

### Step 3: Transactions table

Create `apps/web/src/pages/transactions/TransactionsTable.tsx`:

- Uses ExplorerTable (task 0061) with column definitions
- Columns: hash, ledger sequence, source account, operation type, status badge, fee, timestamp
- Identifier columns use linked IdentifierDisplay (task 0062)
- Status column uses StatusBadge (task 0063)
- Timestamp column uses RelativeTimestamp (task 0063)

### Step 4: Page composition

Create `apps/web/src/pages/transactions/TransactionsListPage.tsx`:

- SectionHeader: "Transactions"
- TransactionFilters above table
- TransactionsTable with data
- PaginationControls below table (task 0061)
- Loading skeleton during fetch
- Empty state when no results match filters
- Error state with retry for transient failures

## Acceptance Criteria

- [x] Table displays columns: hash, ledger, source account, operation, status, fee, time
- [x] Default sort: most recent first (API default — `GET /transactions` exposes no sort param)
- [x] Filters reflected in URL query params (`q`, `op`)
- [x] Filter change resets cursor to first page
- [x] Cursor-based pagination: Previous/Next only, no total count
- [x] No hard reloads on filter or pagination changes
- [x] All identifiers (hash, ledger, account) linked to their detail pages
- [x] Loading skeleton shown during fetch
- [x] Empty state shown — distinct "no transactions yet" vs "no transactions match your filters" (+ Clear filters)
- [x] Error state with retry button for transient failures
- [~] Filters source/contract/operation additive — **changed to two controls per Figma**: a
  combined "Source account or contract ID" input + operation-type dropdown. Source and
  contract therefore cannot be applied simultaneously. See Design Decisions → Emerged.

## Implementation Notes

Files (all under `web/src/pages/`, 5 new + the page stub replaced):

- `TransactionsListPage.tsx` — composition: header + Card(filters + body + pagination).
  Filter state via `useTableUrlState` (`q`, `op` URL params). Pagination via
  `useInfiniteQuery` (existing `useTransactionsList` hook) + a `pageIndex` cursor stack.
- `transactions/TransactionFilters.tsx` — combined search input (300 ms debounce) +
  operation-type `Select`.
- `transactions/TransactionsTable.tsx` — `ExplorerTable` (task 0061), 7 columns.
- `transactions/TransactionTime.tsx` — two-line Time cell (relative + absolute UTC).
- `transactions/operationTypes.ts` — filter dropdown options + `formatOperationType`.
- `transactions/formatters.ts` — `formatFee` (stroops → XLM), `formatAbsoluteUtc`.

Verification: `nx build/typecheck/lint web` green. Layout + transient-error path
verified in the dev preview; table-with-data not verified (local API offline).

## Design Decisions

### From Plan

1. **`ExplorerTable` / `PaginationControls` / `useTableUrlState`** from task 0061;
   `IdentifierDisplay` / `IdentifierWithCopy` from task 0062.
2. **Filters + pagination in URL / cursor-based Previous-Next**, filter change resets cursor.

### Emerged

3. **Two filter controls, not three** — the Figma design (Designs file, node 114-16433)
   merges "source account" and "contract ID" into one input; the spec's three-filter
   list was written before Figma. Input is routed to `filter[source_account]` or
   `filter[contract_id]` by StrKey prefix (`G…` / `C…`); unrecognised input applies no
   filter. Consequence: source AND contract cannot be filtered at the same time.
4. **Reused `web/src/api/hooks/useTransactionsList.ts`** (task 0066) instead of creating
   a new hook under `pages/transactions/` as the plan said — the hook already exists.
5. **Operation-type dropdown = the 5 types in the Figma dropdown** (Path Payment,
   Invoke Contract, Create Account, Payment, Manage Offer). Figma's generic Dropdown
   component still carried placeholder items ("Norway/Finland/Denmark") which were
   ignored. "Path Payment" / "Manage Offer" map to the more common XDR variant
   (`PATH_PAYMENT_STRICT_RECEIVE` / `MANAGE_SELL_OFFER`) — the Figma labels are
   ambiguous against the finer XDR enum granularity.
6. **Two-line Time cell built in `web`** (`TransactionTime.tsx`) rather than in
   `libs/ui` — `libs/ui` is owned by other tasks; page-specific composition stays in
   `web`. Uses the `formatRelative` / `useNow` primitives exported by `libs/ui`.
7. **No sort UI** — `GET /transactions` exposes no sort parameter, so columns are not
   interactive; "most recent first" is the server default.
8. **Pagination uses a `pageIndex` over `useInfiniteQuery` pages** — the API returns a
   forward cursor only, so "Previous" walks back through already-fetched pages.

## Issues Encountered

- **Worktree had no `node_modules`** — `@rumblefish/*` resolved to the main repo's
  `libs/ui` (on `develop`, without the 0061 merge), so `web` could not see
  `ExplorerTable` etc. Fixed by `npm install` inside the worktree.
- **`libs/ui/dist/index.d.ts` was stale** after the 0061 merge — cleaned `dist/` +
  `*.tsbuildinfo` and rebuilt.

## Docs Updated (ADR 0032)

- `docs/architecture/**` — `N/A`. The page consumes the existing `GET /transactions`
  endpoint with no schema, API, ingestion or infrastructure change.

## Future Work

- **Task 0227** — `libs/ui` error/empty state components diverge from Figma
  (error copy/buttons/icons, missing `TableEmptyState` filtered variant, square
  `EmptyState` icon container, no two-line `TimestampCell`). Fixes were prepared
  here but reverted — those files belong to other tasks; spawned 0227 to correct
  them at source.
- Operation pill colour (`Chip color="neutral"`) and the `+N` multi-operation
  indicator were judgement calls — confirm against Figma with design.

## Notes

- This table serves as the reference implementation for all other list pages.
- Operation type values should come from domain types (tasks 0009-0012).
- The same table row conventions should be reused wherever transactions appear (home page, account detail, token detail, etc.).
