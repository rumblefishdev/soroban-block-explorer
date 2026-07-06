---
id: '0244'
title: 'REFACTOR: remove Postgres/sqlx entirely — CH is the only DB (API collapse + dead crates + dev tools)'
type: REFACTOR
status: active
related_adr: ['0047']
related_tasks: ['0243', '0239', '0318']
tags:
  [
    priority-medium,
    effort-large,
    layer-api,
    layer-backend,
    layer-ops,
    cleanup,
    refactor,
    clickhouse,
  ]
milestone: 3
links:
  - crates/api/Cargo.toml
  - crates/api/src/
  - Cargo.toml
  - infra/src/lib/stacks/compute-stack.ts
history:
  - date: '2026-05-20'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from M1-M3 sequencing plan (2026-05-20). Follow-up after 0243
      reaches a stable signal. Activate only once all 9 API modules are on
      CH default + 7 days stable with no errors.
  - date: '2026-06-09'
    status: backlog
    who: stkrolikiewicz
    note: >
      Moved to milestone 3. Team decision: only the PG cleanup is deferred to M3;
      the base per-module CH-read (0243) stays in M2 (launch-critical — prod has
      no PG). In production the sqlx/queries.rs path is NOT a rollback path — RDS
      is decommissioned (0239), prod is CH-only — it survives purely for local-dev
      parity.
  - date: '2026-07-06'
    status: backlog
    who: karolkow
    note: >
      Scope broadened from "crates/api only" to full Postgres removal across the
      whole codebase. Gates cleared: 0243 (API feature flag, all 9 modules on CH)
      and 0239 (RDS decommission) both completed. Team decision: delete PG
      COMPLETELY — including the dev/bench tools that intentionally kept a
      pg-persist path (backfill-runner, backfill-bench, audit-harness, db-merge).
      No pg variant stays in live code. sqlx and the "postgres" workspace feature
      are dropped entirely. Recon map produced this session (see Scope below);
      search CH path verified real (not a stub — 0318 completed, 880 lines).
  - date: '2026-07-06'
    status: active
    who: karolkow
    note: >
      Promoted to active. Starting the full PG removal (buckets A+B+C).
---

# Remove Postgres/sqlx entirely — ClickHouse is the only DB

## Summary

Prod is already CH-only (RDS decommissioned, ADR 0047 / task 0239). All PG
code is now dead weight or migration-era dev tooling. Team decision: remove
Postgres from the **entire codebase** — API dual-backend collapse, dead
PG-only crates, migration files, infra placeholder, docker-compose PG
services, and the dev/bench tools that kept a `pg-persist` path. After this
task the words `sqlx`, `PgPool`, `postgres` appear in no live crate.

## Context

0243 put all 9 API modules behind an `API_DATASOURCE_<MODULE>` flag (PG↔CH)
for safe rollback; all 9 now run on `ch` in prod and 0243 is completed. 0239
removed RDS. So the PG path has no prod role. The dev tools (backfill/bench/
audit/merge) only ever talked to a _local_ docker Postgres — a migration-era
convenience nobody needs post-cutover. All of it goes.

## Scope — recon map (2026-07-06)

Three buckets, all → DELETE.

### A. Dead PG-only components (delete outright)

- `crates/db-migrate/` — CloudFormation Lambda that ran RDS migrations; CDK no
  longer invokes it.
- `crates/db-partition-mgmt/` — RDS table partitioner CLI.
- `crates/db/migrations/` — 36 Postgres DDL SQL files (no CH equivalent; CH
  schema is static `crates/db-clickhouse/schema/init.sql`).
- `crates/db/{migrate.rs, secrets.rs, pool.rs}` — migration runner, RDS creds
  resolver, `PgPool` builder. Assess whether `crates/db` survives at all after
  these go.
- `infra/.../compute-stack.ts` — remove the `DATABASE_URL` disabled placeholder
  (crates/api/src/common/datasource.rs:81 area) and the `API_DATASOURCE_*` env
  block (~lines 306–371) once the API boots without a PgPool.

### B. API dual-backend collapse (crates/api)

- 9 modules (accounts, assets, contracts, ledgers, liquidity_pools, network,
  nfts, search, transactions): delete `queries.rs` (PG), rename
  `queries_ch.rs` → `queries.rs`. All 9 CH paths verified present (search =
  880 lines, real, 0318).
- Handlers: drop the `match DataSource` dispatch, call `queries::*` directly.
- `crates/api/src/common/datasource.rs` — delete the `DataSource` enum + the
  `API_DATASOURCE_` env plumbing.
- `crates/api/src/state.rs` — remove `pub db: PgPool` from `AppState` + its
  construction in `main.rs`.
- `tests_integration.rs` — port PG fixtures to CH (or drop PG-only cases).

### C. Dev/bench tools on `pg-persist` (delete — team decision)

- `crates/backfill-runner/` — drop `--target postgres` sink + the PG features
  dep; CH-only. (Confirm the whole crate stays useful CH-only vs. is retired.)
- `crates/backfill-bench/` — local PG benchmark; retire (targeted a local PG).
- `crates/audit-harness/` — `horizon-diff`/`archive-diff`/`operations-order-diff`
  diffed against old PG data; retire.
- `crates/db-merge/` — snapshot merge over PG FDW; retire.
- `crates/indexer/` — remove the `pg-persist` cargo feature (Cargo.toml:32) and
  every `#[cfg(feature = "pg-persist")]` block, incl. `handler/persist/` (the
  15-step PG write tree).
- `docker-compose.yml` — remove the Postgres 16 service(s) (`postgres`,
  `postgres-merge`, `postgres-snapshot-source`, `db-merge` profile).

### Cross-cutting

- Workspace `Cargo.toml:43` — drop the `sqlx` dependency (or at minimum remove
  the `"postgres"` feature; expect `sqlx` gone entirely once B+C land).
- `crates/domain/Cargo.toml` — remove the optional `sqlx` feature + the
  `#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]` enum derives.

## Implementation Plan

Suggest 2–3 PRs for review sanity (one task, staged):

1. **PR1 — API collapse (B):** remove PG queries + DataSource enum + PgPool from
   AppState. API boots CH-only. Remove `DATABASE_URL` placeholder + `API_DATASOURCE_*`
   from infra in the same PR.
2. **PR2 — dead crates + tools (A + C):** delete the 5+ PG-only crates, the
   `pg-persist` feature, migrations dir, docker PG services.
3. **PR3 — cross-cutting:** drop `sqlx` from workspace + domain; final
   `rg -i 'sqlx|pgpool|postgres'` sweep = zero hits in live code.

Deletions go to `.trash/` per repo policy, not `rm`.

## Acceptance Criteria

- [ ] `rg -i 'sqlx|PgPool|PgConnection|postgres' crates/ infra/ libs/` returns
      zero hits outside comments/lore/docs archive
- [ ] `sqlx` absent from workspace `Cargo.toml` and every crate `Cargo.toml`
- [ ] `cargo check --workspace` clean; no orphaned imports / dead code
- [ ] API integration tests pass on CH fixtures (PG fixtures ported or dropped)
- [ ] `API_DATASOURCE_*` + `DATABASE_URL` removed from infra + Lambda config
- [ ] docker-compose has no Postgres service
- [ ] Deleted crates removed from workspace members + CDK
- [ ] Staging smoke: all API endpoints return expected data
- [ ] **Docs updated** — `docs/architecture/**`: remove PG/RDS from DB layer,
      ingestion, and infra topology docs (ADR 0032 requires it); ADRs stay as
      historical record
- [ ] **API types regenerated** — sanity check
      `nx run @rumblefish/api-types:check-generated` (response shapes unchanged)

## Depends on

- **0243** — all 9 modules on `ch` default ✅ completed
- **0239** — RDS decommissioned ✅ completed

## Notes

- Mostly deletions — review focus is "no orphaned imports, no dead code, no
  stray sqlx".
- ADRs and archived lore tasks keep their PG references (historical record) —
  do NOT touch them.
- After this task the PG cleanup for the whole project is complete.
