---
id: '0389'
title: 'CI: cut ~14min pipeline — Rust/nx cache, concurrency, parallel lambda build'
type: PERF
status: active
related_adr: []
related_tasks: []
tags: ['area-ci', 'effort-small', 'priority-high', 'phase-1']
links:
  - '.github/workflows/ci.yml'
history:
  - date: 2026-07-14
    status: active
    who: karolkow
    note: >
      Spawned from CI failure/timing analysis. Measured (median of 24 runs):
      Rust job 847s (max 995s), TypeScript 468s, api-types 216s. Root cause:
      ZERO caching (Rust + nx) — every job compiles cold. Rust job recompiles
      the workspace ~4x (debug clippy → debug test → +swagger → release lambda).
      Biggest single step: `cargo lambda build --release --arm64` = 429s (51%).
---

# CI: cut ~14min pipeline — Rust/nx cache, concurrency, parallel lambda build

## Summary

CI critical path is ~14 min because nothing is cached — Rust and nx both
compile cold on every run. Add dependency caching, cancel superseded runs,
and stop serializing the release Lambda build behind clippy/test. Target:
~14 min → ~4-5 min critical path (~3x), no change to what CI validates.

## Status: Active

**Current state:** Phase 1 (rust-cache + concurrency) diff prepared, awaiting
review/commit. Phase 2 (parallel lambda split + `nx affected`) not started.

## Context

Measured from `gh run view` across the last 24 runs (current `ci.yml`):

| Job                                    | Median            | Max             |
| -------------------------------------- | ----------------- | --------------- |
| Rust (clippy, test, lambda build)      | **847s (14 min)** | 995s (16.6 min) |
| TypeScript (lint/build/typecheck/test) | 468s (7.8 min)    | 589s            |
| API types freshness                    | 216s (3.6 min)    | 233s            |
| Detect changes                         | 8s                | 10s             |

Rust job step breakdown (median):

| Step                                        | Time | Share |
| ------------------------------------------- | ---- | ----- |
| `cargo lambda build --release --arm64`      | 429s | 51%   |
| `cargo test`                                | 187s | 22%   |
| `cargo clippy --all-targets`                | 96s  | 11%   |
| `cargo test -p api --features swagger-ui`   | 85s  | 10%   |
| `cargo clippy -p api --features swagger-ui` | 41s  | 5%    |

**Root cause:** no `Swatinem/rust-cache`, no nx cache persistence. Every job
recompiles the full dependency tree (soroban, stellar-xdr, sqlx, axum, tokio…)
from scratch. The Rust job additionally recompiles the workspace ~4x across
profiles/features. Validation itself is correct and sufficient — the problem is
purely wasted compile time.

Separately: no `concurrency` group, so each push to a PR starts a fresh full CI
while the previous run keeps burning runners.

## Implementation Plan

### Phase 0 — re-land discarded develop work (clean, under this task)

Two good but uncommitted infra changes were sitting in develop's working tree
and were discarded (snapshotted) to redo them cleanly here:

0a. **`.husky/post-checkout`** — auto-symlinks `node_modules` from the primary
worktree on `git worktree add`, so husky's format gate actually runs in
fresh worktrees. This is the upstream cause of the format-check CI failures
(~90% of failed runs were `format:check` / `cargo fmt --check`). Empirically
verified: fires on `git worktree add` and provisions the symlink.
0b. **rust-fmt split** in `ci.yml` — move `cargo fmt --check` out of the heavy
Rust job into its own ~1s job, for fast red on the most common failure
without waiting on a full workspace build.

### Phase 1 — caching + concurrency (low risk, ~80% of the win)

1. **`Swatinem/rust-cache@v2`** on the `rust` job and the `api-types-codegen`
   job, sharing a `shared-key` so both reuse the registry/git cache. Caches
   `~/.cargo/{registry,git}` + `target/`. Expected: Rust 847s → ~280-350s
   (warm); api-types 216s → ~70s.
2. **`concurrency`** block at workflow level, `cancel-in-progress` on
   non-master refs (master stays protected — merges are rare and must not be
   cancelled).

### Phase 2 — restructure (more change, remaining win)

3. **Split `cargo lambda build --release` into its own parallel job.** Today
   the 5 cargo steps run serially (838s). Release build shares nothing with the
   debug clippy/test steps, so parallelizing halves the Rust critical path even
   before cache: `max(clippy+test+swagger ≈ 410s, lambda ≈ 429s) ≈ 429s` vs 838s.
4. **`nx affected` instead of `nx run-many`** on PRs (keep `run-many` on master),
   plus `actions/cache` on `.nx/cache`. TypeScript 468s → ~150-200s for a
   typical PR touching 1-2 projects.

## Acceptance Criteria

- [ ] Phase 0: `.husky/post-checkout` committed (tracked, shared) — fresh
      worktrees auto-provision `node_modules`; verified it fires on `worktree add`
- [ ] Phase 0: `rust-fmt` split into its own job in `ci.yml`
- [ ] Phase 1: `Swatinem/rust-cache@v2` added to `rust` + `api-types-codegen`
      jobs with shared key; cache hit observed on a second run
- [ ] Phase 1: `concurrency` block added; superseded PR runs auto-cancel; master
      not cancelled
- [ ] Rust job median drops materially on warm cache (target < ~400s)
- [ ] No change to validation coverage (clippy, test, swagger on/off, lambda
      artifact verification all still run)
- [ ] Phase 2 (parallel lambda split + `nx affected` + nx cache) — tracked here,
      implement after Phase 1 hit-rate is confirmed
- [ ] **Docs updated** — N/A: CI tooling/process only, does not change the shape
      of the system (schema, API, ingestion, infra topology per ADR 0032)
- [ ] **API types regenerated** — N/A: touches only `.github/workflows/ci.yml`,
      no `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**` changes

## Notes

- Files touched: `.husky/post-checkout` (Phase 0a) and
  `.github/workflows/ci.yml` (Phase 0b + Phase 1). No app code, no API/types.
- Phased deliberately: land Phase 0 (redo discarded develop work) + Phase 1
  (two small non-structural ci.yml insertions) first; measure real cache
  hit-rate before the Phase 2 restructure.
- Snapshots of the discarded develop work (post-checkout, rust-fmt-split ci.yml)
  are preserved in the session scratchpad to re-create byte-exact.
