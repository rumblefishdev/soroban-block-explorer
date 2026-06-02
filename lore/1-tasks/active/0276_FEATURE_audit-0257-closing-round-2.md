---
id: '0276'
title: 'Audit 0257 closing round 2 — pre-launch SHOULD batch from full re-run findings'
type: FEATURE
status: active
related_adr: ['0032']
related_tasks: ['0257', '0272', '0274', '0275']
tags:
  [
    'frontend',
    'audit-closing',
    'elastic',
    'priority-high',
    'effort-medium',
    'pre-launch',
    'phase-implementation',
  ]
links:
  - 'Parent audit: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/README.md'
  - 'Master action queue: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/audit-action-queue.md'
  - 'Predecessor container: lore/1-tasks/archive/0272_FEATURE_audit-0257-closing.md (round 1, PR #230)'
  - 'Tier triage source: audit-action-queue.md §"Pre-launch tier triage (2026-06-01)"'
history:
  - date: '2026-06-01'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0257 full re-run on merged baseline (HEAD e3fe1968,
      post 0272/0243/0273). Round 1 closure (0272) retired; this is the
      round-2 elastic container scoped to the pre-launch SHOULD batch
      surfaced by the re-run + earlier-session findings. Master action
      queue tier triage = source of truth (1 MUST backend-owned by 0274,
      14 SHOULD here, ~20 NICE deferred). Same elastic card-by-card model
      as 0272; commits cite F-IDs.
  - date: '2026-06-01'
    status: active
    who: karolkow
    note: Promoted backlog → active for implementation (round-2 pre-launch SHOULD batch).
---

# Audit 0257 closing round 2 — pre-launch SHOULD batch

## Summary

Second elastic closure container for audit 0257, scoped to the
**pre-launch SHOULD batch** surfaced by the 2026-06-01 full re-run on the
merged baseline (after the 0272 round-1 closure landed). Round 1's elastic
container (0272) is retired; round 2 picks up the next realistic
pre-launch quality bar. Master action queue remains the source of truth —
this task implements its `SHOULD`-tier findings card-by-card.

## Status: Active (In-Progress)

Currently in active development.

## Context

Full re-run (Waves 1-4 code + live Wave 5-6 @375) produced 36 new findings
(F-RR-1..33, F-W6-LOADSKEL-1..3) on top of the 0272-session list-page
findings (F-0272S-1..6). Deterministic baseline verified GREEN (typecheck
clean once deps built; tests 86/86 on develop — local 26-fail was a
worktree env artifact). 0272 consolidation (NetworkToggle, formatters,
hex→tokens, debounce, error-state primitives) verified clean.

## Scope — SHOULD batch (14)

Implement these from the master action queue (file:line + repro in queue):

1. [x] **F-RR-17** — PoolCharts: surface `isError` + retry (stop masking fetch error as "no activity"). `pool-detail/PoolCharts.tsx`. (✅ DONE in `3724e0d3`)
2. [x] **F-RR-33** — NotFoundState long-strkey overflow @375 → `word-break`/`overflow-wrap`. `libs/ui/src/states/errors/NotFoundState.tsx`. (✅ DONE)
3. [x] **F-RR-25** — `/search` error state: route through `QueryErrorState` (add retry, stop swallowing error class). `search/SearchResultsView.tsx` + `useSearchResults` expose error/refetch. (✅ DONE in `3724e0d3`)
4. [ ] **F-RR-2** — remove (or wire) dead "CTRL + K" hint pill. `home/HeroSearch.tsx:100`.
5. [x] **F-RR-3** — gate home section footer "N latest records" on success branch. `home/LatestTransactions.tsx`, `LatestLedgers.tsx`. (✅ DONE in `3724e0d3`)
6. [x] **F-RR-6** — "Choose payment" → "Choose operation" (picker lists all op types). `transaction-detail/sections/OperationPicker.tsx:89`. (✅ DONE in `bcc440b5`)
7. [x] **F-RR-7** — wrap `event.contract_id` in `IdentifierDisplay` link. `transaction-detail/advanced/EventsSection.tsx:80-90`. (✅ DONE in `3724e0d3`)
8. [x] **F-RR-18** — FeePill: drop the bad fallback arg so NaN fee → em-dash. `liquidity-pools/FeePill.tsx:24`. (✅ DONE)
9. [ ] **F-RR-21** — search a11y: listbox `role=option`/`aria-activedescendant`, tablist `tabpanel`/`aria-controls`. `search/GlobalSearchBar.tsx`, `SearchResultsTabs.tsx`.
10. [ ] **F-RR-14** — LedgerSummary semantic `<h2>` (a11y). `ledgers/LedgerSummary.tsx`.
11. [x] **F-W6-LOADSKEL-1** — route Suspense fallback shape: add `HomeSkeleton` (hero+KPI+2 tables) so phase-1 ≈ phase-2 (kills home load flicker). `router/index.tsx` + new skeleton. (✅ DONE)
12. [ ] **F-0272S-3** — remove dead sort arrows (assets total-supply, ledgers sequence). FE-quick; backend-sort variant = POST (F-RR-1).
13. [x] **F-0272S-4** — silent no-op search: add placeholder + empty-state hints (transactions tx-hash gate, NFT exact-match). (✅ DONE via helper text validation warning in `9146d4ee`)
14. [x] **F-0272S-2** — LP `filter[asset_code]` → ILIKE partial (match assets). (✅ DONE via ILIKE substring matching in `9146d4ee`)
15. [x] **F-DP-3 (11.3)** — raw zIndex additions → themed zIndex scale. (✅ DONE on theme in this session)

## Out of scope

- **MUST F-0272S-1** (accounts mock → 404) — owned by **0274** (`/v1/accounts` endpoint). Not duplicated here.
- **NICE (~20)** + **POST/backend (3)** findings — stay in queue; cherry-pick post-launch.
- Per-finding atomic spawn tasks — elastic container by design (same as 0272).

## Acceptance Criteria

- [/] All 14 SHOULD findings DONE or SKIP-with-rationale (8/14 completed); queue STATUS + appendix flipped per finding
- [x] Each commit cites the F-ID(s) closed
- [ ] F-0272S-2 / F-RR-1 backend bits coordinated with 0274 (or split out)
- [ ] **Docs updated** — `N/A` unless a fix changes system shape (most are FE-local); mark per ADR 0032
- [ ] **API types regenerated** — `N/A` unless `crates/api/**` touched (F-0272S-2 LP ILIKE may qualify)
- [x] Queue `audit-action-queue.md` reflects final round-2 state

## Notes

- Elastic, card-by-card. Checkpoint review every ~6 cards.
- Effort: ~2-3 FE days for the 14 SHOULD (most are small/local).
- Audit trail via commit F-ID citations (one elastic container, not 14 task IDs).
