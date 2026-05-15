---
id: '0227'
title: 'libs/ui: align error/empty states with Figma'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0069', '0063']
tags: [priority-low, effort-small, layer-frontend-shared]
milestone: 2
links: []
history:
  - date: 2026-05-15
    status: backlog
    who: karolkow
    note: 'Spawned from 0069 — Figma gaps found in libs/ui states while building the transactions list page.'
---

# libs/ui: align error/empty states with Figma

## Summary

While building the Transactions list page (task 0069), several `libs/ui`
state components were found to diverge from the Figma Design System
(file `siumLgKOc9...`, "Empty states & error states" node `8958:2893`).
Fixes were prepared on the 0069 branch but reverted — those files belong
to other tasks (0063 / ui-foundation, authored by FilipD) and should be
corrected at source rather than bundled into a page feature.

## Context

The transactions page composes around these gaps (e.g. its filtered-empty
state uses the generic `EmptyState` because `TableEmptyState` lacks a
filtered variant). Aligning `libs/ui` lets every list/detail page use the
shared components directly.

## Gaps Found

- **Error states copy/buttons** — `GenericErrorState` title is "Something
  went wrong" (Figma: "An unexpected error occurred"); `RateLimitState`
  description and button label differ; Figma uses "Back to home" buttons
  where the code hardcodes "Try again". `NotFoundState` has one generic
  description where Figma gives per-entity copy.
- **`TableEmptyState`** — no filtered-empty kind ("No transactions match
  your filters" + "Clear filters" action) and no `action` prop.
- **`EmptyState`** — icon container uses `borderRadius: 8`; Figma shows a
  circular container.
- **Error-state icons** — differ from Figma (info-circle / clock /
  warning-triangle).
- **No two-line `TimestampCell`** — `RelativeTimestamp` is single-line +
  tooltip; the DS table Time column is two lines (relative + absolute UTC).
  Task 0069 ships a local `TransactionTime` component as a stopgap.

## Implementation

- Update `libs/ui/src/states/errors/*` copy, icons and button affordances
  to match Figma node `8958:2893`.
- Add a filtered-empty kind + `action` prop to `TableEmptyState`.
- Round the `EmptyState` icon container.
- Add a two-line `TimestampCell` to `libs/ui/src/timestamps/`; migrate
  `web` consumers (including 0069's `TransactionTime`) onto it.

## Acceptance Criteria

- [ ] Error/empty state copy, icons and buttons match Figma `8958:2893`
- [ ] `TableEmptyState` supports a filtered-empty kind with an action
- [ ] `EmptyState` icon container is circular
- [ ] `libs/ui` exports a two-line `TimestampCell`; `web` uses it
- [ ] Coordinated with the owner of the ui-foundation / 0063 components
