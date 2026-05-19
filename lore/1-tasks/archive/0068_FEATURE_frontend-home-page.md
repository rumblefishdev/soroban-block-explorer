---
id: '0068'
title: 'Frontend: Home page'
type: FEATURE
status: completed
related_adr: []
related_tasks: []
tags: [priority-high, effort-medium, layer-frontend-pages]
milestone: 2
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-18
    status: active
    who: karolkow
    note: 'Promoted to active — starting Figma-first frontend home page implementation.'
  - date: 2026-05-19
    status: completed
    who: karolkow
    note: >
      Home page implemented Figma-first on feat/0068 (PR #197): 9 new home
      components + 2 query hooks + shared-component fixes. Built 1:1 to
      Figma with documented deviations; verified with Playwright + a mock
      API. Key decisions: page-level hero backdrop, dedicated HeroSearch,
      48px DS table rows.
---

# Frontend: Home page

## Summary

Implement the Home page (`/`) as the entry point and chain overview for the Stellar Block Explorer. Provides at-a-glance network state, latest transactions, and latest ledgers with polling-based auto-refresh.

## Status: Completed

**Current state:** Implemented on `feat/0068_frontend-home-page` (PR #197),
verified, and archived. Lands on develop when the PR merges.

## Context

The home page is the fastest way to understand whether the indexer is current and the explorer is healthy. It uses a dashboard summary layout (compact summary cards) followed by latest-activity modules. Polling refreshes summary counts and latest rows without layout jumping.

### API Endpoints Consumed

| Endpoint                     | Purpose                                                                       | Polling     |
| ---------------------------- | ----------------------------------------------------------------------------- | ----------- |
| `GET /network/stats`         | Chain overview: current ledger sequence, TPS, total accounts, total contracts | Yes, 10-15s |
| `GET /transactions?limit=10` | Latest 10 transactions for the activity table                                 | Yes, 10-15s |
| `GET /ledgers?limit=10`      | Latest 10 ledgers for the activity table                                      | Yes, 10-15s |

### Chain Overview Cards

| Card            | Field                   | Source                                    |
| --------------- | ----------------------- | ----------------------------------------- |
| Current Ledger  | Ledger sequence number  | `GET /network/stats` -> `ledger_sequence` |
| TPS             | Transactions per second | `GET /network/stats` -> `tps`             |
| Total Accounts  | Account count           | `GET /network/stats` -> `total_accounts`  |
| Total Contracts | Contract count          | `GET /network/stats` -> `total_contracts` |

### Latest Transactions Table Columns

| Column         | Display                                    | Notes                                   |
| -------------- | ------------------------------------------ | --------------------------------------- |
| Hash           | Truncated, linked to `/transactions/:hash` | Identifier component (task 0062)        |
| Source Account | Truncated, linked to `/accounts/:id`       | Identifier component (task 0062)        |
| Operation Type | Human-readable label                       | e.g., "Payment", "Invoke Contract"      |
| Status         | Badge (success/failed)                     | StatusBadge component (task 0063)       |
| Timestamp      | Relative ("2 min ago")                     | RelativeTimestamp component (task 0063) |

### Latest Ledgers Table Columns

| Column            | Display                        | Notes                                   |
| ----------------- | ------------------------------ | --------------------------------------- |
| Sequence          | Linked to `/ledgers/:sequence` | Identifier component (task 0062)        |
| Closed At         | Relative timestamp             | RelativeTimestamp component (task 0063) |
| Transaction Count | Integer                        | Number of transactions in the ledger    |

## Implementation Plan

### Step 1: Network stats query hook

Create `apps/web/src/pages/home/useNetworkStats.ts`:

- Fetches `GET /network/stats`
- Polling interval: 10-15 seconds
- Stale time: 10-30 seconds

### Step 2: Latest activity query hooks

Create `apps/web/src/pages/home/useLatestTransactions.ts` and `useLatestLedgers.ts`:

- Fetch `GET /transactions?limit=10` and `GET /ledgers?limit=10`
- Polling interval: 10-15 seconds
- No cursor pagination needed (always shows latest)

### Step 3: Chain overview cards section

Create `apps/web/src/pages/home/ChainOverview.tsx`:

- Four compact summary cards: ledger sequence, TPS, accounts, contracts
- Card skeleton loader while loading
- Independent error boundary (failed stats should not collapse activity tables)

### Step 4: Latest transactions section

Create `apps/web/src/pages/home/LatestTransactions.tsx`:

- Table with columns: hash, source account, operation type, status badge, timestamp
- Uses ExplorerTable component (task 0061)
- SectionHeader: "Latest Transactions"
- "View All" link to `/transactions`

### Step 5: Latest ledgers section

Create `apps/web/src/pages/home/LatestLedgers.tsx`:

- Table with columns: sequence, closed_at, tx count
- Uses ExplorerTable component (task 0061)
- SectionHeader: "Latest Ledgers"
- "View All" link to `/ledgers`

### Step 6: Home page composition

Create `apps/web/src/pages/home/HomePage.tsx`:

- Composes: ChainOverview, LatestTransactions, LatestLedgers
- Each section wrapped in SectionErrorBoundary (task 0064)
- Polling indicator visible showing last refresh time (task 0063)
- No layout jump on polling update (stable row heights, no reflow)

## Acceptance Criteria

- [x] Chain overview cards show: current ledger sequence, TPS, total accounts, total contracts
- [x] Latest transactions table shows: hash (truncated, linked), source account (truncated, linked), operation type, status badge, timestamp (relative)
- [x] Latest ledgers table shows: sequence (linked), closed_at (relative), transaction count
- [x] All three API endpoints polled at 10-15 second intervals
- [x] Polling updates do not cause layout jump or visual reflow
- [x] Polling indicator visible showing "Updated Xs ago"
- [x] Each section has independent error boundary (failed section does not collapse others)
- [x] Skeleton loaders shown during initial load
- [x] "View All" links navigate to `/transactions` and `/ledgers`
- [x] Identifiers are linked to their respective detail pages

## Notes

- The home page is the primary indicator of explorer health and indexer freshness.
- Layout should feel like a dashboard summary, not a dense analytics page.
- The global search bar is already present in the header via the layout shell (task 0059).

## Implementation Notes

Built on `feat/0068_frontend-home-page` (PR #197). Code lives in `web/`
(spec said `apps/web/` — stale).

- New home components in `web/src/pages/home/`: `HomeHero`, `HeroSearch`,
  `ChainOverview`, `ChainOverviewCard`, `LatestTransactions`,
  `LatestTransactionsTable`, `LatestLedgers`, `LiveIndicator`,
  `ViewAllLink`; `HomePage.tsx` composes them with per-section
  `SectionErrorBoundary`.
- Query hooks `useLatestTransactions` / `useLatestLedgers` in
  `web/src/api/hooks/` — `limit: 10`, `homePolicy` (10s stale / 12s poll).
- Reused: `ExplorerTable`, `TableSectionHeader`, `LedgersTable`, the
  transaction cells, `IdentifierDisplay`/`IdentifierWithCopy`, skeletons,
  error states, `PollingIndicator`, `useNetworkStats`.
- Shared-component changes: `ExplorerTable` rows pinned to the 48px DS
  cell height; `LedgersTable` Protocol as plain text + ledger-hash
  truncation; `truncate.ts` default truncation corrected; `AppShell`
  renders the home route full-bleed.
- Docs: `frontend-overview.md` §6.2 updated (ADR 0032).
- Verified: typecheck / lint / build; Playwright visual checks against
  Figma using a mock API.

## Design Decisions

### From Plan

1. **Reuse over rebuild** — `ExplorerTable`, `LedgersTable`, transaction
   cells, identifiers and query-hook patterns reused; only the home
   composition + 2 thin hooks are genuinely new.
2. **Per-section error boundaries** — each section in its own
   `SectionErrorBoundary` so one failure does not collapse the page.

### Emerged

3. **Hero section added** — Figma shows a hero (headline + tagline +
   large search); the spec said search is header-only. Figma-first wins.
4. **Dedicated `HeroSearch`** — the shared header `SearchInput`
   collapses/expands; the Figma hero search is static. Built a separate
   component rather than forking a mode into the shared one.
5. **`AppShell` full-bleed for `/`** — needed to render the full-bleed
   Figma hero; all other routes keep the standard content padding.
6. **Page-level hero backdrop** — glow rebuilt from the exact Figma
   `Group 1` (two blurred `#fdda24` pills) and grid from `Grid layers`
   (white, 0.11 opacity, 1.26px, 80.69px pitch); a radial-gradient
   stands in for the blurred-blob grid mask.
7. **48px DS table rows** — `ExplorerTable` pins row height to the
   Design System table cell; affects every explorer table (intended).
8. **Truncation defaults corrected** — `transaction` 6/4, `account`
   4/4 in `truncate.ts`, verified against the Figma home + Transactions
   list tables; per-call overrides removed.
9. **Client-side sorting reverted** — built, then dropped: out of the
   0068 spec and the API exposes no sort parameter.
10. **`PollingIndicator` `isFetching`/`onRefresh` reverted** — built,
    then dropped as out of scope (belongs to task 0063).

## Issues Encountered

- **nx build-ordering flake** — `web:typecheck` intermittently failed
  inside `run-many` reading a stale `libs/ui/dist`; fixed by a clean
  rebuild. Not a regression.
- **Stray deletion** — commit `3e3095b` removed unrelated task files
  `0065`/`0069` (stale staged deletions in the worktree index);
  restored in `7c2ffe6`.
- **Figma glow mix-up** — `Ellipse 21` exports white and looked like
  the glow, but it is the grid's alpha mask; the real warm glow is
  `Group 1` (blurred `#fdda24` pills).

**Broken/modified tests:** none — the project has no frontend tests.

## Future Work

Not spawned as backlog tasks (contingent / owned elsewhere):

- Functional table sorting once the API exposes a sort parameter.
- Wire `AppShell` TopNav live stats (currently `MOCK_STATS`) — belongs
  with task 0059 / 0066.
- Populated-data visual diff against a real backend (verified here
  with a mock API).
