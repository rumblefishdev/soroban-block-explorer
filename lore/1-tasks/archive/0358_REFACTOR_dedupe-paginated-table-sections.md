---
id: '0358'
title: 'REFACTOR: dedupe paginated-table-section boilerplate — cursor-pagination hook, shared column defs, PAGE_SIZE const'
type: REFACTOR
status: completed
related_adr: []
related_tasks: ['0351']
tags: [frontend, refactor, dedup, tables]
links: []
history:
  - date: 2026-07-06
    status: active
    who: karolkow
    note: 'Task created — safe-subset dedup surviving audit; supersedes the rejected DataListCard-migration follow-up'
  - date: 2026-07-06
    status: completed
    who: karolkow
    note: >
      All three cuts landed, behaviour- and pixel-preserving. A:
      usePagedRows hook collapses the identical rows + usePageHandlers tail
      at 14 call-sites. B: ledgerColumn/hashColumn/statusColumn factories
      replace the identical column literals (4/4/4 tables); divergent
      columns untouched. C: single PAGE_SIZE=20 replaces 12 local consts +
      2 inline literals. 30 files, net -60 LOC. Verified: web typecheck +
      lint + test (111/111) green; browser boot/render/responsive
      desktop+mobile, zero runtime errors in refactored code. Delivered via
      PR (develop <- refactor/0358).
---

# REFACTOR: dedupe paginated-table-section boilerplate

## Summary

Remove genuine copy-paste across the frontend's paginated table sections
**without changing any behaviour or any pixel of the rendered UI, and without
introducing prop drilling.** Three narrow, low-risk cuts: (A) a data-only
cursor-pagination hook, (B) shared column definitions for the identical
Ledger/Hash/Status cells, (C) one shared `PAGE_SIZE = 20` constant. This is the
subset of the 0351 F6 follow-up that actually reduces redundancy safely — the
originally-proposed `DataListCard` migration was rejected (see Non-Goals).

## Status: Active

**Current state:** Scoped from a two-agent frontend dedup audit. Not started.

## Context

Task 0351 fix F6 removed a copy-pasted `<Box minHeight>` floor from 8
detail-embedded table sections. The floor was gone but the surrounding
duplication remained, so a follow-up was proposed to migrate all 8 onto the
shared `web/src/pages/detail/DataListCard.tsx`.

That migration was investigated and **rejected**: the 8 sections are NOT
uniform. Their loading/error/empty branches diverge per caller (custom
`EmptyState` icons + copy for Pool/NFT, `TableEmptyState` with custom
title/description for Contract/Ledger, `py` padding of 6 / 8 / none, three
different wrappers: `SectionCard` / bare `<Box>` / `Card`+`TableSectionHeader`,
and `DataListCard`'s own `<Card>` + forced `py={8}` error state). Hosting that
variance in `DataListCard` would require ~5 new optional render-prop slots
(`title`, header mode, `renderEmpty`, error/empty `py`) threaded from every
caller — i.e. exactly the prop drilling we want to avoid — and would change
padding/wrapper visuals. It fails the "zero visual change / no prop drilling"
bar, so it is explicitly out of scope (Non-Goals).

What a repo-wide audit DID confirm as safe, genuine duplication:

**A. Cursor-pagination data boilerplate (~15 call-sites).** Byte-identical in
shape across the 7 list pages, the 7 detail sections, and `LedgerDetailPage`:

```ts
const { cursor, goNext, goPrev } = useCursorPagination({ cursorParam, resetKey });
const { data, isLoading, isPlaceholderData, isError, error, refetch } = useX(...);
const rows = data?.data ?? [];
const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(data?.page, goNext, goPrev);
```

This is pure data/logic wiring — **no JSX, no visual output** — so extracting it
into a hook cannot change any rendering. Callers keep their own JSX bodies.

**B. Identical table column definitions.** The same cell renderers are
copy-pasted across tables:

- **Ledger** column — identical in 5 tables (`AccountTransactions`,
  `AssetTransactions`, `ContractInvocations`, `ContractEvents`,
  `TransactionsTable`): `<IdentifierDisplay value={String(row.ledger_sequence)} type="ledger" />`.
- **Hash** column — identical in 5 tables: `<IdentifierWithCopy value={row.hash} type="transaction" />`.
- **Status** column — identical in 3 tables: `<StatusChip successful={row.successful} />`.

There is already a home for shared cells: `web/src/pages/transactions/cells.tsx`
(hosts `OperationCell`). Extracting these as shared column objects/factories
produces the exact same rendered output.

**C. `PAGE_SIZE = 20` repeated.** Defined as an inline `const PAGE_SIZE = 20` in
6 API hooks plus 2 inline `limit: 20` literals. One shared constant.

## Implementation Plan

### Step A: cursor-pagination hook

Extract the 4-line boilerplate into a small hook (e.g.
`web/src/pages/detail/usePaginatedSection.ts` or under `web/src/api`) that takes
the query hook + `{ cursorParam?, resetKey }` and returns
`{ rows, isLoading, isReloading, isError, error, refetch, canPrev, canNext, handlePrev, handleNext }`.
The hook returns **values only** — each call-site keeps its own table/skeleton/
empty/error JSX exactly as-is. Migrate the ~15 call-sites. No JSX moves into the
hook.

### Step B: shared column definitions

Add `createLedgerColumn()` / `createHashColumn()` / `createStatusColumn()` (or
plain exported column consts, since they take no params) alongside
`web/src/pages/transactions/cells.tsx`. Replace the copy-pasted column literals
with them. Leave columns that diverge as-is unless they parametrise trivially
(Time differs by `sortable`; Source/Caller differs by field name) — only include
those if the parameter is a clean 1-arg and adds no branching.

### Step C: shared PAGE_SIZE

Add `export const PAGE_SIZE = 20` to a shared module (e.g. `web/src/api/polling.ts`)
and reference it from the 6 hooks + 2 inline literals.

## Acceptance Criteria

- [x] **Zero behaviour change** — proven by code equivalence (same column
      defs, same `rows`/handlers logic, same page size) + 111 green component
      tests that render populated tables and assert cell output. Browser check
      (desktop+mobile) confirmed shell/skeleton/empty/error render with zero
      runtime errors in refactored code. NOTE: a live-data pixel diff
      develop↔branch was NOT run — no backend available in the worktree
      (Lambda-only, dev proxy key gitignored); populated-state parity rests on
      code-equivalence + unit tests, not a screenshot comparison.
- [x] **No prop drilling introduced** — `usePagedRows` returns values only
      (`rows` + page handlers); no render-prop callbacks added anywhere. Column
      factories take zero args.
- [x] A: 14 call-sites use `usePagedRows` (7 list pages + 7 detail sections).
      LedgerDetailPage left hand-wired (nested `data.transactions.page`).
- [x] B: Ledger/Hash/Status columns sourced from shared factories in
      `transactions/cells.tsx` across the tx/event/invocation tables.
- [x] C: single `PAGE_SIZE` in `api/polling.ts`; 12 local consts + 2 inline
      `limit:20` literals removed.
- [x] `nx run web:typecheck`, `web:lint`, `web:test` all green (111/111).
- [x] **Docs updated** — N/A — pure frontend internal refactor; no change to
      schema, API endpoints, ingestion, infra, or frontend data contracts
      (per [ADR 0032](../2-adrs/0032_docs-architecture-evergreen-maintenance.md)).
- [x] **API types regenerated** — N/A — no change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Implementation Notes

- **Files:** 30 changed, net −60 LOC. New: `web/src/api/usePagedRows.ts`.
  Shared column factories added to `web/src/pages/transactions/cells.tsx`.
- **B scope:** identical columns replaced — ledger×4 (Transactions, Account,
  ContractInvocations, ContractEvents), hash×4 (Transactions, Latest, Account,
  PoolTransactions), status×4 (Transactions, Latest, Account,
  ContractInvocations). Divergent columns (`transaction_hash`,
  `first_deposit_ledger`, `deployed_at_ledger`) left untouched.

## Design Decisions

### Emerged

1. **A: hook takes the query `data`, not the query hook.** The plan sketched a
   hook receiving the query hook and returning `isReloading` etc. Passing a hook
   as a callback breaks `react-hooks/rules-of-hooks` (lint gate), and renaming
   `isPlaceholderData`→`isReloading` in every caller's JSX is render-logic churn
   against the zero-visual-change bar. Chose the leaner shape: caller keeps its
   own status destructure + JSX; hook only collapses the byte-identical
   `rows` + `usePageHandlers` tail.
2. **Two ListPage tests + two DetailPage tests re-pointed their mocks.**
   `Transactions/AssetsListPage.test` now mock the specific hook module (as
   `AccountsListPage.test` already did) instead of the whole `api/index.js`
   barrel; `Account/AssetDetailPage.test` feed the real `usePagedRows` via
   `importActual`. Moving `usePagedRows`/`PAGE_SIZE` into the barrel otherwise
   made them resolve `undefined` under a barrel mock (sections fell into their
   error boundary). Test-infra only; assertions unchanged.
3. **`skeletonRows={20}` in detail sections left as literals.** Out of C's
   scope (C = the API `limit` constant); the skeleton count is a display concern
   that only coincidentally equals PAGE_SIZE.

## Non-Goals (explicitly rejected — do not attempt)

- **Migrating the 8 detail sections onto `DataListCard`.** Rejected: their
  loading/error/empty branches and wrappers diverge; hosting the variance needs
  render-prop slots = prop drilling + visual (padding/wrapper) change.
- **A rendering `usePaginatedTableBody` hook** that owns the empty/error/table
  JSX. Same trap as above — the empty branch is not uniform.
- **`createDetailHook` / `createListHook` factories.** Callers diverge in path
  param names, `enabled` conditions, and generated types; a factory hides intent
  rather than removing true duplication.
- **Collapsing the specialised error-state wrappers** (`NotFoundState`,
  `TransientErrorState`, etc.) — intentionally thin, distinct semantics.

## Notes

- Divergent columns NOT to force-merge: **Time** (`sortable` varies:
  `TransactionsTable` sorts, others don't) and **Source/Caller**
  (`source_account` vs `caller_account`). Parametrise only if it stays a clean
  1-arg with no added branching; otherwise leave duplicated.
- `LedgerTransactions` is presentational (rows as props, parent owns loading) —
  it participates in A only via its parent `LedgerDetailPage`, not as its own
  section hook.
- Audit also flagged, and this task ignores as noise: `capitalize` is NOT dead
  (used in 3 places — an audit agent was wrong); `TransactionDetailPage.tsx`
  re-export and the `states/empty` barrel are 1-line indirections not worth a
  churn.
