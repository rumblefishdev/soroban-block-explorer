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
  - date: 2026-07-15
    status: active
    who: karolkow
    note: >
      Implemented Phases 0-2 in PR #340 (6 commits). Warm-cache measurement:
      critical path 847s->481s (1.75x); Rust 847s->301s (-64%, cache + parallel
      lambda); api-types 216s->111s (-49%). rust-cache cold-vs-warm proof:
      clippy+test 435s->210s (-52%). nx affected + nx cache tried then reverted
      (affected = run-everything choice; nx cache = no-op, nx.json marks no
      cacheable targets). TypeScript now the bottleneck (481s), left as run-many.
---

# CI: cut ~14min pipeline — Rust/nx cache, concurrency, parallel lambda build

## Summary

CI critical path is ~14 min because nothing is cached — Rust and nx both
compile cold on every run. Add dependency caching, cancel superseded runs,
and stop serializing the release Lambda build behind clippy/test. Target:
~14 min → ~4-5 min critical path (~3x), no change to what CI validates.

## Status: Active — implementation merged (PR #340 landed on develop)

**Result (warm cache, measured):** critical path **847s → 481s (~1.75×)**;
Rust side **847s → 301s (−64%)**; api-types 216s → 111s (−49%). TypeScript is
now the long pole (481s) and unchanged — see Design Decisions.

**In PR #340 (6 commits):** rust-fmt split · rust-cache (SHA-pinned v2.9.1) ·
concurrency · `.husky/post-checkout` (worktree provisioning) · parallel lambda
job · dev debuginfo → line-tables.

**Tried and reverted:** `nx affected` (chose full `run-many` always for
coverage) and `.nx/cache` (measured no-op — see Issues Encountered). Phase 3
(validation hardening) deferred.

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
4. **`nx affected` + `.nx/cache`** — BOTH REVERTED. `nx affected` reverted by
   choice (run full `run-many` always for coverage, not just affected).
   `.nx/cache` measured a no-op (nx.json declares no cacheable targets → empty
   ~1MB cache, no replay + I/O overhead: TS 481s→585s→621s). See Issues.
5. **Rust debug info → line-tables** (`CARGO_PROFILE_DEV_DEBUG=1` + `RUST_BACKTRACE=1`):
   faster codegen + smaller cached target; backtraces keep file:line for readable logs.

### Phase 3 — validation hardening (scope widened from perf-only; future)

CI perf is near its ceiling; the remaining leverage is catching MORE bugs, not
running faster. Ranked for this project (a block explorer fails at the parser,
SQL drift, dep vulns, chain-data fidelity — not where CI currently looks):

6.  **Fuzz the XDR parser** (proptest/cargo-fuzz) — chain data is adversarial; a
    parser must survive malformed input without panicking. ⭐⭐⭐
7.  **Contract test vs real chain** — parse a fixed ledger range, diff against
    Horizon / stellar.expert (see `compare-with-stellar-api` skill); red on parser
    drift from chain truth. ⭐⭐⭐
8.  **`.sqlx` freshness gate** — like `API types freshness`: assert the offline
    query cache matches the schema so `SQLX_OFFLINE` drift can't become a runtime 500. ⭐⭐⭐
9.  **`cargo deny` / `audit`** — block known-vulnerable deps (GitHub flagged 36, 2
    critical) + license policy. Triage/allowlist the existing set first; start
    non-blocking. ⭐⭐
10. **Coverage gate** (cargo-llvm-cov / vitest) — fail when new code lands
    untested / coverage regresses. ⭐⭐
11. **Secret scanning** (gitleaks) — catch committed keys/tokens; public repo. ⭐⭐
12. **Merge queue** — test each PR against latest trunk before merge so a green PR
    can't break `develop` in combination. Needs every job's `if:` updated for
    `merge_group` + the queue enabled in branch protection. ⭐⭐
13. **Flake detector** / **FE visual+a11y regression** (Playwright + axe). ⭐

## Acceptance Criteria

- [x] Phase 0: `.husky/post-checkout` committed (tracked, shared) — fresh
      worktrees auto-provision `node_modules`; verified it fires on `worktree add`
- [x] Phase 0: `rust-fmt` split into its own job in `ci.yml` (~16s)
- [x] Phase 1: `Swatinem/rust-cache` (SHA-pinned v2.9.1) on `rust` +
      `api-types-codegen`; warm-cache hit proven (clippy+test 435s→210s)
- [x] Phase 1: `concurrency` block added; superseded PR runs auto-cancel
      (observed live: run dc7b7a22 cancelled); master not cancelled
- [x] Rust job drops materially on warm cache (target < ~400s) — clippy+test
      210s, lambda 301s, both < 400s
- [x] No change to validation coverage (clippy, test, swagger on/off, lambda
      artifact verification all still run — unchanged, just reorganized)
- [x] Phase 2: parallel lambda split — done. `nx affected` + nx cache — reverted
      (affected = run-everything choice; nx cache = measured no-op)
- [ ] Phase 3 (validation hardening) — deferred, tracked above as future work
- [x] **Docs updated** — N/A: CI tooling/process only, does not change the shape
      of the system (schema, API, ingestion, infra topology per ADR 0032)
- [x] **API types regenerated** — N/A: touches only `.github/workflows/ci.yml` +
      `.husky/post-checkout`, no `crates/api/**`, `Cargo.{toml,lock}`, `libs/api-types/**`

## Notes

- Files touched: `.husky/post-checkout` and `.github/workflows/ci.yml`. No app
  code, no API/types.
- Phased deliberately: land Phase 0 + Phase 1 first, measure warm-cache
  hit-rate before the Phase 2 restructure.

## Design Decisions

### From Plan

1. **rust-cache SHA-pinned (not `@v2` tag)** — matches the repo's third-party
   action convention (`dorny/paths-filter@sha`); prevents a supply-chain swap.
2. **`rust-fmt` split into its own job** — fmt is compile-free; fast red on the
   most common CI failure without waiting on the heavy build.
3. **Parallel `rust-lambda` job with a distinct cache key** (`ci-rust-lambda`) —
   release `target/` would thrash the debug job's `ci-rust` cache on the same arm64.

### Emerged

4. **debuginfo=1 (line-tables), not 0** — user's call: keep file:line in panic
   backtraces (readable CI logs) while trimming full debug info. Added
   `RUST_BACKTRACE=1` so failures print the location.
5. **`nx affected` reverted for full `run-many` always** — user chose full
   coverage over speed on the TS side; master already ran full, PRs now do too.
6. **`.nx/cache` reverted** — measured no-op (see Issues); not carried as dead config.
7. **Secret scanning via native GitHub push-protection, not a CI job** — user
   chose the server-side setting (un-bypassable, no install) over a gitleaks CI
   job. The `secret-scan` job was added then dropped.

## Issues Encountered

- **CI flake on the first PR run** — the TypeScript job failed once with a purged
  log + empty step conclusion (infra/cancellation pattern, not a code error). A
  re-run of the same commit went green. Not a regression.
- **`.nx/cache` no-op** — nx.json declares no cacheable targets (`targetDefaults`
  has only `test`, no `cache: true`), so nx cached nothing (~1MB) and re-ran
  everything. TS got slower (481s→585s→621s) from cache I/O with no replay.
  Effective nx caching needs an nx.json change (mark targets cacheable + declare
  inputs/outputs) — deferred to Phase 3 (app-config, correctness risk).
- **Vestigial `SQLX_OFFLINE`** — the rust job sets `SQLX_OFFLINE=true` but the
  code has zero compile-checked sqlx macros (all runtime `sqlx::query`, backend is
  ClickHouse). The flag gates nothing. Left as-is; removable.
- **`nx affected` re-added by a `git pull`** — after a local drop of that commit,
  a `git pull` fast-forwarded it back (it was already on origin); resolved by
  force-push with the user's explicit consent.

## Future Work

Phase 3 (validation hardening) above. Top 3 for this project: fuzz the XDR parser,
contract-test vs chain (golden fixtures), and — if ClickHouse gains a query check
— a CH query smoke test. Tracked here; not yet spawned as separate backlog tasks.
