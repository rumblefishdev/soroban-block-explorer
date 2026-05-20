---
id: '0244'
title: 'REFACTOR: API cleanup — usunięcie sqlx + PG path po wszystkich 9 modułach na CH default'
type: REFACTOR
status: backlog
related_adr: ['0046']
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
      Spawned z M1-M3 sequencing planu (2026-05-20). Follow-up post-0243 stable.
      Activate dopiero gdy wszystkie 9 modułów API są na CH default + 7 dni
      stable bez błędów. Wcześniejszy activate ryzykowny — sqlx + queries.rs
      potrzebne jako rollback path do każdego z modułów.
---

# API cleanup — usunięcie sqlx + PG path

## Summary

Po 0243 (API feature flag) i 7 dni stable wszystkich 9 modułów na `Ch` default:
usunąć `sqlx` z `crates/api/Cargo.toml`, usunąć per-module `queries.rs` (PG path),
uprościć `DataSource` enum (wykasować Pg wariant).

## Context

0243 zostawia oba path (PG + CH) za feature flagem dla bezpiecznego rollback per
module. Po finalnym flipie + observability window: kod do uproszczenia. Reduced
LOC, simpler handlers, mniejsza binary size, mniej dep maintenance.

## Implementation Plan

### Step 1: Verify stable signal

- Wszystkie 9 modułów na `ch` default w prod
- 7 dni bez error rate spike, bez latency regression
- Brak rollback per module w ostatnim tygodniu

### Step 2: Remove sqlx + queries.rs

1. `crates/api/Cargo.toml`: usunąć `sqlx = { workspace = true }`
2. Każdy moduł (9):
   - Usunąć `queries.rs` (PG path)
   - Przemianować `queries_ch.rs` → `queries.rs` (CH staje się single path)
   - Handler: usunąć `match DataSource` dispatch, wywoływać `queries::*` directly
3. `crates/api/src/common/datasource.rs`: usunąć enum lub uprościć do single-variant
4. Env var `API_DATASOURCE_*` — usunąć z env config + Lambda env

### Step 3: Verify no regression

- `cargo check -p api` clean
- `cargo nx test @rumblefish/api` pass
- Integration tests: 87 tests pass (cleaned up bez `Pg` path)
- Smoke test staging: API endpoints zwracają oczekiwane dane
- `cargo build --release -p api` size delta: expected reduction (sqlx removed)

### Step 4: Documentation update

- `docs/architecture/api/api-overview.md`: usunąć wzmianki o feature flag /
  PG fallback (jeśli były z 0243)
- `crates/api/README.md` (jeśli istnieje): update connection setup section

## Acceptance Criteria

- [ ] `cargo check -p api` clean bez `sqlx` dep
- [ ] No regression: 87 API integration tests pass
- [ ] Binary size reduction verifiable (`cargo bloat` lub `ls -lh` artifacts)
- [ ] LOC reduction: usunięto 9× `queries.rs` (PG path) — `git diff --stat` shows net negative
- [ ] `API_DATASOURCE_*` env vars usunięte z infra + Lambda config
- [ ] Smoke test staging API endpoints pass
- [ ] **Docs updated** — `docs/architecture/api/api-overview.md` (jeśli istnieje)
      reflects CH-only datastore; usunąć wzmianki PG fallback
- [ ] **API types regenerated** — N/A response shapes nie zmieniają się (już
      identyczne z 0243); sanity check `nx run @rumblefish/api-types:check-generated`

## Depends on

- **0243** wszystkie 9 modułów na `ch` default + 7 dni stable

## Notes

- Spawn dopiero po stable signal — nie przed.
- Może być prosty PR (głównie deletions); review focus na "no orphaned imports / dead code".
- Po tym tasku 0244 + 0239 Phase 6 (RDS drop, M3) = pełny PG cleanup w projekcie.
