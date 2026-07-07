---
id: '0358'
title: 'REFACTOR: dedupe paginated-table-section boilerplate — cursor-pagination hook, shared column defs, PAGE_SIZE const'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0351']
tags: [frontend, refactor, dedup, tables]
links: []
history:
  - date: 2026-07-06
    status: active
    who: karolkow
    note: 'Task created — safe-subset dedup surviving audit; supersedes the rejected DataListCard-migration follow-up'
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

- [ ] **Zero behaviour change** — every affected list page and detail section
      renders identically in all five states (first-load skeleton, reloading/
      pagination skeleton, empty, error, populated) at desktop + mobile widths.
      Verify by inspection; no snapshot/copy/padding/wrapper changes.
- [ ] **No prop drilling introduced** — the pagination hook returns values only;
      no `renderTable` / `renderEmpty` / `renderSkeleton`-style callbacks added
      to any shared component. Column helpers take at most one trivial arg.
- [ ] A: ~15 call-sites use the shared cursor-pagination hook.
- [ ] B: Ledger/Hash/Status columns sourced from shared defs across the tables.
- [ ] C: single shared `PAGE_SIZE`/`limit` constant.
- [ ] `nx run web:typecheck`, `web:lint`, `web:test` all green.
- [ ] **Docs updated** — N/A — pure frontend internal refactor; no change to
      schema, API endpoints, ingestion, infra, or frontend data contracts
      (per [ADR 0032](../2-adrs/0032_docs-architecture-evergreen-maintenance.md)).
- [ ] **API types regenerated** — N/A — no change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.

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
