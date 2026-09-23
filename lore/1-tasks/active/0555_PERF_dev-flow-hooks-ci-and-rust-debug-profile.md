---
id: '0555'
title: 'PERF: dev-flow efficiency — docs-only CI, git hooks, TS job cache, Rust debug profile'
type: PERF
status: active
related_adr: []
related_tasks: ['0389', '0455', '0480', '0532']
tags: ['area-ci', 'area-dx', 'priority-medium', 'effort-medium']
links:
  - '.github/workflows/ci.yml'
  - '.husky/pre-commit'
  - '.husky/pre-push'
  - '.github/workflows/cleanup-pr-caches.yml'
  - 'Cargo.toml'
history:
  - date: '2026-09-15'
    status: backlog
    who: karolkow
    note: >
      Filed from a local audit of disk use and build time across worktrees
      and CI. Four worktree `target/` dirs reached 11–19 GiB each; most
      develop pushes are docs/lore-only yet pay the full hook and CI cost.
  - date: '2026-09-23'
    status: active
    who: karolkow
    note: >
      Every step approved, plus a cache cleanup at pull request close; one
      pull request, branch perf/0555_dev-flow.
---

# PERF: dev-flow efficiency

## Summary

Cut the time and disk every commit, push and CI run pays for work that did
not change code. Four independent changes, delivered in one pull request
(#484), plus a cache cleanup at pull request close.

## Context

Measured 2026-09-15 (audit run locally, figures below are from it unless
marked estimate):

1. **Docs-only pushes run the full CI.** `ci.yml` uses `dorny/paths-filter`,
   but on a push to `develop` the open develop→master PR re-runs all six
   jobs against the whole PR diff, not the pushed commit. One run ≈ 21
   job-minutes / ~10 min wall. 5 of 6 sampled runs were triggered by
   lore-only commits; 307 of the last 548 develop commits touched only docs.
2. **`pre-push` runs `cargo clippy --all-targets` unconditionally**
   (`.husky/pre-push:8`), including docs-only pushes. ~20 s warm in CI; in a
   fresh worktree it builds `target/` from scratch (estimate: minutes).
3. **CI TypeScript job runs `pnpm nx run-many -t lint build typecheck test`**
   (`ci.yml:88`) over every project with no Nx cache; ~7 min, triggered by
   any `crates/` change. Playwright browser download (47 s) is uncached.
4. **`pre-commit` costs ~11.4 s with nothing staged** — Nx starts three
   times before the staged list is checked; any staged `.rs` adds ~14 s.
5. **Root `Cargo.toml` has no `[profile.*]`.** Dev builds carry full debug
   info for every dependency. Cargo's own guidance is
   `debug = "line-tables-only"` plus `debug = false` for `package."*"`.
   Measured on `xdr-parser` alone: −19–22 % `target/` size, −19 % CPU time
   for test binaries. Workspace-wide effect is an estimate.

Also seen, out of scope here: GitHub Actions cache at 9.88/10 GiB (per-PR
1.67 GiB Rust caches, none saved for `develop`, x86 vs arm keys never
shared); `stellar-xdr` compiled twice (v26 + v28).

Rejected: a shared `CARGO_TARGET_DIR` across worktrees. Cargo's freshness
check is mtime-based; a worktree with 22 changed source files was reported
`Fresh` with zero crates rebuilt (upstream rust-lang/cargo#12516; the fix,
`-Zchecksum-freshness`, is nightly-only on 1.97). `sccache` saves CPU, not
disk, and never caches workspace members.

## Implementation Plan

### Step 1: CI skips docs-only pushes

Evaluate the paths filter against the pushed range (`base: ${{ github.event.before }}`
on push) or add `paths-ignore` for `lore/**`, `docs/**`, `**/*.md`.
Keep required checks satisfiable (skipped ≠ missing).

### Step 2: hooks skip what the change cannot affect

`pre-push`: run clippy only when the pushed range touches `crates/**`,
`Cargo.{toml,lock}`. `pre-commit`: exit before starting Nx when nothing
relevant is staged.

### Step 3: TypeScript job uses `nx affected` + cache

`nx affected -t lint build typecheck test` with a correct base, cache
`.nx/cache` and Playwright browsers.

### Step 4: Rust dev profile

Add to root `Cargo.toml`:

```toml
[profile.dev]
debug = "line-tables-only"

[profile.dev.package."*"]
debug = false

[profile.debugging]
inherits = "dev"
debug = true
```

Measure `target/` size and clean build time before/after on the whole
workspace; record both.

## Acceptance Criteria

- [ ] A lore-only push to an open pull request triggers no Rust/TS/API-types
      jobs after a green run — logic dry-run against real commits through the
      API; to be seen on this pull request's own docs push
- [x] A docs-only `git push` does not invoke `cargo` — hook run through
      `git hook run pre-push` on three ranges (docs-only and a new branch at
      the develop tip: no clippy; an unknown remote sha: clippy)
- [x] Empty/irrelevant `git commit` hook overhead measured before/after —
      the Nx part: 11.4 s → 0.07 s
- [x] TS job runs only affected projects; Playwright cache hit shown on a
      re-run — run 35855921396: the job took 3 min 01 s (10 min 41 s on the
      0573 pull request) and restored `ms-playwright-Linux-1.62.1`
- [x] Workspace `target/` size + clean build time measured before/after the
      profile change — below
- [x] **Docs updated** — N/A — CI/tooling only; no file under `docs/`
      describes the hooks or these CI commands
- [x] **API types regenerated** — empty diff

## Decided (karolkow, 2026-09-23), with what was measured for it

Every step is approved. Two items are new, and one reverses an earlier
decision.

- **`pre-push` runs clippy only for a push that carries Rust** (step 2).
  Measured 2026-09-22: pushing a single `.md` took 4 min 26 s, all of it the
  unconditional `cargo clippy --all-targets`, in a worktree whose `target/`
  had gone stale.
- **`pre-commit` returns before Nx starts when nothing relevant is staged**
  (step 2). The script asks Nx three times which projects have `lint`,
  `typecheck` and `test` _before_ it looks at the staged list.
- **CI skips the Rust, TypeScript and API-types jobs for a docs-only push**
  (step 1).
- **The TypeScript job runs only the affected projects** (step 3). This
  reverses task 0389's decision 5, "full `run-many` always". The infra
  declared-vs-emitted test (task 0455) reads `crates/**`; it keeps running on
  a Rust-only change because those files are already one of its inputs (see
  Implementation).
- **The Playwright browser is cached** (step 3): 47 s per run today.
- **New — a workflow deletes a pull request's caches when it closes**, and a
  one-off prune runs first. Measured 2026-09-23 through `gh api`: 9.73 GB of
  the 10 GB cache is in use and 5.9 GB of it is dead — 3.8 GB belongs to two
  pull requests merged on 2026-09-16, 2.1 GB to an older `Cargo.lock` on
  `master`. A pull request keeps writing its own cache while it is open; the
  cleanup at close is what reclaims the space. `develop` still needs no cache
  of its own, because a pull request reads the default branch's.
- **The debug profile lands in `Cargo.toml`** (step 4), and `ci.yml`'s
  `CARGO_PROFILE_DEV_DEBUG: '1'` goes with it, so one place states it for
  every checkout. Nothing in the repository configures a debugger — no
  `launch.json`, no mention of `lldb` or `gdb` — and CI has built with
  trimmed debug info since task 0389. The `debugging` profile stays for the
  day someone wants variables.

## Implementation (2026-09-23)

Branch `perf/0555_dev-flow`, one pull request.

- **`.husky/pre-push`** reads the pushed refs from stdin and runs clippy only
  when the range touches `crates/`, `Cargo.toml`, `Cargo.lock` or
  `rust-toolchain.toml`. A new branch, or a remote commit this clone lacks,
  is compared from its merge base with `origin/develop`.
- **`tools/scripts/run-affected-checks.mjs`** (later removed, see below) lost
  its target discovery (three
  `nx show projects` calls on every commit and push; `nx affected` skips a
  target no project has anyway). A commit of `.md` files alone exits before
  Nx starts.
- **`ci.yml`, job `changes`**: on a `synchronize` event, when the previous
  commit's CI run succeeded and both the pushed range and the base branch's
  changes since that commit are documents only (`lore/`, `docs/`, `*.md`),
  the three outputs go `false`. The step fails open: any error means a full
  run.
- **`ci.yml`, job `typescript`**: a pull request runs `nx affected`, master
  `run-many`. Playwright's browser is cached per
  Playwright version, and the e2e steps run only when the web project is
  affected.
- **`.github/workflows/cleanup-pr-caches.yml`** deletes a pull request's
  caches when it closes.
- **`Cargo.toml`** carries the debug profile; `ci.yml` drops
  `CARGO_PROFILE_DEV_DEBUG`.

### Measured

Clean `cargo test --workspace --no-run`, own target dir each, same laptop:

|        | `target/`       | wall         | CPU (user)  |
| ------ | --------------- | ------------ | ----------- |
| before | 6.42 GiB        | 200 s        | 720 s       |
| after  | 4.11 GiB (−36%) | 146 s (−27%) | 668 s (−7%) |

Other sessions compiled on the same machine during both runs, so the times
are indicative; the size is not affected.

### Found on the way

- **The TypeScript job compiled the whole Rust workspace.** The Nx project
  `rust` (`rust/project.json`, no code of its own) wrapped `cargo build`,
  `cargo test` and `cargo clippy`; `run-many` ran all three, in parallel, on
  the x86 runner with no Rust cache. Measured on the 0573 pull request's run
  (2026-09-22): 469 crates compiled, the three finished after 4 min 51 s,
  5 min 09 s and 6 min 33 s of a 10 min 41 s job. The Rust jobs already run
  all three. **Removed (karolkow, 2026-09-23)** rather than excluded: locally
  a change to `package.json` or `nx.json` affects every project, so the
  `pre-push` hook's `nx affected` would still have run the whole cargo test
  suite through it. Nothing else used it; the one runbook line that named
  `nx run rust:lint` now names cargo. This answers task 0480's question 6.
- **The infra dependency on `crates/` needed no change.** The infra project's
  `test` target already lists `{workspaceRoot}/crates/**/*.rs` as an input,
  and Nx 22 counts inputs for `affected`: a change to one `.rs` file affects
  `@rumblefish/soroban-block-explorer-aws-cdk` and nothing else.
- ~~**`.nx/cache` stays uncached**, unlike step 3's proposal: task 0389
  measured it a no-op (no target in `nx.json` is cacheable) and slower.~~
  Wrong, see "Nx cache in CI" below.
- **The develop→master pull request is open only around a release.** The
  docs-only skip pays most on feature pull requests that are up to date with
  their base. One branched from an older develop sees develop's code changes
  in the base-side compare and runs in full — the safe side of not knowing
  which base the previous run merged with (the runs API keeps no base sha
  for closed pull requests).
- **Husky prepends `~/.cargo/bin`** through `~/.config/husky/init.sh`, so a
  stub `cargo` on `PATH` does not intercept the hook's clippy; the hook test
  hit the real one on its last range, which is the expected branch.

## Review (2026-09-23)

A two-axis review (standards, spec) of #484 found two ways the docs-only
skip could skip CI for a push that changed code. Both fixed (karolkow: fix
now, in this pull request):

- **A force-push.** `compare/A...B` lists what B changed since its merge base
  with A, not the difference between their trees; a force-push that drops a
  code commit and adds a document read as documents only. The pushed commit
  must now be `ahead` of the tested one, or the run is full.
- **A swallowed API error.** The two compares ran in one loop inside `$(…)`,
  whose status is the last call's; a failed pushed-range compare left only
  the base's list. Each compare is now its own assignment, so an error fails
  the step, and a failed step is a full run.

Tested against the API: an `ahead` docs-only range lists its files, a
`behind` one reads `not-comparable`. Locally `grep` is `ugrep`, whose `-q -v`
answers differently from GNU and BSD grep; the decision was checked with
`/usr/bin/grep` (CI runs GNU grep).

Fixed in passing: `local`/`remote` renamed `local_sha`/`remote_sha` in the
hook (`local` is a shell builtin); `Cargo.toml`'s comment carries the
workspace measurement instead of the single-crate one; the cleanup workflow's
job has a name and fails when the cache listing fails.

Decided (karolkow):

- ~~**`.nx/cache` stays off** (plan step 3): no target in `nx.json` is
  cacheable, and task 0389 measured it slower (481 s → 621 s).~~ Taken on a
  wrong premise and reversed, see "Nx cache in CI" below.
- **The task stays a single file** although it is over the ~150-line mark.

Left as judgement calls: "documentation" is defined as `lore/`, `docs/`,
`*.md` in CI and as `*.md` in the commit hook (each fits its job); the
e2e condition repeats on five steps.

### `run-affected-checks.mjs` rewritten (karolkow: rewrite in this pull request)

The script came from a generic template (March 2026) and was never fitted to
this repository. Three findings:

- **A new branch's first push checked almost everything.** Without an
  upstream the base fell back to `origin/HEAD`, which is `origin/master`, 99
  commits and 146 files behind `develop`. Measured on #484: the first push ran
  lint, typecheck and test for all 4 projects, the next (with an upstream) for
  1. The fallback is now `origin/develop`, as in `pre-push`.
- **Half the file was fallbacks for branches that do not exist here**
  (`origin/main`, `main`, `HEAD~1`, `HEAD`) and three helpers that served only
  them. 139 lines → 67.
- **A staged deletion ran no checks.** Only added and modified files were
  passed on (`--diff-filter=ACMR` plus `existsSync`), so deleting a file
  another one imports went unnoticed until CI. Nx maps a path to its project
  whether or not the file exists (`web/src/does-not-exist.ts` → web), so every
  staged path is passed on now.

Checked through `pnpm run -s verify:staged` / `verify:push` (the hooks'
commands; bare `node` has no `nx` on its PATH): nothing staged exits in
0.06 s; a staged deletion of `infra/scripts/deploy-scope.sh` checks the infra
project; a push with an upstream compares with it; a branch without one
compares with `origin/develop`. On this branch that still reaches all 4
projects — deleting `rust/project.json` changes the project graph, which
Nx counts against every project; `Cargo.toml`, `ci.yml` and the script
itself reach none.

### The script removed (karolkow: hooks call Nx directly)

The rewritten script still only glued two things that already do the work:
Nx takes a file list (`--stdin`) or a commit range (`--base`/`--head`) and
finds the projects itself, and git hands `pre-push` the exact pushed range —
which the hook already read for clippy while the script guessed the same
range a second way, through the upstream. The published Nx hook setups call
`nx affected` straight from the hook, with lint-staged for formatting only.

- **`pre-commit`**: lint-staged formats; the staged paths (deletions
  included, `.md` excluded) go to `nx affected --stdin`; an empty list starts
  no Nx.
- **`pre-push`**: one range per pushed ref — from the remote sha, or from the
  merge base with `origin/develop` for a new branch or a remote commit this
  clone lacks — drives both `nx affected --base --head` and the clippy
  decision.
- `tools/scripts/run-affected-checks.mjs` and the `verify:staged` /
  `verify:push` scripts are gone. The `/pr` skill's verify step named them
  and a `format:staged` script that never existed, and told to amend; it now
  says the hooks run the checks and a failure is fixed in a new commit.

Rejected: a lint-staged function for `pre-commit` (lint-staged passes no
deleted files by default, feeding them in would hand them to prettier too,
and it splits long lists so Nx could run several times); moving to lefthook
(declarative, but a rebuild of the whole hook setup — its own task).

Checked through `git hook run`: nothing staged, 0.3 s; a staged deletion of
`infra/scripts/deploy-scope.sh` checks the infra project; a docs-only pushed
range runs no tasks and no clippy; an unknown remote sha falls back to
develop; a range touching `crates/` checks the infra project and runs clippy.

### Nx cache in CI (karolkow: add it here and measure)

The premise behind keeping `.nx/cache` off was wrong. `nx.json` marks no
target cacheable, but the Nx plugins infer `cache: true` for what they
create: `lint`, `typecheck`, `test` and `build` all carry it in the web and
infra projects (`nx show project … --json`), and a local commit replayed 13
of 13 tasks from the cache. Task 0389's measurement most likely predates the
inferred targets.

Nx 22 needs two directories for a hit, per its own source
(`node_modules/nx/src/tasks-runner/cache.js`, `assertCacheIsValid`): the
outputs in `.nx/cache`, and the records of them in a database under
`.nx/workspace-data`, named after the machine id. Outputs without their
record read as "Unrecognized Cache Artifacts", a warning, and every task
runs. The TypeScript job therefore caches both. Whether a GitHub runner keeps
its machine id from run to run decides whether this pays; if it does not,
the step comes out again before merge.

**Measured, and removed.** Run 35855921396 saved both directories; its
rerun restored them by the exact key and Nx answered "Unrecognized Cache
Artifacts" — a GitHub runner does not keep its machine id, so the records
never match and nothing replays. The step is gone. A shared Nx cache in CI
needs a remote cache (Nx Cloud, or the self-hosted HTTP cache Nx 22
supports through `NX_SELF_HOSTED_REMOTE_CACHE_SERVER`), not `actions/cache`.

**Found on the rerun: `web:build` races `web:typecheck`.** The rerun failed
in `vite build` with `ENOTEMPTY: directory not empty, rmdir
'web/dist/pages'`. `typecheck` runs `tsc --build --emitDeclarationOnly`
with `tsconfig.lib.json`'s `outDir: dist`, which writes 237 `.d.ts` files
into `web/dist` — the directory `vite build` empties at start. Nx runs the
two in parallel, so a declaration landing mid-cleanup fails the build. It
predates this task (`run-many` ran them in parallel too); the first run of
the same commit passed.

**Fixed (karolkow: fix in this pull request).** `web/tsconfig.lib.json` and
`web/tsconfig.spec.json` write to `out-tsc/` instead of `dist/`; nothing reads
web's declarations (it is an app), so only `vite build` writes `web/dist`
now. `out-tsc` joins the ESLint and Prettier ignores, as `dist` is (it is
already in `.gitignore`); without that, lint picked up a warning from a
generated `.d.ts` and a local `format:check` failed on all 237 of them.
Checked from a clean `web/dist`: typecheck and build in parallel, both green;
`web/dist` holds no `.d.ts`, `web/out-tsc` 237; web lint back to its 4
warnings, 376 tests pass, `format:check --all` clean.

`libs/ui` has the same shape — typecheck and `vite build` (`emptyOutDir:
true`) share `libs/ui/dist` — but there the declarations are what web
compiles against, so moving them is a change to how the library is consumed,
not a path fix. Not touched here.
