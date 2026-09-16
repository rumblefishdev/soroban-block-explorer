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
