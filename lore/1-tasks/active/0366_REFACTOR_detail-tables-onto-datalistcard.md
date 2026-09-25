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
      (already used by the 7 list pages) to delete the duplication at the root.
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
---

# REFACTOR: migrate detail tables onto DataListCard

## Summary

The 7 main list pages render table + skeleton + empty/error + pagination via
the shared `web/src/pages/detail/DataListCard.tsx`. The 8 detail-embedded
tables hand-roll the same layout. Migrate them onto `DataListCard` so the
layout has a single source of truth (0351 F6's `minHeight` floor — and future
drift — can't reappear).

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

- [ ] All 8 tables use `DataListCard`; no hand-rolled body/skeleton/pagination
      — **5 of 8 done**. PoolParticipants + PoolActivity deferred until
      0374 merges (its open PRs rewrite pool-detail); LedgerTransactions left
      out as low value. See Design Decisions → Emerged.
- [x] Each detail page renders identically for loading / empty / error /
      populated / paginating states — proven by a before/after HTML harness
      (40/40 cases), **not** verified live: the dev server's API proxy needs
      a `DEV_API_KEY` this worktree does not have. A live look remains open.
- [x] `web` typecheck + lint + test green
- [x] **Docs updated** — N/A (no system-shape change; pure FE component reuse)
- [x] **API types regenerated** — N/A (FE-only)

## Implementation Notes

- `DataListCard` gained three optional props, nothing page-specific:
  - `renderContainer(content)` — the shell around filters + body + pager.
    Default is the old plain `<Card>`, so the 7 list pages are untouched.
    Detail pages pass a `SectionCard` (account, asset), a `Card` +
    `TableSectionHeader` (NFT), or a bare `Box` (contract tabs).
  - `renderEmpty()` — the unfiltered empty state. Typed as exclusive with
    `emptyKind` (union), so a caller passes exactly one. The filtered-empty
    state still wins when `hasActiveFilters`.
  - `errorPy` — error-state padding, default 8 (list pages). The four
    detail tables that used `QueryErrorState`'s default pass 6; NFT keeps 8.
- `columnCount` / `emptyNoun` stay required; detail pages pass
  `columns.length` and a real noun. Both are unused there (they use
  `renderSkeleton` and have no filters) but keep the list-page contract
  unchanged.
- Tests: `web/src/pages/detail/__tests__/DataListCard.test.tsx` (9 cases:
  filled + pager, loading, reloading, fallback skeleton, error + retry +
  `errorPy`, preset empty, `renderEmpty`, filtered-empty precedence, default
  Card, `renderContainer`). The page tests (`AccountDetailPage`,
  `AssetDetailPage`, `ContractDetailPage`) pass unchanged.
- Identity proof: a throwaway vitest harness rendered the pre-migration copy
  and the migrated page side by side, per page, in 8 states (loading,
  reloading with previous rows, 503 / 429 / generic error, empty, first page,
  middle page) and compared `innerHTML`. 40/40 identical. The only DOM
  difference, normalised away, is DataListCard's style-less `<Box>` around
  the body (`MuiBox-root css-0`, no CSS). Mutation check: setting
  ContractEvents' `errorPy` to 8 made its 3 error cases fail, so the harness
  catches a padding change. The harness was not committed (it imports
  `.orig` copies); it is in the main checkout's `.trash/0366-identity-harness/`.

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

- **One `renderContainer` render-prop instead of `header` + `variant`.** The
  three looks are not "same Card plus a header": `SectionCard` also changes
  the card and body background, and the contract tabs have no card at all.
  One render-prop reproduces all three exactly; a `header` slot would have
  needed a second knob for the container. A render function (not a component
  type) avoids remounting the table when the parent re-renders.
- **`errorPy` as a number rather than a `renderError` slot.** Error content
  is identical everywhere (same `QueryErrorState` classification + retry);
  only the padding differs (6 vs 8). A slot would make every caller rebuild
  the error switch.
- **Kept the body `<Box>` wrapper** so list pages stay byte-identical; it
  has no styles.

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
