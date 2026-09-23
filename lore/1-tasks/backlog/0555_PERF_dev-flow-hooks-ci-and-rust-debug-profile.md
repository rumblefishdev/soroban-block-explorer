---
id: '0555'
title: 'PERF: dev-flow efficiency — docs-only CI, git hooks, TS job cache, Rust debug profile'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0389', '0480', '0532']
tags: ['area-ci', 'area-dx', 'priority-medium', 'effort-medium']
links:
  - '.github/workflows/ci.yml'
  - '.husky/pre-commit'
  - '.husky/pre-push'
  - 'Cargo.toml'
history:
  - date: '2026-09-15'
    status: backlog
    who: karolkow
    note: >
      Filed from a local audit of disk use and build time across worktrees
      and CI. Four worktree `target/` dirs reached 11–19 GiB each; most
      develop pushes are docs/lore-only yet pay the full hook and CI cost.
---

# PERF: dev-flow efficiency

## Summary

Cut the time and disk every commit, push and CI run pays for work that did
not change code. Four independent changes, one PR each or one combined PR.

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

- [ ] A lore-only push to `develop` triggers no Rust/TS/API-types jobs
- [ ] A docs-only `git push` does not invoke `cargo`
- [ ] Empty/irrelevant `git commit` hook overhead measured before/after
- [ ] TS job runs only affected projects; cache hit shown on a re-run
- [ ] Workspace `target/` size + clean build time measured before/after the profile change
- [ ] **Docs updated** — N/A — CI/tooling only, no change to described architecture
- [ ] **API types regenerated** — `Cargo.toml` changes → run
      `pnpm nx run @rumblefish/api-types:generate`; expect an empty diff

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
  declared-vs-emitted test (task 0455) reads `crates/**`, so that project
  needs an explicit dependency on those files or it stops running on a
  Rust-only change.
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
