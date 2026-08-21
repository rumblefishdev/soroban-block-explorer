---
id: '0458'
title: 'REFACTOR: web boundaries — promote the shared page-kit out of pages/, forbid cross-page imports'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0453', '0366']
tags: [frontend, architecture, priority-medium, effort-medium]
links: []
history:
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0453's architecture review: pages/ directories act as
      shared libraries (SectionCard 18 importers, PageBreadcrumb 9,
      TransactionTime 9, operationTypes 6+) and nothing polices pages/X ->
      pages/Y edges — the Nx boundary rule only sees project-level imports.
  - date: 2026-08-19
    status: backlog
    who: karolkow
    note: >
      Absorbed 0455 review findings 45 and 52 — a 1000-line component inside
      pages/ and a 928-line theme-overrides file in libs/ui. Both measured, both
      pure moves with this task's existing gate; neither warranted its own task.
---

# REFACTOR: web boundaries — page-kit out of pages/

## Scope

- Move `web/src/pages/detail/*` (SectionCard, SummaryRow, FeeCell,
  PageBreadcrumb, DataListCard, PageHeader…) and the shared modules of
  `web/src/pages/transactions/` (operationTypes, formatters, cells,
  TransactionTime) to `web/src/components/` (or libs/ui where generic).
- Add an eslint `no-restricted-imports` (or import-x) rule forbidding
  `pages/X -> pages/Y`; allow `pages/* -> components/*`.
- While there: `libs/ui/src/visualization/` lost its cohesion after
  OperationFlowTree's removal (Tabs/LazySection/useTabUrlState under a
  "visualization" name) — regroup or rename.
- Coordinate with 0366 (DataListCard migration) — same files.

## Also here: two files past any plausible size limit (0455 findings 45, 52)

Measured 2026-08-19:

| File                                                          | Lines |
| ------------------------------------------------------------- | ----- |
| `web/src/pages/transaction-detail/op-card/ExecutionTrace.tsx` | 1000  |
| `libs/ui/src/theme/overrides.ts`                              | 928   |

`ExecutionTrace.tsx` carries three separable concerns in one component (the
trace tree, its node rendering, and the surrounding card). It sits inside
`pages/`, which is exactly the boundary this task draws — so whatever moves out
of `pages/detail/*` should take it into account rather than leave the largest
file in the directory untouched.

`overrides.ts` is a different shape: a single flat block of component overrides
with no internal structure. It does not cross a boundary, so it is not a
boundary fix — but it lives in `libs/ui`, this task already reaches into
`libs/ui/src/visualization/`, and splitting it by component family is the same
kind of pure move with the same gate (no behaviour change, full suite green).

Neither is urgent. Both are cheap while the files are already open, and
expensive to schedule on their own.

## Acceptance criteria

- [ ] Zero `pages/X -> pages/Y` imports; lint rule proves it stays that way
- [ ] Pure moves + import fixes; no behavior change (full suite green)
