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

## Acceptance criteria

- [ ] Zero `pages/X -> pages/Y` imports; lint rule proves it stays that way
- [ ] Pure moves + import fixes; no behavior change (full suite green)
