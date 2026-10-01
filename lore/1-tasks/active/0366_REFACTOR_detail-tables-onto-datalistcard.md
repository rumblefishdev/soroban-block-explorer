---
id: '0366'
title: 'REFACTOR: migrate 8 detail-page tables onto shared DataListCard'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0351']
tags: ['frontend', 'refactor', 'dedup', 'tables', 'effort-medium']
links: []
history:
  - date: 2026-07-03
    status: active
    who: karolkow
    note: >
      Spawned from 0351 F6. F6 removed a copy-pasted `minHeight` floor from 8
      hand-rolled detail-page table sections; the duplicated body / skeleton /
      pagination layout remains. Migrate them onto the shared `DataListCard`
      (already used by the 7 list pages) so the state order and the pager
      position live in one place.
  - date: 2026-07-07
    status: active
    who: karolkow
    note: >
      Renumbered 0353 → 0364 to resolve an id collision: PERF task
      "ctrevents read-in-order + acclist projection" also held 0353 and is the
      rightful owner (reserved by 0345 "deferred to 0353"; referenced by
      0354/0357). This REFACTOR had no external lore refs, so it moved. Git
      branch `feat/0353_detail-tables-datalistcard` + prior lore-0353 commits
      are immutable history and left as-is; the lore id is the dedup key.
  - date: 2026-07-08
    status: active
    who: karolkow
    note: >
      Renumbered 0364 → 0366 to resolve a *second* id collision: the
      2026-07-07 renumber (0353 → 0364) landed this REFACTOR on top of PERF
      task "astlist + astdetail bounded assets-FINAL read", which already held
      0364 on develop and is referenced by the 0357 read-path cluster
      (0357/0354/0334). Same dedup rule as before — the externally-referenced
      PERF task keeps 0364; this REFACTOR (still no external lore refs) moves to
      the next free id, 0366 (0365 = PERF oa-entity-keyed-mv). Git branch name
      left as-is; lore id is the dedup key.
  - date: 2026-09-25
    status: active
    who: karolkow
    note: >
      DataListCard gained renderContainer / renderEmpty / errorPy; 5 of 8
      tables migrated on branch refactor/0366_detail-tables-onto-datalistcard.
      Pool-detail pair deferred behind 0374; LedgerTransactions left out.
  - date: 2026-09-28
    status: active
    who: karolkow
    note: >
      Review fixes: frameless `DataList` split out of `DataListCard`
      (composition replaces `renderContainer`), `emptyPy` added, tests
      tightened. Identity harness still 40/40.
  - date: 2026-09-28
    status: active
    who: karolkow
    note: >
      Follow-up after merge of #510: narrow the DataList interface, decision
      W32 A. `query` + `pager` objects replace 9 copied props, one
      `renderTable(rows, { loading })` replaces the renderSkeleton /
      renderTable pair, the unused TableSkeleton fallback and `columnCount`
      are gone, `emptyNoun` is required only with filters. 12 call sites +
      DataList: 1597 → 1361 lines. Identity harness 108/108 + 30 click checks.
---

# REFACTOR: migrate detail tables onto DataListCard

## Summary

The 7 main list pages render table + skeleton + empty/error + pagination via
the shared `web/src/pages/detail/DataListCard.tsx`. The 8 detail-embedded
tables hand-roll the same layout. Migrate them onto the shared list so the
order of states (skeleton → error → empty → table) and the pager position
have a single source of truth (0351 F6's `minHeight` floor — and future
drift — can't reappear). That is what is centralised; per-page line counts
barely move (see Implementation Notes), because each page still declares its
columns, skeleton, table and empty copy.

## Context

Follow-up from task 0351 finding F6. Refactor only, no behaviour change.

## Targets

Current file names (2026-09-25; `PoolTransactions` was renamed
`PoolActivity` in 0491).

| File                                              | Status                                      |
| ------------------------------------------------- | ------------------------------------------- |
| `web/src/pages/accounts/AccountTransactions.tsx`  | migrated                                    |
| `web/src/pages/assets/AssetTransactions.tsx`      | migrated                                    |
| `web/src/pages/nft-detail/NftTransfers.tsx`       | migrated                                    |
| `web/src/pages/contracts/ContractInvocations.tsx` | migrated                                    |
| `web/src/pages/contracts/ContractEvents.tsx`      | migrated                                    |
| `web/src/pages/pool-detail/PoolParticipants.tsx`  | deferred — follow-up after 0374 merges      |
| `web/src/pages/pool-detail/PoolActivity.tsx`      | deferred — follow-up after 0374 merges      |
| `web/src/pages/ledgers/LedgerTransactions.tsx`    | left out — low value (see Design Decisions) |

## Known gaps to resolve

- **Custom empty states.** Some detail tables use a bespoke `EmptyState`
  (e.g. PoolParticipants "No participants yet" + `GroupIcon`), not the standard
  `TableEmptyState(emptyKind)` DataListCard renders. Likely needs an optional
  `renderEmpty` slot on `DataListCard` (or standardise the empty states).
- **LedgerTransactions pagination.** Uses a count-based caption + plain
  `onPrev/onNext`, not cursor pagination — confirm it maps to DataListCard's
  pagination props.
- **`isReloading`** (`isPlaceholderData`) must be wired per table so the
  skeleton shows during page changes, matching current behaviour.

## Acceptance Criteria

- [ ] All 8 tables use the shared `DataList`; no hand-rolled
      body/skeleton/pagination — **5 of 8 done**. PoolParticipants + PoolActivity deferred until
      0374 merges (its open PRs rewrite pool-detail); LedgerTransactions left
      out as low value. See Design Decisions → Emerged.
- [x] Each detail page renders identically for loading / empty / error /
      populated / paginating states — proven by a before/after HTML harness
      (40/40 cases). Live look done 2026-09-25 against the production API
      through the dev proxy: account transactions, asset (native) latest
      transactions and NFT transfer history each render 10 rows in their own
      card; a contract with one invocation renders it in the tab (then in
      a bare box) with the pager, and its Events tab renders the custom empty state
      ("This contract has not emitted any events yet.").
- [x] `web` typecheck + lint + test green
- [x] **Docs updated** — N/A (no system-shape change; pure FE component reuse)
- [x] **API types regenerated** — N/A (FE-only)

## Implementation Notes

- **Two components** (after the 2026-09-28 review, see Emerged):
  - `web/src/pages/detail/DataList.tsx` — no frame: filters, then exactly one
    of skeleton / error / empty / table, then the pager. Detail sections
    render it inside their own frame: `SectionCard` (account, asset), a
    `Card` with a `TableSectionHeader` (NFT), nothing at all (contract tabs —
    the tab's `Card` is the frame; the old unstyled wrapper `<Box>` is gone).
  - `web/src/pages/detail/DataListCard.tsx` — `<Card><DataList/></Card>`,
    same props. The 7 list pages keep their call sites unchanged.
- Props added to the shared list, nothing page-specific:
  - `renderEmpty()` — a custom unfiltered empty state (NFT, contract
    invocations, contract events: own copy / icon). Typed as exclusive with
    `emptyKind` (union), so a caller passes exactly one. The filtered-empty
    state still wins when `hasActiveFilters`.
  - `emptyPy` — padding of the `emptyKind` preset (only allowed with
    `emptyKind`). Account and asset transactions pass 6.
  - `errorPy` — error-state padding, default 8 (list pages). The four
    detail tables that used `QueryErrorState`'s default pass 6; NFT keeps 8.
  - `renderContainer` existed in the first revision and was removed.
- ~~`columnCount` / `emptyNoun` stay required~~ — superseded by the
  2026-09-28 follow-up below: `columnCount` is gone, `emptyNoun` is required
  only with `hasActiveFilters`.
- **Line counts** (`wc -l`, `origin/develop` → branch): AccountTransactions
  144 → 142, AssetTransactions 127 → 124, NftTransfers 145 → 139,
  ContractInvocations 129 → 123, ContractEvents 224 → 221 — the five pages
  together 769 → 749 (−20). Shared code 129 (`DataListCard`) → 171
  (`DataList` 156 + `DataListCard` 15). The gain is one owner of the state
  order and pager position, not fewer lines.
- Tests:
  - `web/src/pages/detail/__tests__/DataList.test.tsx` (10 cases): filled +
    pager + no frame, filters → body → pager order, skeleton on loading and
    reloading, generic `TableSkeleton` fallback (counts its rows and cells),
    error + retry, `errorPy` and `emptyPy` padding (compared against a
    reference render of `QueryErrorState` / `TableEmptyState` at that py, not
    by walking `EmptyState`'s DOM), `renderEmpty`, filtered-empty precedence,
    and `@ts-expect-error` cases for both / neither of `emptyKind` +
    `renderEmpty` and `emptyPy` with `renderEmpty`.
  - `web/src/pages/detail/__tests__/DataListCard.test.tsx` (3 cases): the
    Card wraps filters + body + pager, props are forwarded, and the same
    union holds on the card (`@ts-expect-error`, both / neither).
  - Loosening the union (letting `renderEmpty` pass with `emptyKind`) makes
    `typecheck` fail on the "both" cases (TS2578 unused directive), and
    changing the `errorPy` default / dropping `emptyPy` fails the two padding
    tests. The page tests (`AccountDetailPage`, `AssetDetailPage`,
    `ContractDetailPage`) pass unchanged.
- Identity proof: a throwaway vitest harness rendered the pre-migration copy
  and the migrated page side by side, per page, in 8 states (loading,
  reloading with previous rows, 503 / 429 / generic error, empty, first page,
  middle page) and compared `innerHTML`. 40/40 identical, before and after
  the `DataList` split. It unwraps every style-less `<Box>`
  (`div.MuiBox-root.css-0`, no CSS) on both sides: DataList's body `<Box>`
  on the branch, and the contract tabs' old wrapper `<Box>` on develop.
  Mutation checks: ContractEvents' `errorPy` 6 → 8 failed its 3 error cases;
  after the split, dropping AccountTransactions' `emptyPy={6}` failed its
  empty case. The harness was not committed (it imports `.orig` copies); it
  is in the main checkout's `.trash/0366-identity-harness/`. The 2026-09-25
  live look predates the split; the harness re-proved the DOM after it.

### Follow-up 2026-09-28 — narrower DataList interface (W32 A)

After #510 every call site still passed 13–14 props, most copied 1:1, and
production lines went 898 → 920. The follow-up narrows the interface:

- **Removed props:** `isLoading`, `isReloading`, `isError`, `error`,
  `onRetry` (→ `query`); `rows`, `canPrev`, `canNext`, `onPrev`, `onNext`
  (→ `pager`); `renderSkeleton` (→ `renderTable`'s `loading` flag);
  `columnCount`, `skeletonRows` (the generic `TableSkeleton` fallback,
  which no caller reached).
- **Added props:** `query: DataListQuery` (structural: `isLoading`,
  `isPlaceholderData`, `isError`, `error`, `refetch` — a `useQuery` result
  fits as is) and `pager: DataListPager<T>` (structural: `rows`, `canPrev`,
  `canNext`, `handlePrev`, `handleNext` — a `usePagedRows` result fits as
  is). `renderTable(rows, { loading })` is called with `[]` and
  `loading: true` on first load and while reloading.
- **Changed:** `emptyNoun` is required only together with
  `hasActiveFilters` (a union, like `emptyKind` / `renderEmpty`); Ledgers and
  the five detail sections no longer pass one.
- All 12 callers already used `usePagedRows` and a React Query `useQuery`
  hook, so no adapter was needed anywhere.
- The 7 `*_COLUMN_COUNT` exports (`AccountsTable`, `AssetsTable`,
  `ContractsTable`, `LedgersTable`, `PoolsTable`, `NftsTable`,
  `TransactionsTable`) existed only for `columnCount` and were deleted.
  `ListPageSkeleton`'s comments, which pointed at DataListCard's
  `skeletonRows` default and its `TableSkeleton`, were corrected.
- **Line counts** (`wc -l`, `origin/develop` → branch):

  | File                   | Before | After |
  | ---------------------- | -----: | ----: |
  | AccountsListPage       |     99 |    80 |
  | AssetsListPage         |    118 |   102 |
  | ContractsListPage      |     89 |    74 |
  | LedgersListPage        |     69 |    53 |
  | LiquidityPoolsListPage |    101 |    85 |
  | NftsListPage           |     84 |    68 |
  | TransactionsListPage   |    104 |    89 |
  | AccountTransactions    |    142 |   119 |
  | AssetTransactions      |    124 |   100 |
  | NftTransfers           |    139 |   115 |
  | ContractInvocations    |    136 |   112 |
  | ContractEvents         |    221 |   197 |
  | DataList               |    156 |   152 |
  | DataListCard           |     15 |    15 |
  | **Total**              |   1597 |  1361 |

  −236 lines, plus −19 from the deleted `*_COLUMN_COUNT` exports.

- **Identity harness** (same method as #510, extended to all 12 call
  sites; `.orig` copies of the 12 pages + old `DataList` / `DataListCard`
  from `origin/develop`, hook mocked per state, `innerHTML` compared,
  style-less `div.MuiBox-root.css-0` unwrapped, React ids normalised):
  108/108 identical — 12 pages × 8 states (loading, reloading, 503, generic
  error, 429, empty, first page, middle page), 6 filtered-empty states (the
  list pages with filters), 6 `?dir=asc` states (loading + filled for the 3
  sortable tables). Each state also asserts what it rendered (skeleton = 21
  `<tr>`, filled = 2, error = "Try again", empty = 0). Plus 30 behaviour
  checks, old vs new: Next then Previous produce the same hook-call cursors
  (12), Try again calls `refetch()` bare (12), header sort clicks while
  loading (3) and when filled (3) produce the same calls.
  Mutations caught: rendering AccountTransactions' skeleton with the sort
  props failed 4 cases (loading, reloading, loading asc, header click while
  loading); NftsListPage `skeletonRows` 20 → 19 failed loading + reloading.
  Harness not committed; it is in the main checkout's
  `.trash/0366-identity-harness-2/`.
- `DataList.test.tsx` / `DataListCard.test.tsx` rewritten for the new props
  (not a behaviour change): the generic `TableSkeleton` fallback test is
  gone with the fallback; new asserts: loading calls `renderTable` with no
  rows, retry calls `refetch()` with no arguments, and a
  `@ts-expect-error` for `hasActiveFilters` without `emptyNoun`. The
  `errorPy` reference render now includes a retry button, because DataList
  always wires `refetch`.

## Design Decisions

### From Plan

- **Scope (set by the lead, 2026-09-25):** extend DataListCard minimally, then
  migrate AccountTransactions, AssetTransactions, NftTransfers,
  ContractInvocations, ContractEvents.
- **PoolParticipants + PoolActivity OUT:** the open PRs #455 / #496 of task
  0374 are rewriting pool-detail now; migrating them here would collide.
  Follow-up once 0374 merges — the new slots should cover both
  (PoolParticipants' "No participants yet" + `GroupIcon` is a `renderEmpty`).
- **LedgerTransactions OUT, low value:** the parent page owns loading and
  error, so the component only has an empty state and a count-caption pager.
  Migrating it means passing constant `isLoading={false}` / `isError={false}`
  just to relocate those two — contortion, not dedup.
- **Mobile `renderCard` (Notes) is a feature, not this refactor** — not done.

### Emerged

- **~~One `renderContainer` render-prop instead of `header` + `variant`~~ —
  superseded 2026-09-28, see the next entry.** The three looks are not "same
  Card plus a header": `SectionCard` also changes the card and body
  background, and the contract tabs have no card at all. A render-prop
  reproduced all three exactly.
- **Composition instead of the render-prop (review, 2026-09-28).** The frame
  is not the list's business: `DataList` renders the frameless content and
  each caller puts it in its own frame; `DataListCard` is the list pages'
  `Card` + `DataList`. Same three looks, no container prop to learn, and the
  contract tabs lose a wrapper `<Box>` that existed only to be a container.
  Done now because it is cheaper than after the deferred pool-detail pages
  (PoolParticipants, PoolActivity) adopt the API.
- **`emptyPy` instead of `renderEmpty` for padding (review, 2026-09-28).**
  Account and asset transactions used `renderEmpty` only to render the
  standard preset at py 6. `emptyPy` sits next to `errorPy` and is typed to
  go with `emptyKind` only; `renderEmpty` stays for genuinely custom copy.
  No density system — two numbers are enough for two paddings.
- **`errorPy` as a number rather than a `renderError` slot.** Error content
  is identical everywhere (same `QueryErrorState` classification + retry);
  only the padding differs (6 vs 8). A slot would make every caller rebuild
  the error switch.
- **Kept the body `<Box>` wrapper** so list pages stay byte-identical; it
  has no styles.
- **Structural `query` / `pager` types, not React Query's own
  (follow-up, 2026-09-28).** `DataListQuery` lists only the five fields
  DataList reads, so a non-React-Query caller can build one; a `useQuery`
  result and a `usePagedRows` result both fit without mapping. All fields
  are required — every caller has them, and a required `refetch` means the
  error state always offers a retry, as all 12 callers already did.
- **`pager` carries `rows` too.** `usePagedRows` returns rows and the pager
  together, so passing its result whole removes the `rows` prop as well as
  the four pager props.
- **Deleted the `TableSkeleton` fallback, `columnCount` and
  `skeletonRows`** instead of making `columnCount` conditional: all 12
  callers passed `renderSkeleton`, so the fallback was unreachable. The
  `*_COLUMN_COUNT` exports it fed were deleted with it.
- **`emptyNoun` as a union with `hasActiveFilters`, not optional with a
  default.** A default ("results") would compile but let a filtered list
  silently lose its noun; the union costs a few type lines and makes the
  compiler demand the noun exactly where it is shown.
- **The skeleton's header still ignores the sort.** Before, the three
  sortable tables (Accounts, Ledgers, AccountTransactions) rendered their
  skeleton without `sortDir` / `onSortChange` (and without `sortBy` in
  AccountTransactions): the header showed the default sort and a click did
  nothing. With one `renderTable`, passing the sort props unconditionally
  would change that (the harness caught it), so those three pages spread
  them only when `!loading`. Showing the current sort in the skeleton is
  arguably better, but it is a UI change, not this refactor.
- **Retry calls `refetch()` with no arguments** (`() => void
query.refetch()`), never `onRetry={query.refetch}`, so the click event can
  never reach React Query as refetch options.

## Issues Encountered

- Under heavy machine load (load average ~66) the pre-commit `web:test` run
  timed out two unrelated list-page tests at 5 s (AccountsListPage,
  TransactionsListPage). Both pass alone and on the re-run; no change made.

## Notes

- **Mobile card rows belong here** (2026-08-18, from the 0491 UX pass). On a
  375px viewport the richest tables are ~3 screens of horizontal scroll —
  the pool activity table measured 1020px. 0491 added the small lever
  (`ExplorerTableColumn.hideBelow`, adopted for its Account column, 1020 →
  860px), but the honest ceiling of a five-column data table on a phone is a
  **card row**: chip + amount line, secondary line, meta line — the reason
  stellar.expert reads well on mobile is that its sentence rows reflow.
  Since this task funnels every detail table through one shell, a
  `renderCard` variant on `DataListCard` (used below `sm`, `renderTable`
  above) would give all 8 tables a mobile mode in one place instead of
  eight bespoke ones. Scope it here, not per-table.
