---
id: '0272'
title: 'Audit 0257 closing — elastic single-task implementation of remaining findings'
type: FEATURE
status: backlog
related_adr: ['0032']
related_tasks:
  ['0257', '0262', '0263', '0264', '0265', '0270', '0271']
tags:
  [
    'frontend',
    'backend',
    'audit-closing',
    'elastic',
    'priority-high',
    'effort-large',
    'cross-cutting',
    'phase-implementation',
  ]
links:
  - 'Parent audit: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/README.md'
  - 'Master action queue: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/audit-action-queue.md'
  - 'Prior fix batches: 0262/0263/0264 (Gate B), 0265 (Vite CVE), 0270 (search canonical)'
  - 'Spawned follow-ups: 0271 (search broad enhancement)'
history:
  - date: '2026-05-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0257 audit closure decision. Per user senior call
      2026-05-27: bypass per-finding spawn convention (50 atomic backlog
      tasks) in favor of elastic single-task closure. Trade-off accepted:
      faster execution + single audit-trail file vs less parallel-team
      friendly + scope-creep risk. Master action queue at
      `lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/audit-action-queue.md`
      contains 38 cluster cards + 375-row appendix covering 281+ cumulative
      findings. User implements card-by-card with `STATUS: TODO → DONE`
      flips in queue file; each commit cites card N.M + closed F-IDs.
---

# Audit 0257 closing — elastic single-task implementation

## Summary

Single elastic task for implementing remaining post-audit-0257 work.
Replaces the per-finding spawn convention (~50 atomic tasks) with one
elastic container backed by a master action queue. User implements
card-by-card from the queue; STATUS field in queue tracks progress;
commits cite finding IDs.

Pre-launch closure target: ~3-4 working days for MUST + SHOULD tier
cards. Full backlog (incl. NICE + POST-LAUNCH cards): ~24 days FE
spread across pre-launch sprint + post-launch maintenance.

## Status: Backlog

Activate when ready to begin closure sprint. Independent of other
in-flight tasks (Filip 0241 indexer, Staś 0252 CH validation, etc.) —
this is pure FE closure work plus minor backend coordination cards.

## Context

Audit 0257 (Frontend comprehensive audit, pre-launch) completed Waves
1-6 with ~225+ cumulative findings (2 🔴 / 33 🟠 / 84 🟡 / 107 🟢) across
30+ findings files. Audit-blocker fix-first scope already CLEARED via:

- **Gate A:** F-E-1 (URL cursor write) RESOLVED by 0254 pagination
  merge; F-E-2 (URL op normalisation) DROPPED per user "URL = wire
  contract"; F-D-1 (API stale binary) RESOLVED by binary restart.
- **Gate B batch (PR #219):** 0262 (composite NotFound), 0263 (pool
  detail cross-entity links + PoolAssetLeg backend), 0264 (strkey
  canonical cross-cutting + NFT route refactor bonus), 0265 (Vite
  CVE bump) — closed 7 audit findings (F-D-2, F-AE-5, F-K-2, F-K-3,
  F-K-9, F-AN-8, F-CO-1).
- **0270 (PR #220):** search strkey canonical output + redirect
  coverage — closed F-L-1, F-K-4, NFT search-404 regression. Spawned
  0271 follow-up (search broad enhancement + pool strkey column).

Remaining work documented in master action queue:
**`lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/audit-action-queue.md`**

## Implementation approach

### Per-session workflow

1. Open `findings/audit-action-queue.md` in editor
2. Scroll to next `STATUS: TODO` card (priority order: Category 1 first;
   within category by Pre-launch tier MUST → SHOULD → NICE → POST-LAUNCH)
3. Read card Rationale + Scope + Sub-checklist
4. Implement on branch `feat/0272_<card-slug>` (or batch multiple cards
   on single branch if same area)
5. Mark queue `STATUS: DONE` (or `STATUS: IN-PROGRESS` mid-session)
6. Update sub-checklist `[ ]` → `[x]` for each finding closed
7. Update appendix STATUS column for all closed F-IDs (bulk)
8. Commit: `feat(lore-0272): close C N.M <title> — F-X-Y, F-X-Z, ...`
9. Open PR → merge develop → repeat next card

### Category ordering (Option A — category-based, per user 2026-05-27)

| Cat | # cards | Theme |
|---|---:|---|
| C1 | 3 | Pre-launch must-fix (footer legal, build SHA, contracts list page) |
| C2 | 3 | Atomic refactor batches (format/truncate/debounce, folder rationalization, state primitives) |
| C3 | 3 | Type-safety (noUncheckedIndexedAccess, branded IDs, assertNever) |
| C4 | 1 | Performance (bundle + LP lazy + vendor split) |
| C5 | 4 | Routing leftovers (NotFound h1+main, URL tab state, sub-section queries, cross-entity gaps) |
| C6 | 4 | Forward-linked (lore drift 0066, 23 spawn, backend coord, ADR/doc sweep) |
| C7 | 8 | Wave 6 visual / UX |
| C8 | 8 | Catalog / lore / docs |
| C9 | 1 | Bulk out-of-scope spawn (13 follow-up tasks) |
| C10 | 3 | Gated external (LP oracle ADR, MUI 7→9, 0251 B1 root cause) |

### Pre-launch decision points

Pending user decisions during impl (capture in queue card Notes):

1. **C1.1 Footer legal** — path (a) real hrefs (needs legal content)
   or (b) hide until ready?
2. **C8.3 Responsive redesign** — mobile launch in scope or desktop-first?
   (Determines if MUST or POST-LAUNCH.)
3. **C9.1 Out-of-scope GDPR** — EU launch target? Could escalate
   POST-LAUNCH → MUST.
4. **C10.1 Figma fidelity** — user provides URLs; spawn dedicated
   research task post-URL.
5. **C6.2 23 unspawned Future Work** — all or cherry-pick few?

## Acceptance Criteria

High-level — granular checkpoints tracked per-card in the queue file.

- [ ] All C1 (pre-launch must-fix) cards STATUS = DONE or SKIP with rationale
- [ ] Sufficient C2-C5 cards landed for pre-launch quality bar (user decides quorum)
- [ ] C9.1 spawn pass executed (13 out-of-scope follow-up tasks live in backlog)
- [ ] All RESOLVED appendix rows verified post-merge (positive checks against develop)
- [ ] All SKIP appendix rows documented with rationale in audit-summary.md
- [ ] Master audit task 0257 archived: `git mv active/0257_* archive/0257_*` + frontmatter `status: completed`
- [ ] `lore/3-wiki/` updated if patterns emerged worth documenting
  (FE testing standards, formatter conventions, error state taxonomy,
  useTableUrlState ADR cross-link)
- [ ] **Docs updated** — `docs/architecture/frontend/frontend-overview.md`
      cross-link to audit-action-queue.md final state per ADR 0032.
- [ ] **API types regenerated** — `N/A` if no `crates/api/**` changes;
      mark explicitly per card requiring backend coordination
      (C6.3 XDR codegen, C10.1 if Figma URL surfaces backend gaps).

## Out of scope (explicit)

- Per-finding atomic spawn tasks (would be 50+ tasks; convention bypass
  per user decision)
- Phase 3 sub-phase 3.1 consolidated-bugs.md (replaced by queue file
  serving same role)
- audit-summary.md final write-up (separate close-out task, post-impl)

## Notes

- **Effort:** ~3-4 days pre-launch core (MUST + SHOULD subset), ~24 days
  full closure if all cards landed. User can ship pre-launch subset +
  post-launch backlog rest.
- **Scope-creep risk:** elastic task by design. Mitigate by checkpoint
  reviews every ~10 cards completed — re-evaluate remaining queue, drop
  cards that proved unimportant.
- **Audit trail:** preserved via commit messages citing F-IDs (not via
  task IDs since this is one elastic container). Final
  audit-action-queue.md state = closed-state document.
- **Parallel team execution:** harder than 50-task spawn model. If
  Filip/Karol/Staś want to grab specific cards, they can implement
  on parallel branches; merge ordering coordinated via this task body.
- **Re-classification budget:** queue appendix has ~4 borderline rows
  flagged (F-D-5, F-AA-1, F-AN-5, F-AN-6) for ad-hoc reclassify during
  impl based on user judgment.
