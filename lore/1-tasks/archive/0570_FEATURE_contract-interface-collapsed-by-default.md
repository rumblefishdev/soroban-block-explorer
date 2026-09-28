---
id: '0570'
title: 'FEATURE: contract Interface tab — start with every function collapsed'
type: FEATURE
status: completed
related_adr: []
related_tasks: []
tags: ['frontend', 'effort-small']
links:
  - https://github.com/rumblefishdev/soroban-block-explorer/issues/467
history:
  - date: 2026-09-21
    status: active
    who: karolkow
    note: 'Task created from issue #467 triage (one-liner).'
  - date: 2026-09-21
    status: completed
    who: karolkow
    note: >
      Merged in PR #471, shipped in production-2026.09.21-1 (Compute + SPA,
      run 35652244312 green; only the three Lambda code assets changed across
      all stacks). Verified on production on the reported contract: 27 rows,
      0 expanded on load, 0 overflowing, click expands. Issue #467 stays open
      until the close comment is posted.
---

# FEATURE: contract Interface tab — start with every function collapsed

## Summary

The Interface tab on the contract detail page renders every function signature
expanded. On contracts with a large interface that is a long scroll before the
reader sees which functions exist. Start every row collapsed, so the first view
is the list of function names; the reader expands only what they need.

## Status: Completed

**Current state:** shipped and verified on production (2026-09-21).

## Context

`FunctionRow` in `web/src/pages/contracts/ContractInterface.tsx` passed
`defaultExpanded` to its MUI `<Accordion>`; the comment above it cited the Figma
panel as the reason. A user report asked for the collapsed view instead.

## Implementation Plan

1. Drop `defaultExpanded` from the `<Accordion>` in `FunctionRow`.
2. Update the comment above `FunctionRow`.
3. Check locally on the contract from the report.

## Acceptance Criteria

- [x] Interface tab opens with every function row collapsed; each row still
      expands and collapses on click.
- [x] Checked on the contract from the report
      (`CAD5W4MAEAFGRSARWE3TMWRWA4RVEIWCCMUOEQZPWEDIC6AZA4IAUVKA`).
- [x] No collapsed row overflows the card (long signatures ellipsize).
- [x] Live on production — `production-2026.09.21-1`; checked on
      https://sorobanscan.rumblefish.dev/contracts/CAD5W4MAEAFGRSARWE3TMWRWA4RVEIWCCMUOEQZPWEDIC6AZA4IAUVKA
      (before the deploy the same page showed 27/27 rows expanded and 2
      overflowing).
- [x] **Docs updated** — N/A: UI default state only, no change to the shape of
      the system.
- [x] **API types regenerated** — N/A: frontend-only, nothing under
      `crates/api/**`, `Cargo.{toml,lock}` or `libs/api-types/**`.

## Design Decisions

### From Plan

1. **Collapsed for every contract, no size threshold**: a "collapse only above
   N functions" rule was considered and rejected — more code, nobody asked for
   it. The user report overrides the Figma default.

### Emerged

2. **`minWidth: 0` on the summary content**: with every row collapsed, the
   summary line is the whole view, and a long signature (`deploy` on the
   reported contract) widened its row past the card (row `scrollWidth` 955 vs
   `clientWidth` 851) instead of ellipsizing — MUI's
   `.MuiAccordionSummary-content` defaults to `min-width: auto`, so the inner
   `minWidth: 0` / `text-overflow: ellipsis` never engaged. Pre-existing, not a
   regression; fixed in the same PR because the collapsed view is exactly
   where it shows.

## Implementation Notes

- `web/src/pages/contracts/ContractInterface.tsx`: removed `defaultExpanded`,
  rewrote the `FunctionRow` doc comment, added
  `'& .MuiAccordionSummary-content': { minWidth: 0 }` to the summary `sx`.
- Local check (Vite against the dev API proxy) on the reported contract:
  27 function rows, 0 expanded on load, 0 overflowing; first row expands on
  click and collapses on the second click.
- `nx typecheck` + `test` for the web project green (45 test files); eslint and
  prettier clean on the changed file.

## Issues Encountered

- **Worktree toolchain**: Nx failed with `Could not find ".modules.yaml"` — the
  worktree still had the pre-pnpm npm-layout `node_modules`; one
  `pnpm install` fixed it. Then typecheck false-failed on MUI theme
  augmentation (`Property 'surface' does not exist on type 'Palette'`) from a
  stale `web/dist/*.tsbuildinfo` that `nx reset` does not remove; moving
  `web/dist` to `.trash/` fixed it. Both environmental, unrelated to the diff.

## Notes

`ContractDetailPage.test.tsx` mocks `ContractInterface` entirely, so no existing
test covers the expanded state and none changes.
