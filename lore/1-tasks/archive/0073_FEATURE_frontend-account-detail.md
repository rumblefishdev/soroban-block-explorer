---
id: '0073'
title: 'Frontend: Account detail page'
type: FEATURE
status: completed
related_adr: []
related_tasks: []
tags: [priority-medium, effort-small, layer-frontend-pages]
milestone: 2
links:
  - 'https://www.figma.com/design/n1p6WCMVd4iinbuvOA2WjP/Designs?node-id=154-12747&m=dev'
  - 'https://www.figma.com/design/n1p6WCMVd4iinbuvOA2WjP/Designs?node-id=157-22153&m=dev'
  - 'https://www.figma.com/design/siumLgKOc9LLepEfbimyp3/Design-System---Stellar-Block-Explorer?node-id=360-1812&m=dev'
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-18
    status: active
    who: karolkow
    note: 'Promoted to active — bundled with 0074 on shared branch feat/0073-0074_frontend-account-and-asset-pages.'
  - date: 2026-05-18
    status: active
    who: karolkow
    note: 'Spec sync: paths corrected to web/ (no apps/); hooks to web/src/api/hooks/ per 0066; sub-component layout per 0069; status badge via Chip (no StatusBadge); balance link /tokens→/assets per 0154.'
  - date: 2026-05-18
    status: active
    who: karolkow
    note: 'Added Figma links — account detail frames + design-system file (Chip).'
  - date: 2026-05-18
    status: active
    who: karolkow
    note: 'Implemented account detail page. Account-transactions table omits the Source account column to match the Figma design — acceptance criteria updated to match.'
  - date: 2026-05-18
    status: completed
    who: karolkow
    note: >
      Completed. Account detail page (summary, balances, transactions)
      delivered on branch feat/0073-0074. ~10 files; verified 1:1 against
      Figma; typecheck/lint/build green. Tests deferred to task 0226.
---

# Frontend: Account detail page

## Summary

Implement the Account detail page (`/accounts/:accountId`) showing account summary, balances, and paginated transaction history. This is the canonical destination for account ID lookups from global search and linked identifiers throughout the explorer.

## Status: Completed

**Current state:** Implemented on branch
`feat/0073-0074_frontend-account-and-asset-pages`; archived pending merge.

## Context

The account detail page provides a complete view of a Stellar account's state and activity. It serves as the landing page when users click any account identifier in the explorer or search for an account by ID.

### API Endpoints Consumed

| Endpoint                                 | Query Params      | Purpose                                              |
| ---------------------------------------- | ----------------- | ---------------------------------------------------- |
| `GET /accounts/:account_id`              | none              | Account summary: balances, sequence, first/last seen |
| `GET /accounts/:account_id/transactions` | `limit`, `cursor` | Paginated transactions involving this account        |

### Account Summary Fields

| Field             | Display                        | Notes                                                     |
| ----------------- | ------------------------------ | --------------------------------------------------------- |
| Account ID        | Full, copyable                 | IdentifierWithCopy (task 0062). Prominent at top of page. |
| Sequence Number   | Integer                        | Account sequence number                                   |
| First Seen Ledger | Linked to `/ledgers/:sequence` | IdentifierDisplay (task 0062)                             |
| Last Seen Ledger  | Linked to `/ledgers/:sequence` | IdentifierDisplay (task 0062)                             |

### Balances Section

| Field              | Display                            | Notes                                                |
| ------------------ | ---------------------------------- | ---------------------------------------------------- |
| XLM Balance        | Native balance                     | Prominent, at top of balances                        |
| Trustline Balances | Token code + balance per trustline | Each token code linked to `/assets/:id` if available |

- Balances visually separated from transaction history
- XLM balance distinguished from trustline/token balances

### Account Transactions Table Columns

| Column          | Display                                    | Notes                                  |
| --------------- | ------------------------------------------ | -------------------------------------- |
| Hash            | Truncated, linked to `/transactions/:hash` | IdentifierDisplay (task 0062)          |
| Ledger Sequence | Linked to `/ledgers/:sequence`             | IdentifierDisplay (task 0062)          |
| Operation Type  | Human-readable label                       | Same as global transactions table      |
| Status          | Badge (success/failed)                     | `Chip` color success/error (task 0063) |
| Fee             | XLM amount                                 | Fee charged                            |
| Timestamp       | Relative                                   | RelativeTimestamp (task 0063)          |

- Paginated with cursor-based pagination
- Reuses global transaction row conventions
- Source account column omitted per the Figma design

## Implementation Plan

> Structure follows task 0069 (transactions list): the page entry file stays
> flat in `web/src/pages/` (router imports it), page-specific sub-components go
> in the route-named subdirectory `web/src/pages/accounts/`, and data hooks go
> in `web/src/api/hooks/` per task 0066 convention.

### Step 1: Account detail query hooks

Create `web/src/api/hooks/useAccountDetail.ts` and `web/src/api/hooks/useAccountTransactions.ts`:

- `useAccountDetail`: fetches `GET /accounts/:account_id`, stale time 5 minutes
- `useAccountTransactions`: fetches `GET /accounts/:account_id/transactions` with cursor, stale time 60 seconds
- Separate queries for independent section fetching

### Step 2: Account summary section

Create `web/src/pages/accounts/AccountSummary.tsx`:

- Renders: account ID (full, copyable), sequence number, first seen ledger (linked), last seen ledger (linked)
- Summary card layout at top of page

### Step 3: Balances section

Create `web/src/pages/accounts/AccountBalances.tsx`:

- XLM balance prominent at top
- Trustline/token balances listed below
- Each token code linked to token detail if available
- Visually separated from transactions section

### Step 4: Account transactions section

Create `web/src/pages/accounts/AccountTransactions.tsx`:

- Paginated transaction table with standard columns
- SectionHeader: "Transactions"
- Uses ExplorerTable (task 0061) with cursor pagination
- Reuses global transaction row conventions

### Step 5: Page composition

Flesh out the existing router stub `web/src/pages/AccountDetailPage.tsx`:

- Composes: AccountSummary, AccountBalances, AccountTransactions
- Each section in SectionErrorBoundary (task 0064)
- Param validation: G... format account ID (from task 0067)
- 404 state: "Account not found"
- Loading skeleton during fetch

## Acceptance Criteria

- [x] Account summary shows: account ID (full, copyable), sequence number, first seen ledger (linked), last seen ledger (linked)
- [x] Balances section shows: XLM balance (prominent) + trustline/token balances
- [x] Balances visually separated from transaction history
- [x] Transaction table columns: hash, ledger sequence, operation type, status badge, fee, timestamp
- [x] Transactions paginated with cursor-based pagination
- [x] Account summary and transactions fetched independently (separate queries)
- [x] Failed transactions section does NOT collapse account summary
- [x] Param validation: G... format for accountId
- [x] 404 state: "Account not found"
- [x] Loading skeleton and error states per section

## Implementation Notes

Delivered on branch `feat/0073-0074_frontend-account-and-asset-pages`
(commits `6e3347f`, `1518020`).

- Hooks: `web/src/api/hooks/useAccountDetail.ts`, `useAccountTransactions.ts`.
- Page: `web/src/pages/AccountDetailPage.tsx` (fleshed-out router stub).
- Sub-components: `web/src/pages/accounts/{AccountSummary,AccountBalances,AccountTransactions}.tsx`.
- Shared primitives created here and reused by 0074:
  `web/src/pages/detail/{SectionCard,SummaryRow,PageBreadcrumb}.tsx`,
  `web/src/pages/useInfinitePager.ts`, `web/src/pages/format.ts`, and
  `web/src/pages/transactions/cells.tsx` (Dash / OperationCell / StatusCell
  extracted from the 0069 table for reuse).
- Verified 1:1 against the Figma frames by rendering against a mock API;
  typecheck, lint and build green. No automated tests — deferred to 0226.

## Issues Encountered

- **Infinite-query `initialPageParam`**: for path-bearing endpoints
  (`/accounts/:id/transactions`) the page-param type requires `path`, so
  `initialPageParam` is `{ path: { account_id } }`, not `{}`.
- **Custom Typography variants are inline**: `bodySmMedium` etc. map to
  `<span>`, so stacked label/value pairs ran together — fixed by wrapping
  them in `Stack`.

## Design Decisions

### From Plan

1. **Independent per-section queries** — summary/balances
   (`useAccountDetail`) and transactions (`useAccountTransactions`) are
   separate queries, each in its own `SectionErrorBoundary`, so a failure in
   one section never collapses the others.
2. **Reuse libs/ui + 0069 conventions** — ExplorerTable, identifiers,
   states, `Chip`, `RelativeTimestamp`; transaction rows reuse 0069 cells.

### Emerged

3. **Source account column dropped** — the Figma account-transactions table
   has no Source account column though the original spec listed one. Figma
   took priority; column omitted and acceptance criteria updated.
4. **Badge via generic `Chip`** — task 0063 shipped a generic `Chip`, not a
   named `StatusBadge`; status renders `Chip` with success/error colour.
5. **Balances show only Native / Classic** — the API `AccountBalance` cannot
   distinguish a SAC (`native | credit_alphanum4 | credit_alphanum12`), so
   balance rows label "Native asset" or "Classic" only.
6. **`useInfinitePager` extracted** — cursor-pagination logic pulled into a
   shared hook; the 0069 transactions list was refactored onto it
   (commit `9008a8a`) to keep one implementation.

## Notes

- This page is the canonical destination for all account ID links and search results.
- The account scope is intentionally limited to summary, balances, and transactions per the architecture docs.
- Transaction rows should look identical to the global transactions list page for consistency. Reuse the conventions from `web/src/pages/transactions/TransactionsTable.tsx` (task 0069).
