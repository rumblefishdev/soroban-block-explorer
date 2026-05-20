---
id: '0244'
title: 'REFACTOR: API cleanup — remove sqlx + PG path after all 9 modules on CH default'
type: REFACTOR
status: backlog
related_adr: ['0047']
related_tasks: ['0243']
blocked_by: ['0243']
tags:
  [
    priority-medium,
    effort-medium,
    layer-api,
    layer-backend,
    cleanup,
    refactor,
    clickhouse,
  ]
milestone: 2
links:
  - crates/api/Cargo.toml
  - crates/api/src/
history:
  - date: '2026-05-20'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from M1-M3 sequencing plan (2026-05-20). Follow-up after 0243
      reaches a stable signal. Activate only once all 9 API modules are on
      CH default + 7 days stable with no errors. Activating earlier is risky
      — `sqlx` + `queries.rs` are still needed as the per-module rollback
      path.
---

# API cleanup — remove sqlx + PG path

## Summary

After 0243 (API feature flag) and 7 days of stable signal across all 9
modules on `Ch` default: remove `sqlx` from `crates/api/Cargo.toml`, drop the
per-module `queries.rs` (PG path), and simplify the `DataSource` enum
(remove the `Pg` variant).

## Context

0243 keeps both paths (PG + CH) behind a feature flag for safe per-module
rollback. After the final flip + observability window, the code can be
simplified. Outcomes: reduced LOC, simpler handlers, smaller binary size,
fewer deps to maintain.

## Implementation Plan

### Step 1: Verify stable signal

- All 9 modules on `ch` default in prod
- 7 days with no error rate spike, no latency regression
- No per-module rollback in the last week

### Step 2: Remove sqlx + queries.rs

1. `crates/api/Cargo.toml`: remove `sqlx = { workspace = true }`.
2. For each of the 9 modules:
   - Delete `queries.rs` (PG path)
   - Rename `queries_ch.rs` → `queries.rs` (CH becomes the single path)
   - Handler: drop the `match DataSource` dispatch and call `queries::*`
     directly
3. `crates/api/src/common/datasource.rs`: remove the enum (or simplify to a
   single-variant marker).
4. Env var `API_DATASOURCE_*` — remove from env configs + Lambda
   environment.

### Step 3: Verify no regression

- `cargo check -p api` clean
- `cargo nx test @rumblefish/api` passes
- Integration tests: 87 tests pass (cleaned up without the `Pg` path)
- Staging smoke: API endpoints return the expected data
- `cargo build --release -p api` size delta: expected reduction (sqlx removed)

### Step 4: Documentation update

- `docs/architecture/api/api-overview.md`: remove references to the feature
  flag and the PG fallback (if any from 0243).
- `crates/api/README.md` (if present): update the connection setup section.

## Acceptance Criteria

- [ ] `cargo check -p api` clean with no `sqlx` dep
- [ ] No regression: 87 API integration tests pass
- [ ] Binary size reduction verifiable (`cargo bloat` or `ls -lh` artifacts)
- [ ] LOC reduction: 9× `queries.rs` (PG path) removed —
      `git diff --stat` shows a net negative
- [ ] `API_DATASOURCE_*` env vars removed from infra + Lambda config
- [ ] Staging smoke API endpoints pass
- [ ] **Docs updated** — `docs/architecture/api/api-overview.md` (if it
      exists) reflects the CH-only datastore; references to the PG fallback
      removed
- [ ] **API types regenerated** — N/A — response shapes do not change (they
      already match after 0243); sanity check
      `nx run @rumblefish/api-types:check-generated`

## Depends on

- **0243** — all 9 modules on `ch` default + 7 days stable

## Notes

- Spawn only after a stable signal — never before.
- This should be a small PR (mostly deletions); review focus is on "no
  orphaned imports, no dead code".
- After this task plus 0239 Phase 6 (RDS drop, M3), the project completes
  the PG cleanup.
