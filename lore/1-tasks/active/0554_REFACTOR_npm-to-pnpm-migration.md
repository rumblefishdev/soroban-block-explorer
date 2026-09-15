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

**Current state:** all steps done and every acceptance criterion met locally
(implementation commit `aef8b641`). Remaining: the first CI run on `develop`
proves the rewritten workflows; the first release proves
`deploy-production.yml`.
Task 0532 (worktree provisioning) absorbed here and archived as superseded.

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

- [x] `pnpm install --frozen-lockfile` succeeds from an empty `node_modules`;
      no `package-lock.json` left
- [x] Resolved versions unchanged — `cdk synth` output identical to the npm
      baseline; web build output identical (one transitive build-tool bump,
      decision 5)
- [x] `nx run-many -t lint build typecheck test` green for the 4 TS projects;
      web e2e green; `api-types:check-generated` green. `rust:*` targets not
      run locally (no Rust change beyond a doc comment; pre-push clippy + CI)
- [x] CI workflows (`ci.yml`, `deploy-production.yml`, `deploy-board.yml`) use
      pnpm; no `npm ci` / `cache: npm` remains
- [x] A new worktree gets its own `node_modules`; `@rumblefish/*` resolve to
      the worktree's `libs/`, proven with main parked on a different branch
- [x] A new worktree rejects a deliberately malformed staged file (hooks run)
- [x] No `npx` / `npm ci` / `npm run` left in repo-owned scripts, hooks, CI,
      docs (Nx-generated agent config excluded, decision 12)
- [x] **Docs updated** — `docs/deployment.md` (build prerequisites, `cdk`
      invocation), `docs/runbooks/live-tail-cutover.md`;
      `docs/architecture/**` N/A — tooling only, system shape unchanged
- [x] **API types regenerated** — N/A — `crates/api/**` change is a doc
      comment only; `libs/api-types/project.json` command change proven by a
      green `check-generated` (generated output unchanged)

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

5. **One transitive version moved: `brace-expansion` 2.0.3 → 2.1.1.**
   `pnpm import` re-resolved `filelist` → `minimatch@5.1.9` →
   `brace-expansion@^2.0.1` and deduplicated it with the 2.1.1 `glob` already
   used. Build tooling only, inside semver range; accepted rather than pinned
   with an override.
6. **Build allow-list is `@swc/core`, `esbuild`, `nx` only.** npm's lockfile
   also flagged `core-js-pure` (bundled inside `cargo-lambda-cdk`, so pnpm
   never installs it separately) and `fsevents@2.3.3` (no install script in
   its manifest). Neither needs an entry.
7. **Root `package.json` scripts call bare `nx`, not `pnpm nx`.** `pnpm run`
   puts `node_modules/.bin` on `PATH`, so there is no registry fallback to
   avoid there; `pnpm exec` / `pnpm nx` is used everywhere a script is not run
   through `pnpm run` (hooks, Makefile, CI, `project.json`, docs).
8. **`deploy-production.yml` loses its `node_modules` cache.** It cached the
   root `node_modules` only; under pnpm each workspace package (`web/`,
   `infra/`, `libs/*`) has its own, so a cache hit would restore half a tree
   and skip the install. `setup-node` now caches the pnpm store and the install
   always runs (link-only on a warm store).
9. **`pnpm/action-setup` pinned to a commit SHA** (`0977fd9…`, v6.0.10,
   released 2026-08-03, no advisories) — same safety-over-recency rule as
   decision 4; the repo already SHA-pins `Swatinem/rust-cache`.
10. **`pnpm-lock.yaml` replaces `package-lock.json` in `.prettierignore`.**
    Without it lint-staged's `nx format:write --files` would rewrite the
    lockfile on every dependency commit.
11. **`post-checkout` does not auto-repair an npm-era symlinked
    `node_modules`; it prints the one-line repair.** Moving a directory out
    from under a checkout inside a hook is surprising; the repair is
    documented in the `worktree-hooks` skill too.
12. **Codex/Nx-generated agent config left untouched.** `.agents/skills/**`,
    `.codex/config.toml` (`npx` for the Nx MCP server), `.nx/nxw.js` and the
    Nx boilerplate in `AGENTS.md`/`CLAUDE.md` are generated by Nx tooling and
    are not repo-owned scripts.
13. **Hooks hand over to the checkout's own script (scope widened on
    request).** Each `.husky/{pre-commit,pre-push,post-checkout}` starts with
    `own="$(git rev-parse --show-toplevel)/.husky/$(basename "$0")"` and
    `exec`s it unless `[ "$0" -ef "$own" ]`; a branch without that hook exits 0.
    Changing the worktree `hooksPath` instead is impossible: `.husky/_` is
    generated and absent in a fresh worktree, so no hook of its own could run
    to create it. Probe-tested: main copy → own copy runs with arguments;
    main-checkout relative invocation → runs itself; absent hook → exit 0;
    exit code 7 propagates. Bootstrap limit: live only after the main
    checkout is on a branch containing it.

## Implementation Notes

- **Step 1 baseline (npm):** `web` build output (59 files, sha256 list),
  `cdk synth` of `dist/bin/production.js` run directly with
  `CDK_CONTEXT_JSON='{"aws:cdk:bundling-stacks":[]}'` (no Rust bundling, no
  AWS calls), and the set of 1130 `name@version` pairs from
  `package-lock.json`.
- **Step 2 swap:** `pnpm import` (21 s), then
  `pnpm install --frozen-lockfile` (1 min 31 s, cold store). The pinned
  10.34.5 ran even though the shell's `pnpm` was 10.13.1 —
  `managePackageManagerVersions` confirmed.
- **Version proof:** 1105 packages in `pnpm-lock.yaml`. Of 25 pairs present
  only in the npm set, 24 are `inBundle` (shipped inside `aws-cdk-lib` /
  `cargo-lambda-cdk` tarballs, which pnpm does not list); the 25th is
  decision 5.
- **Output proof:** after the swap, `web` build output is byte-identical
  (all 59 sha256 match) and the full `cdk.out` directory is byte-identical
  to the npm baseline.
- **Step 3–5 call sites:** `pnpm exec <bin>` verified to resolve the same
  binaries from each package directory (`openapi-ts` 0.97.0 and `prettier`
  2.8.8 from `libs/api-types`, `cdk` 2.1116.0 from `infra`, `playwright`
  1.62.1 and `vite` 7.3.3 from `web`). `tools/scripts/worktree-node-modules.sh`
  removed (copy in the main checkout's `.trash/`).

- **Step 6 verification:** `nx run-many -t lint build typecheck test
--skip-nx-cache` for api-types, ui, aws-cdk, web — green (ui 86 tests, web
  369, aws-cdk 5; lint 0 errors). Web e2e 3/3. `check-generated` green.
- **Commit gate:** the pre-commit hook ran lint, typecheck and test for all
  5 projects, `rust` included (`cargo check`, `clippy`, `test --workspace`) —
  green, 5 min 4 s on a cold worktree `target/`.
- **Fresh worktree** (`git worktree add --detach` at `aef8b641`, hooks off
  during the add; main checkout parked on
  `feat/0374_lp-native-leg-and-soroban-amm-completeness`):
  - `post-checkout` started from another checkout's copy, as a main
    checkout's would be: handed over, ran `pnpm install` — **14 s** on a warm
    store (npm-era clone script: 3 min 12 s);
  - `web/node_modules/@rumblefish/{soroban-block-explorer-ui,api-types}`
    resolve (`pwd -P`) inside the new worktree's `libs/`;
  - `nx typecheck` for web green in the new worktree;
  - `git commit` of a staged `fn broken( {` with `core.hooksPath` set to
    another checkout's `.husky/_`: rejected by rustfmt (`unclosed
  delimiter`), exit 1, nothing committed, lint-staged restored the index.
    Temporary worktree moved to the main checkout's `.trash/`, pruned.

## Issues Encountered

- **First verification run failed on a full disk, not on the migration.**
  The volume hit 191 MiB free (sibling worktrees' `target/` dirs: 15, 10,
  6.8 GB …). Cargo and the nx cache hit `ENOSPC`, and 19 web tests timed out
  (5 s) while sharing the machine with a workspace build and a concurrent
  `cargo clippy`. Rerun alone after space was freed: 369/369.

- **Worktree clone is not ~20 s.** `tools/scripts/worktree-node-modules.sh`
  took 3 min 12 s to provision this worktree before the baseline could be
  taken (the symlinked tree resolved `@rumblefish/*` into the main checkout,
  parked on another branch).
- **`git mv` cannot target `.trash/` outside the worktree.** Lockfile copied
  to the main checkout's `.trash/`, then `git rm`.
- **Worktrees run the main checkout's hook scripts, not their own.** Worktree
  `core.hooksPath` is the main checkout's absolute `.husky/_`; husky's `h`
  wrapper runs `$(dirname "$(dirname "$0")")/<hook>`, i.e. the main checkout's
  `.husky/<hook>`. Every worktree's commit gate is therefore coupled to the
  branch the main checkout is parked on — the same coupling 0532 recorded for
  `node_modules`. Consequence here: the new `post-checkout`/`pre-commit` go
  live in worktrees only once the main checkout is on a branch containing
  them. Fixed in this task — decision 13.
- **Local AWS/CDK guard hook blocked a file-editing script** because its source
  text contained `cdk`. Nothing was executed; the same text edits were made
  with the editor tool instead.

## Notes

- Version landscape on 2026-09-15 (npm registry dist-tags): `latest-10`
  10.34.5, `latest-11` 11.26.0, `latest` 12.4.2 (12.0.0 released
  2026-08-26). v11 requires Node ≥ 22.13 (repo: 22.22.0), reads only
  auth/registry from `.npmrc`, and defaults `minimumReleaseAge` to 1 day.
- pnpm 10.13.1 (a typical older local install) is affected by several
  high-severity advisories fixed in 10.34.x (lockfile integrity bypass,
  path traversal on install, lifecycle-script allow-list bypass). pnpm 10
  honours `packageManager` and switches to the pinned version itself
  (`managePackageManagerVersions`, default on) — confirmed in Step 2.
