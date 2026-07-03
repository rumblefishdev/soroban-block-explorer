---
id: '0353'
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
