---
id: '0109'
title: 'BUG: hotfixes for 3 P1 issues found by Codex review on develop'
type: BUG
status: active
related_adr: []
related_tasks: ['0029', '0067', '0099']
tags: [priority-high, effort-small, layer-infra, layer-frontend]
links: []
history:
  - date: 2026-04-07
    status: active
    who: stkrolikiewicz
    note: >
      Codex review on PR #70 (fix/0106) flagged 3 P1 bugs already on
      develop, unrelated to 0106 itself. All three are blockers for
      downstream work and need to land before more features.
---

# BUG: hotfixes for 3 P1 issues found by Codex review on develop

## Summary

While running `/codex:review` on PR #70 (the SPA cache TTL fix), Codex inspected the entire branch diff against master and found three P1 issues that were already merged to develop in earlier tasks. None of them are caused by 0106 — they are pre-existing bugs that block downstream tasks. Hot-fix sweep before continuing with feature work.

## The Three Bugs

### Bug 1 — `__dirname` in ESM Vite configs (blocks frontend build)

`web/vite.config.ts:5` and `libs/ui/vite.config.ts:5` use `__dirname` at module top-level. The workspace `package.json` has `"type": "module"`, so Vite loads these configs as ESM and `__dirname` is undefined at runtime → `ReferenceError: __dirname is not defined`. `nx dev` and `nx build` for both projects fail before Vite can start.

**Blocks:** all UI/frontend tasks (0058–0086).

**Fix:** ESM equivalent — `dirname(fileURLToPath(import.meta.url))`.

### Bug 2 — Indexer Lambda env var name mismatch (broken in production)

`crates/indexer/src/main.rs:24-27` reads `DB_SECRET_ARN` and `RDS_ENDPOINT` env vars. But `infra/src/lib/stacks/compute-stack.ts:86-89` (and the rest of the project — `db-migrate`, `db-partition-mgmt`) sets `SECRET_ARN` and `RDS_PROXY_ENDPOINT`. The indexer Lambda will fail every cold start in any deploy that doesn't provide `DATABASE_URL` directly, which is the normal CDK path.

**Blocks:** indexer running on staging/production. Critical for production launch.

**Fix:** rename in indexer crate to match the rest of the project (`SECRET_ARN`/`RDS_PROXY_ENDPOINT`). The other naming was introduced by task 0029 in isolation, missing the existing convention from `db-migrate` and `db-partition-mgmt`.

### Bug 3 — `envs/*.json` in tsconfig include outside rootDir (breaks typecheck)

`infra/tsconfig.lib.json` sets `rootDir: "src"` but `include` lists `envs/**/*.json`, which is a sibling of `src`. `tsc` rejects with `TS6059: File is not under rootDir`. Currently masked because the project's typecheck target is cached in nx, but a fresh build or nx target rerun fails.

**Blocks:** infra typecheck, CI workflow (0039) on any PR touching infra.

**Fix:** remove `envs/**/*.json` from include. The bin scripts (`bin/staging.ts`, `bin/production.ts`) load envs via runtime `createRequire(import.meta.url)('../../envs/staging.json')`, not via TypeScript JSON import — so tsc doesn't need to see them.

## Acceptance Criteria

- [ ] `nx dev web` and `nx dev @rumblefish/soroban-block-explorer-ui` start without `ReferenceError`
- [ ] `cargo check -p indexer` passes; CDK env vars in compute-stack match what indexer reads
- [ ] `nx typecheck @rumblefish/soroban-block-explorer-aws-cdk` passes after `nx reset`
- [ ] No regression in existing synth or build targets

## Notes

- Each bug is fixed in its own commit so individual fixes can be reverted/cherry-picked
- All three are pre-existing on develop, not introduced by this task or by 0106
- Spawned from /codex:review run on PR #70
