---
id: '0072'
title: 'Frontend: Ledgers list and detail pages'
type: FEATURE
status: completed
related_adr: []
related_tasks: []
tags: [priority-medium, effort-small, layer-frontend-pages]
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
    note: 'Promoted to active'
  - date: 2026-05-18
    status: completed
    who: karolkow
    note: >
      Ledgers list + detail pages. 10 files (~660 insertions): 2 hooks,
      4 subcomponents, 2 pages. Reuses 0069 TransactionsTable and 0061
      table primitives. typecheck/lint/build pass; verified in browser
      against a local mock API (real backend not exercised). No tests
      (0226 covers test infra). Cross-task debt noted under Future Work.
---

# Frontend: Ledgers list and detail pages

## Summary

Implement the Ledgers list page (`/ledgers`) and Ledger detail page (`/ledgers/:sequence`). The list page is a chain history browser optimized for monotonic sequence traversal. The detail page shows ledger metadata and paginated transactions within the ledger.

## Status: Completed

**Current state:** Implemented on `feat/0072`, verified locally against a mock API.

## Context

Ledgers are the fundamental time-ordered unit of the Stellar blockchain. The list page lets users browse the chain history, and the detail page shows what happened in a specific ledger. Previous/next navigation allows stepping through adjacent ledgers.

### API Endpoints Consumed

| Endpoint                 | Query Params      | Purpose                                       |
| ------------------------ | ----------------- | --------------------------------------------- |
| `GET /ledgers`           | `limit`, `cursor` | Paginated ledger list                         |
| `GET /ledgers/:sequence` | none              | Single ledger detail with linked transactions |

### Ledger List Table Columns

| Column            | Display                                                | Notes                                                              |
| ----------------- | ------------------------------------------------------ | ------------------------------------------------------------------ |
| Sequence          | Dominant visual anchor, linked to `/ledgers/:sequence` | IdentifierDisplay (task 0062). Sequence is the primary identifier. |
| Hash              | Truncated                                              | IdentifierDisplay (task 0062)                                      |
| Closed At         | Relative timestamp                                     | RelativeTimestamp (task 0063)                                      |
| Protocol Version  | Integer                                                | e.g., "21"                                                         |
| Transaction Count | Integer                                                | Number of transactions in the ledger                               |

- Default sort: most recent first
- Cursor-based pagination, no total counts

### Ledger Detail Fields

| Field             | Display                  | Notes                                |
| ----------------- | ------------------------ | ------------------------------------ |
| Sequence          | Full, prominent          | Primary identifier                   |
| Hash              | Full, copyable           | IdentifierWithCopy (task 0062)       |
| Closed At         | Full datetime + relative | RelativeTimestamp (task 0063)        |
| Protocol Version  | Integer                  | e.g., "21"                           |
| Transaction Count | Integer                  | Count of transactions in this ledger |
| Base Fee          | Value                    | Base fee for this ledger             |

### Transactions in Ledger

- Paginated table of all transactions in this ledger
- Reuses global transaction row conventions (same columns as `/transactions` list)
- Columns: hash, source account, operation type, status badge, fee, timestamp
- SectionHeader: "Transactions in Ledger #12345678"

### Previous / Next Navigation

- Previous ledger: sequence - 1 (link to `/ledgers/:prev_sequence`)
- Next ledger: sequence + 1 (link to `/ledgers/:next_sequence`)
- Stable at newest indexed ledger: "Next" disabled if no higher sequence exists
- Persistent navigation buttons at top of detail page

## Implementation Plan

### Step 1: Ledger list query hook and page

Create `apps/web/src/pages/ledgers/useLedgersList.ts` and `LedgersListPage.tsx`:

- Fetches `GET /ledgers` with limit and cursor
- Stale time: 60 seconds
- Table with columns: sequence, hash, closed_at, protocol version, tx count
- Cursor-based pagination controls
- Loading skeleton, empty state, error state

### Step 2: Ledger detail query hook

Create `apps/web/src/pages/ledger-detail/useLedgerDetail.ts`:

- Fetches `GET /ledgers/:sequence`
- Stale time: 5 minutes (immutable once closed)
- Param validation: positive integer (from task 0067)

### Step 3: Ledger detail summary

Create `apps/web/src/pages/ledger-detail/LedgerSummary.tsx`:

- Renders: sequence, hash (full, copyable), closed_at, protocol version, tx count, base fee
- Previous/next navigation buttons

### Step 4: Transactions in ledger section

Create `apps/web/src/pages/ledger-detail/LedgerTransactions.tsx`:

- Paginated transaction table (same columns as transactions list page)
- SectionHeader: "Transactions in Ledger #[sequence]"
- Uses ExplorerTable (task 0061) with transaction row conventions

### Step 5: Ledger detail page composition

Create `apps/web/src/pages/ledger-detail/LedgerDetailPage.tsx`:

- Composes: LedgerSummary, LedgerTransactions
- Each section in SectionErrorBoundary (task 0064)
- 404 state: "Ledger not found"
- Loading skeleton during fetch

## Acceptance Criteria

- [x] Ledger list columns: sequence (linked), hash (truncated), closed_at, protocol version, tx count
- [x] List sorted most recent first with cursor-based pagination
- [x] Detail shows: sequence, hash (copyable), closed_at, protocol version, tx count, base fee
- [x] Transactions in ledger: paginated table reusing global transaction row conventions
- [x] Previous/next ledger navigation works correctly
- [x] Next disabled at newest indexed ledger
- [x] Param validation: positive integer for sequence
- [x] 404 state: "Ledger not found"
- [x] Loading skeleton and error states for both list and detail

All verified in the browser against a local mock API; the real backend
was not exercised.

## Implementation Notes

10 files (~660 insertions):

- `web/src/api/hooks/useLedgersList.ts`, `useLedgerDetail.ts` — infinite-query
  hooks over `listLedgersInfiniteOptions` / `getLedgerInfiniteOptions`.
- `web/src/pages/LedgersListPage.tsx`, `LedgerDetailPage.tsx` — pages
  (replaced the `PageStub` placeholders).
- `web/src/pages/ledgers/LedgersTable.tsx`, `LedgerNav.tsx`,
  `LedgerSummary.tsx`, `LedgerTransactions.tsx` — page subcomponents.
- `web/src/api/hooks/index.ts` — hook exports.

`typecheck`, `lint`, `build` pass. Depends on task 0061 (table primitives)
and 0069 (transactions table), both merged into the branch.

## Design Decisions

### From Plan

1. **Reuse 0069 `TransactionsTable`** for the embedded "transactions in
   ledger" table — identical row conventions to `/transactions`.
2. **Mirror `useTransactionsList`** — `useInfiniteQuery` plus a local
   `pageIndex` driving `PaginationControls`, same as the transactions list.
3. **Param validation** via `isLedgerSequence` (task 0067).

### Emerged

4. **Prev/next from the API**, not arithmetic. Spec said `sequence ± 1`;
   the API returns gap-aware `prev_sequence` / `next_sequence`, so those
   are used — arithmetic would break on indexing gaps.
5. **Files under `web/`, not `apps/web/`** as the spec literally said —
   spec paths were stale. Hooks live in `web/src/api/hooks/` (project
   convention), not `pages/ledgers/`.
6. **Hash rendering**: detail = full + copy; list = middle-truncated
   (`{prefix:6,suffix:4}`) + copy. Both `linked={false}` — a ledger has no
   hash route, it is addressed by sequence.
7. **Section header "Transactions in this ledger"** (Figma wording), not
   the spec's "Transactions in Ledger #N".
8. **Cross-imported `formatFee` + `TransactionTime`** from
   `pages/transactions/` (0069) — see Future Work for the suggested hoist
   to `libs/ui`.
9. **`LedgerSummary` is a custom key/value table** — `libs/ui` has no
   key/value or detail-row component.
10. **`LedgerNav` uses raw MUI `Button` + `sx`** — `libs/ui` has no generic
    `Button` (only `NavButton`, a top-nav style). See Future Work.

## Issues Encountered

- **`getLedger` has a required `path` param** → `initialPageParam: {}`
  failed typecheck. Fixed with `initialPageParam: { path: { sequence } }`.
- **`ledger` entity-type truncation is `{0,0}`** (never truncates — tuned
  for sequence numbers), so the list hash rendered full. Fixed with an
  explicit `truncation` config on the hash cell.
- **Verified on a mock API only** (`web/dev-mock-server.mjs`, untracked dev
  scratch) — the real Rust backend was not run.

**Broken/modified tests:** none — no tests added (test infrastructure is
task 0226).

## Future Work

Cross-task debt surfaced while implementing the ledger pages — recorded
here, not yet scheduled:

- **Shared UI primitives.** `libs/ui` has no generic `Button` (only the
  top-nav `NavButton`), and `formatFee` + the two-line timestamp live in
  `web/src/pages/transactions/`. `LedgerNav` reimplements button styling,
  and the ledger pages cross-import from `pages/transactions/`. Hoisting a
  `Button`, the fee formatter and a generic two-line timestamp into
  `libs/ui` would remove the `pages/ledgers/` → `pages/transactions/`
  coupling.
- **URL-synced cursor pagination.** List pages keep pagination in local
  state, so a page is not deep-linkable — counter to
  `docs/architecture/frontend/frontend-overview.md` §5. `useTableUrlState`
  / `useCursorPagination` (task 0061) exist but are unused; wiring them
  into the list pages would make pagination addressable.

## Notes

- Ledger data is immutable once the ledger is closed, so long stale times are appropriate.
- Transaction rows within a ledger should look identical to rows on the global transactions page for consistency.
- Sequence is the dominant visual anchor for each row in the list, not the hash.
