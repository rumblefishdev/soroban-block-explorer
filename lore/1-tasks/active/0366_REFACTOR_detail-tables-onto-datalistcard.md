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

- `web/src/pages/pool-detail/PoolParticipants.tsx`
- `web/src/pages/pool-detail/PoolTransactions.tsx`
- `web/src/pages/accounts/AccountTransactions.tsx`
- `web/src/pages/assets/AssetTransactions.tsx`
- `web/src/pages/contracts/ContractInvocations.tsx`
- `web/src/pages/contracts/ContractEvents.tsx`
- `web/src/pages/ledgers/LedgerTransactions.tsx`
- `web/src/pages/nft-detail/NftTransfers.tsx`

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
- [ ] Each detail page renders identically for loading / empty / error /
      populated / paginating states (verified live)
- [ ] `web` typecheck + lint + test green
- [ ] **Docs updated** — N/A (no system-shape change; pure FE component reuse)
- [ ] **API types regenerated** — N/A (FE-only)

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
