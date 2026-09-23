---
id: '0578'
title: 'BUG: libs/ui typecheck and vite build write the same dist in parallel'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0555', '0577']
tags: ['area-ci', 'area-dx', 'priority-medium', 'effort-small']
links:
  - 'libs/ui/tsconfig.lib.json'
  - 'libs/ui/vite.config.ts'
history:
  - date: '2026-09-23'
    status: backlog
    who: claude
    note: >
      Spawned from 0555 future work. The same race was fixed for web in
      0555; 0577 hit it in libs/ui.
---

# BUG: libs/ui typecheck and vite build write the same dist in parallel

## Summary

`libs/ui`'s `typecheck` (`tsc --build --emitDeclarationOnly`, `outDir: dist`)
and its `build` (`vite build`, `emptyOutDir: true`) both write
`libs/ui/dist`, and Nx runs them in parallel. A build that empties the
directory while tsc writes into it leaves `dist` half-populated or fails.

## Context

Task 0555 fixed the same race in `web`: its CI rerun failed with
`ENOTEMPTY … rmdir web/dist/pages`, and web's tsc output moved to
`web/out-tsc`. Nothing read web's declarations, so that was a path fix.

`libs/ui` is different: `web/tsconfig.lib.json` references
`../libs/ui/tsconfig.lib.json`, so web compiles against the `.d.ts` files
ui's typecheck emits into `libs/ui/dist`. Task 0577 hit the race there:
`libs/ui/dist` held `.d.ts.map` files without their `.d.ts`, and
`web:typecheck` failed on MUI palette augmentation until `dist` was removed
and the targets re-run serially.

## Implementation

- Decide where ui's declarations live: a directory of their own (e.g.
  `libs/ui/out-tsc`) that vite never empties, with every consumer
  (project references, `package.json` `types`/`exports`) pointing there.
- Or make vite stop owning the directory tsc writes (`emptyOutDir: false`
  plus a separate clean step), if the declarations must stay in `dist`.
- Check `libs/api-types` for the same shape.

## Acceptance Criteria

- [ ] `nx run-many -t typecheck build -p @rumblefish/soroban-block-explorer-ui`
      from a clean `libs/ui/dist`, repeated several times, never fails and
      leaves every `.d.ts.map` beside its `.d.ts`
- [ ] `web:typecheck` passes against the new layout
- [ ] **Docs updated** — N/A — build tooling only
