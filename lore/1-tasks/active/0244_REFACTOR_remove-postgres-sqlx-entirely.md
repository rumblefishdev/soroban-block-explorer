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
  - date: '2026-07-06'
    status: active
    who: karolkow
    note: >
      Big session (26 commits). DONE: standalone dead-crate deletes
      (db-migrate, db-merge+scripts, db/migrations+migrate.rs, docker
      PG-merge services); FULL API collapse — all 9 endpoint modules on CH,
      DataSource enum + datasource.rs deleted, state.db/PgPool removed,
      config ch_enabled gone, api dropped the db + sqlx deps; 3 conditional-GET
      tests ported to CH; the 5448-line tests_integration.rs dropped to a
      follow-up (task 0360 on develop). Plus post-collapse cleanup: killed 7
      single-variant error enums, kept ledger/transaction domains isolated,
      and Option C — renamed every queries_ch.rs->queries.rs and moved the
      internal query row/param types out of dto.rs back into the query layer.
      See "Progress" section for the exhaustive remaining list. Gated on two
      pending team decisions: audit-harness (port-to-CH vs delete) and
      backfill-bench (drop vs port).
---

# Remove Postgres/sqlx entirely — ClickHouse is the only DB

## Progress (2026-07-06)

### ✅ Done (26 commits on `refactor/0244_remove-postgres-sqlx-entirely`)

**Standalone dead-crate deletes** (CH mirror verified per artifact):

- `crates/db-migrate/` (RDS migration Lambda) → `.trash/`
- `crates/db-merge/` + 3 merge scripts (FDW snapshot merge; CH does it via
  `backfill-runner/ch_staging` EXCHANGE TABLES) → `.trash/`
- `crates/db/migrations/` (35 SQL DDL) + `crates/db/src/migrate.rs` +
  `pub mod migrate` (the `sqlx::migrate!()` embed had zero callers) → `.trash/`
- `docker-compose.yml`: removed `postgres-merge` / `postgres-snapshot-source`
  (db-merge profile) + their volumes + `x-postgres-base` anchor

**API dual-backend collapse (bucket B) — all 9 modules CH-only:**
accounts, assets, contracts, ledgers, liquidity_pools, network, nfts, search,
transactions. Each: PG `queries.rs` deleted, `queries_ch` = sole path,
`match DataSource` dispatch removed, PG-only helpers gone (contracts
`expand_events`/`ParsedLedger`/`fetch_unique_ledgers`/archive overlay),
`*Cursor::Pg` variants removed, `EventAppearanceRow`/`HashIndexRow`
(PG-fold-only) dropped.

**API teardown:**

- `common/datasource.rs` (`DataSource`/`Module` enum + `API_DATASOURCE_*`
  plumbing) deleted
- `AppState.db: PgPool` removed; `ch` is now non-optional
  `clickhouse::Client`; `main.rs` builds the mTLS CH client unconditionally,
  no PG pool / `db::secrets`
- `config.ch_enabled` removed
- `crates/api/Cargo.toml` dropped the `db` and `sqlx` deps + `domain/sqlx`
  feature; last sqlx uses (network `FetchStatsError::Pg`, ledgers
  `LedgerListItem: sqlx::FromRow`) removed
- `common/head::latest_sequence_pg` deleted; `current_head_opt` CH-only

**Tests:** ledgers/network/transactions conditional-GET tests ported from
`DATABASE_URL`/`PgPool` to `CH_URL`/`clickhouse::Client` (shared
`common::ch::test_client_from_env`). `tests_integration.rs` (5448 lines of
PG-SQL fixtures) moved to `.trash/` → **follow-up task 0360** (on develop):
"rebuild API integration tests on ClickHouse fixtures".

**Post-collapse cleanup refactors:**

- removed 7 single-variant `*FetchError` enums (return `clickhouse::error::Error`)
- reverted a bad `LedgerTxRow`↔`TxListRow` unification to keep the ledger and
  transaction domains isolated (wire `TransactionListItem` stays shared — API
  contract)
- **Option C**: renamed all 9 `queries_ch.rs` → `queries.rs` and moved the
  internal query-result rows / resolved-params / helpers out of `dto.rs` into
  `queries.rs`. `dto.rs` now holds only wire (Serialize/ToSchema) + cursor
  types (cursors are serialized into the opaque ADR-0008 wire cursor).

`cargo check --workspace` + `cargo check -p api --tests` clean throughout.

### ⏳ Remaining — exhaustive

**Gated on two team decisions (audit-harness, backfill-bench):**

1. `crates/audit-harness/` — **decision: port to CH or delete.** No CH mirror
   exists (the `compare-with-stellar-api` skill is also PG-bound); it is
   project functionality (continuous Horizon/archive correctness audit) whose
   only PG-bound part is the `sqlx` read side. Includes crate + workspace
   member + README + `reports/` + `sql/` + `run-invariants.sh`.
2. `crates/backfill-bench/` — **decision: drop or port.** No CH bench exists;
   dev perf tool for the (gone) PG write path. Includes crate + member +
   README + `how-to-run-soroban-backfill.md` + `scripts/bench-schema-layers.sh`.
3. `crates/db-partition-mgmt/` — delete (CH partitions declaratively via
   `PARTITION BY` in `init.sql`). Coupled to backfill-bench (bench depends on
   it), so it moves with the bench decision. Crate + member.
4. `crates/indexer` — remove the `pg-persist` feature: `Cargo.toml`
   (`pg-persist = [dep:db, dep:sqlx, domain/sqlx]`) + 13 `#[cfg(feature =
"pg-persist")]` files + the whole `handler/persist/` tree
   (mod/staging/write) + cfg blocks in `process.rs` / `handler/mod.rs` +
   `tests/persist_integration.rs`. (Used by backfill-bench + backfill-runner,
   so blocked on both.)
5. `crates/backfill-runner` — **crate stays (real CH sink)**; prune the PG
   sink: `Target::Postgres` + `--target postgres` default, the `db` + `sqlx`
   deps, `src/sink.rs` PG code, PG refs across ~11 files (assets_id_backfill,
   balance_seed, bootstrap, contract_type_rebuild, error, main, nft_reclassify,
   repair_tier1, resume, upgradeable_backfill, wasm_upgrade_backfill).
6. `crates/db/` — the PG crate (pool.rs, secrets.rs, lib.rs, Cargo.toml,
   MIGRATIONS.md). Dies only after 4 + 5 + backfill-bench stop importing it.

**Cross-cutting sqlx (after 4/5/6 land):** 7. workspace `Cargo.toml:41` — drop the `sqlx` dep; members — drop db,
db-partition-mgmt, backfill-bench, audit-harness. 8. `crates/domain` — drop the `sqlx` feature (`Cargo.toml:11,21`) + the
`#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]` derives on ~7 enums
(asset_type, contract_event_type, contract_type, nft_event_type,
operation_type, token_asset_type, enums/mod).

**Independent (no decision needed):** 9. `docker-compose.yml` — remove the base `postgres` service + `pgdata` volume
(once nothing local still talks to it — i.e. after 4/5/bench). 10. **infra** — `infra/src/lib/stacks/compute-stack.ts`: the `DATABASE_URL`
placeholder + the `API_DATASOURCE_*` env block + `infra/README.md` PG refs.
⚠️ prod config — needs an explicit go. 11. **api comments** — stale PG prose (no live code): `common/pagination.rs`,
`contracts/cache.rs`, `ledgers/queries.rs`, `liquidity_pools/handlers.rs`,
`transactions/dto.rs`+`queries.rs`, network handler doc. 12. **db-clickhouse comments** — stale PG-context prose: `src/lib.rs`,
`src/persist/stage.rs`, README. 13. **libs/api-types** — the PG strings live in DTO **doc comments**
(`LedgerListItem` "sqlx::FromRow", `NetworkStats` "pg_class.reltuples",
the `pool_ids` PG↔CH caveat, the network-stats query description). Fix the
Rust rustdoc, then **regenerate** (`nx run @rumblefish/api-types:generate`)
so `openapi.json` + `generated/types.gen.ts` refresh. 14. **docs/architecture/** ** (ADR 0032) — remove PG/RDS: `infrastructure/
    infrastructure-overview.md` (RDS + decommission), `backend/backend-overview.md`
(PG database), `database-schema/**`, plus mentions in xdr-parsing / frontend
overviews.

**Final AC verification:** 15. `rg -i 'sqlx|PgPool|postgres' crates/ infra/ libs/` = 0 outside
comments/lore/docs-archive (currently NOT zero). 16. `cargo check --workspace` clean; api-types `check-generated` green.

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

## Deep-dive addendum — easy-to-miss artifacts (recon 2026-07-06, from task 0350)

A separate deep-dive (3-agent recon during task 0350) mapped the contract
events/invocations endpoints. Bucket B covers the collapse, but these specific
PG-only artifacts are **not named** in the plan and are easy to leave behind:

- **PG-only helpers in `crates/api/src/contracts/handlers.rs`** — become dead
  once the `DataSource::Pg` arm of `list_events` is removed, but they are
  functions (not just match arms), so deleting the dispatch does NOT delete
  them. Delete explicitly:
  - `expand_events()` (~lines 626–671) — the archive-XDR-overlay unfold. This
    is the "smoking gun": it exists solely to unpack folded PG appearance rows
    from S3; the CH path has zero equivalent.
  - `ParsedLedger` struct + `build_parsed_ledgers()` (~185–227)
  - `fetch_unique_ledgers()` (~147–181) — Stellar-archive fetch for the overlay
- **Cursor `Pg` variants** — `EventCursor::Pg` (`contracts/dto.rs`) and
  `TxListCursor::Pg` (`transactions/dto.rs`). After the PG path is gone the
  `Pg` variant is unreachable, but because it is a serde-`Deserialize` target
  `cargo check` will **not** flag it as dead — prune by hand. Keep the `Ch`
  variant + the `*_cursor_matches_source` guards (ADR 0008 fail-clean).
- **`soroban_events_appearances`** is PG-only for events (CH reads
  `soroban_events` directly) → fully dead. `soroban_invocations_appearances` is
  replicated to CH and read live → keep.

### Out of scope for 0244 (flag, do not silently skip)

- **CH-native tx-detail cleanup.** `fetch_event_appearances` (transactions
  `queries_ch.rs`) synthesizes a PG-shaped "appearance" via `GROUP BY
(contract, ledger)` over `soroban_events` — the one clearly non-CH-idiom left
  on the _live_ CH path. Unifying `EventItem`/`EventAppearanceItem` (and the
  invocation pair) is a **DTO/wire redesign**, not PG removal. Separate
  follow-up; do not fold into 0244.
- **AC "response shapes unchanged" caveat (line ~161).** No longer strictly
  true: task **0350** already removed the `fold_count` field from
  `EventItem`/`InvocationItem`/`EventAppearanceItem`/`InvocationAppearanceItem`
  (deliberate breaking change on the public API). 0244's own scope keeps shapes
  stable, but the `check-generated` baseline shifts once 0350 lands.

## Notes

- Mostly deletions — review focus is "no orphaned imports, no dead code, no
  stray sqlx".
- ADRs and archived lore tasks keep their PG references (historical record) —
  do NOT touch them.
- After this task the PG cleanup for the whole project is complete.
