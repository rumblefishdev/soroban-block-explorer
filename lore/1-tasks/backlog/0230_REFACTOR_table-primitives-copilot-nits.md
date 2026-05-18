---
id: '0230'
title: 'libs/ui table primitives: address Copilot review nits from 0061'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0061']
tags: [priority-low, effort-small, layer-frontend-shared, phase-future]
milestone: 2
links: ['https://github.com/rumblefishdev/soroban-block-explorer/pull/190']
history:
  - date: 2026-05-18
    status: backlog
    who: karolkow
    note: 'Spawned from 0061 — Copilot review on merged PR #190.'
---

# libs/ui table primitives: address Copilot review nits from 0061

## Summary

Three valid Copilot review comments landed on PR #190 (task 0061) after
merge. All still valid against current `develop`. Small, low-risk
follow-ups to the `libs/ui/src/table/` primitives.

## Context

PR #190 (table primitives + cursor hooks) merged with a late Copilot
review. None of the comments blocked merge; collected here so they are
not lost.

## Implementation

1. **`useTableUrlState.ts` — unstable `filterKeys` memo dep.**
   `filterKeys` (a `readonly string[]`) is a `useMemo` dependency. A
   caller inlining `useTableUrlState({ filterKeys: ['type','status'] })`
   passes a new array reference every render → memo always recomputes →
   `state` / `state.filters` get a new identity every render → downstream
   `useEffect`/`useMemo` on `state` re-run every render.
   Fix: derive a stable key internally (e.g. `filterKeys.join(',')`) and
   depend on that, or document that callers must memoize `filterKeys`.

2. **`useCursorPagination.ts` — `goNext` / `goPrev` identical.**
   Both delegate to `setCursor(cursor)` (correct — cursor is opaque and
   direction-encoded), but two distinct functions imply asymmetry that
   does not exist. Fix: add a short comment explaining why both delegate,
   or collapse to `goTo(cursor)` with `goNext`/`goPrev` as thin aliases.

3. **`ExplorerTable.tsx` — blank gap on empty table without `emptyState`.**
   When `rows` is empty and no `emptyState` prop is given, a full-width
   `TableCell` with an empty `Box` (`py: 6`) still renders — a ~96px
   blank gap. Fix: skip the empty row when `emptyState` is undefined, or
   render a minimal default placeholder.

## Acceptance Criteria

- [ ] `useTableUrlState` does not recompute `state` when `filterKeys`
      is passed inline with stable contents
- [ ] `useCursorPagination` documents or removes the `goNext`/`goPrev`
      duplication
- [ ] `ExplorerTable` renders no blank gap when empty and `emptyState`
      is omitted
- [ ] `libs/ui` typecheck + lint green

## Notes

- Pure frontend-shared refactor; no schema / API / docs-architecture
  change (ADR 0032 N/A).
