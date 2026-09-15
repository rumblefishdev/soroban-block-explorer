---
id: '0554'
title: 'REFACTOR: migrate the JS workspace from npm to pnpm — lockfile, CI, hooks, per-worktree installs'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0532']
tags: [tooling, dx, worktree, ci, effort-medium, priority-medium]
links: []
history:
  - date: '2026-09-15'
    status: active
    who: karolkow
    note: >
      Task created. Scope mapped before any code: lockfile + workspace
      manifest, 3 CI workflows, 3 husky hooks, `npx` call sites (root
      scripts, infra Makefile, api-types codegen, web e2e), docs, and the
      worktree `node_modules` provisioning that pnpm's store makes obsolete.
---

# Migrate the JS workspace from npm to pnpm

## Summary

Replace npm with pnpm for the Nx/TypeScript workspace (`libs/*`, `infra`,
`web`). Main payoff: every git worktree gets its own real `node_modules`
cheaply (pnpm links from one global store), which removes the symlink /
clone provisioning hack and the class of bugs where a worktree silently
typechecks against the main checkout's `libs/`. Package versions must not
change — this is a package-manager swap, not a dependency upgrade.

## Status: Active

**Current state:** scope mapped, decisions taken (below), implementation not
started. Pending: fate of task 0532.

## Context

- npm workspaces materialise `node_modules/@rumblefish/*` as relative
  symlinks. Worktrees symlink `node_modules` to the main checkout, so those
  links resolve into main's `libs/` — the defect recorded in task 0532.
  The current fix (`tools/scripts/worktree-node-modules.sh`, APFS clone) is a
  workaround for npm's layout, not a cure.
- `npm ci` run in a worktree whose `node_modules` is a symlink empties the
  main checkout's tree.
- pnpm keeps one content-addressed store and links packages into each
  project, so a per-worktree install is seconds, not minutes.

## Findings (pre-implementation)

1. **Undeclared imports resolve.** `web/` imports `react`, `@mui/material`,
   `react-router-dom`, `@tanstack/react-query`, `prismjs` without declaring
   them; all are declared in the root `package.json`, and Node walks up to the
   root `node_modules`. `libs/ui` and `infra` declare what they import.
   To be proven by build, not by this reading.
2. **Versions can be preserved.** `pnpm import` converts `package-lock.json`
   into `pnpm-lock.yaml` keeping resolved versions. Proof: `cdk synth`
   templates and `vite build` output compared before/after.
3. **Build scripts are blocked by default.** pnpm only runs dependency
   install scripts for packages explicitly allowed (`@swc/core`, `esbuild`,
   `nx`, …). Setting name depends on the pinned major: `onlyBuiltDependencies`
   (v10) vs `allowBuilds` (v11+).
4. **Worktree hooks run the main checkout's wrappers.** Worktree
   `core.hooksPath` points at the main checkout's `.husky/_`. During the
   transition a pnpm worktree can execute hook wrappers installed by an npm
   main checkout — verify hook behaviour from a worktree, not only from main.
5. **Transition breaks npm worktrees.** Once the main checkout's
   `node_modules` becomes a pnpm layout, worktrees still symlinked to it on
   npm branches lose their tree. Every developer machine needs pnpm.

## Implementation Plan

### Step 1: Baseline

Capture `cdk synth` templates and `web` production build output on npm.

### Step 2: Package manager swap

- `pnpm import` → `pnpm-lock.yaml`; move `package-lock.json` to `.trash/`.
- `workspaces` → `pnpm-workspace.yaml`; allow-list build scripts.
- `packageManager` field in root `package.json`.
- Workspace deps `"*"` → `"workspace:*"`.

### Step 3: Call sites

- `npx` / `npm run` / `npm exec` → `pnpm exec` / `pnpm nx` / `pnpm run` in
  root scripts, husky hooks, `infra/Makefile`, `libs/api-types/project.json`,
  `web/package.json`, `web/playwright.config.ts`, code comments.
- CI: `ci.yml`, `deploy-production.yml`, `deploy-board.yml` — pnpm setup,
  cache, `pnpm install --frozen-lockfile`, path filters on the lockfile.

### Step 4: Worktree provisioning

- `.husky/post-checkout`: replace the symlink with
  `pnpm install --frozen-lockfile --prefer-offline`.
- Move `tools/scripts/worktree-node-modules.sh` to `.trash/`.
- `.claude/skills/worktree-hooks/SKILL.md`: keep the "never `--no-verify`"
  rule; replace the provisioning / "never symlink" sections with the pnpm
  one-liner.

### Step 5: Docs and agent config

`docs/deployment.md`, `CLAUDE.md` (API types codegen command),
`lore/1-tasks/_template.md`, `.claude/settings.json` permissions,
`README.md`, `web/README.md`, `infra/README.md`,
`docs/runbooks/live-tail-cutover.md`, `.claude/skills/{pr,issues}`.

### Step 6: Verify

`format:check`, `lint build typecheck test`, web e2e, api-types
`check-generated`, synth/build comparison against Step 1, hooks from a fresh
worktree with main parked on another branch.

## Acceptance Criteria

- [ ] `pnpm install --frozen-lockfile` succeeds on a clean clone; no
      `package-lock.json` left
- [ ] Resolved versions unchanged — `cdk synth` templates identical to the npm
      baseline; web build output identical or every difference explained
- [ ] `nx run-many -t lint build typecheck test` green; web e2e green;
      `api-types:check-generated` green
- [ ] CI workflows (`ci.yml`, `deploy-production.yml`, `deploy-board.yml`) use
      pnpm; no `npm ci` / `cache: npm` remains
- [ ] A new worktree gets its own `node_modules`; `@rumblefish/*` resolve to
      the worktree's `libs/`, proven with main parked on a different branch
- [ ] A new worktree rejects a deliberately malformed staged file (hooks run)
- [ ] No `npx` / `npm ci` / `npm run` left in repo-owned scripts, hooks, CI,
      docs (vendored `.agents/skills/**` excluded)
- [ ] **Docs updated** — `docs/deployment.md` (build prerequisites);
      `docs/architecture/**` N/A — tooling only, system shape unchanged
- [ ] **API types regenerated** — N/A — no change under `crates/api/**`,
      `Cargo.{toml,lock}`; `libs/api-types/project.json` command changes
      only, `check-generated` proves output unchanged

## Design Decisions

### From Plan

1. **Strict (default isolated) linker, not `node-linker=hoisted`.** Strict
   dependency resolution is the point of the migration. Fall back to hoisting
   only for a specific tool that provably breaks, and record it here.
2. **Per-worktree `pnpm install` replaces symlink/clone provisioning.**
   `worktree-node-modules.sh` and the symlinking `post-checkout` are removed;
   the `worktree-hooks` skill keeps its no-bypass rule and loses the
   npm-layout sections.
3. **All `npx` call sites move to `pnpm exec` / `pnpm nx`.** `npx` falls back
   to downloading from the registry when a local bin is missing, which hides
   a broken install.
4. **Pin pnpm 10.34.5 exactly, with its hash, via `packageManager`**
   (`pnpm@10.34.5+sha512.…`), installed in CI by `pnpm/action-setup` reading
   that field. Chosen for safety over recency:
   - final patch of the v10 line (line ~20 months in use, patch released
     2026-07-10, ~2 months without a follow-up fix);
   - no published pnpm security advisory affects it (31 advisories checked
     2026-09-15; the newest, 2026-08-02/03, are fixed in 10.34.5 and 11.11.0);
   - v12 is 3 weeks old; v11 ships a minor release every few days.
     Cost: moving to v11 later renames the build allow-list setting
     (`onlyBuiltDependencies` → `allowBuilds`) and moves any `.npmrc` settings
     into `pnpm-workspace.yaml`.

### Emerged

## Notes

- Version landscape on 2026-09-15 (npm registry dist-tags): `latest-10`
  10.34.5, `latest-11` 11.26.0, `latest` 12.4.2 (12.0.0 released
  2026-08-26). v11 requires Node ≥ 22.13 (repo: 22.22.0), reads only
  auth/registry from `.npmrc`, and defaults `minimumReleaseAge` to 1 day.
- pnpm 10.13.1 (a typical older local install) is affected by several
  high-severity advisories fixed in 10.34.x (lockfile integrity bypass,
  path traversal on install, lifecycle-script allow-list bypass). pnpm 10
  honours `packageManager` and switches to the pinned version itself
  (`managePackageManagerVersions`, default on) — to be confirmed with
  `pnpm -v` inside the repo during Step 2.
