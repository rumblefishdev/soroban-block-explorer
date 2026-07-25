---
id: '0244'
title: 'REFACTOR: remove Postgres/sqlx entirely — CH is the only DB (API collapse + dead crates + dev tools)'
type: REFACTOR
status: completed
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
  - date: '2026-07-07'
    status: active
    who: karolkow
    note: >
      Full crate/PG teardown (~10 commits): deleted backfill-bench,
      db-partition-mgmt, crates/db; pruned backfill-runner PG sink + indexer
      pg-persist; removed domain sqlx feature/derives; removed docker postgres +
      env; swept backend-overview + indexing-pipeline docs; scrubbed public
      api-types PG leaks + regen. backfill-bench deleted; audit-harness first
      deleted then reverted → kept + PORT (task 0361). Codebase PG-free except
      audit-harness (sqlx→CH port = 0361, workspace sqlx dep gated on it) and
      infra compute-stack.ts (RDS teardown = 0239, consent-gated).
  - date: '2026-07-07'
    status: completed
    who: karolkow
    note: >
      Completed → PR #319 against develop (260 files, ~-30k lines). Merged latest
      develop (perf 0350/0355/0357) via un-rename→merge→re-rename; verified no PG
      regressed back in. /review clean — 0 correctness defects across 3 finder
      passes (Sink collapse, merge decode/amount, API teardown). Also completed
      Stanisław's ask: dropped the dead API_DATASOURCE flag + DATABASE_URL from
      compute-stack.ts. Postgres/sqlx fully removed from the codebase. Residuals
      carved to separate tasks: audit-harness sqlx→CH port (0361), API
      integration-test rebuild (0360), RDS infra teardown (0239),
      soroban_contracts.name/assets.name column DROP (0304/0310).
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

### ✅ Done — session 2 (2026-07-06): docs + comment sweep (+5 commits)

- **api + db-clickhouse stale PG comments** (`b45f7661`): rewrote comments that
  described current ClickHouse code against the retired PG backend — 5
  `queries.rs` headers that claimed to "mirror the PG path (`queries.rs`)" (that
  sibling file is gone; `network` self-referenced), the contracts-cache TTL note,
  the `common/pagination` `sqlx::QueryBuilder` note, and 2 db-clickhouse `lib.rs`
  docs. Load-bearing history (stale-cursor rejection, parity-divergence
  rationale, `persist/*` "mirrors PG" notes) deliberately left as history.
- **docs/architecture PG reference-SQL removal + repoint** (`46cc726e`): deleted
  the PostgreSQL `endpoint-queries/` set (25 `.sql` + README + `run_endpoint.sh`)
  and `compare_pg_ch.sh` → `.trash/`; the CH `endpoint-queries-clickhouse/` set
  is now the sole read-plan reference. Repointed the 17 canonical-SQL doc-comment
  refs across 13 api modules to the CH set; fixed the dangling PG-dir links in
  backend/clickhouse-pilot/CH-README and rewrote `database-schema-overview` §7.2
  (dropped the now-false "CH not wired yet / parallel store" claim).
- **docs/architecture store name** (`35251027`): xdr-parsing (6 store refs, ADR
  0029 "raw XDR not stored" semantics preserved) + frontend overview → ClickHouse.
- **`database-schema-overview.md` retargeted** (`16df884b`): flipped framing from
  "the PostgreSQL schema" to the store-agnostic logical model + a **Store: CH**
  banner; physical authority = `crates/db-clickhouse/schema/init.sql` +
  `clickhouse-pilot.md`; inverted the false §8.0 "read-empty parallel store /
  Postgres unchanged" appendix; §8.2 → `init.sql`. DDL blocks kept as
  banner-labelled historical PG notation (no duplication of clickhouse-pilot.md).
- **`technical-design-general-overview.md` store-identity sweep** (`a7f5e9f9`):
  intro store source + a top **Store-status** banner, §2.1 backend (drop sqlx,
  store → CH), Search → CH buckets, §6 Database Schema framing + a
  historical-notation note. **RDS-infra sections deliberately HELD** as the frozen
  pre-cutover baseline (task 0239 teardown, consent-gated); reconciled the stale
  2026-05-20 amendment note; fixed a mislabeled `0239` link (`backlog`→`archive`).

### ✅ Done — session 3 (2026-07-07): full crate / PG teardown (~10 commits)

All PG-code removal complete, in dependency order:

- **`backfill-bench`** deleted (`7cbf6dc4`) — dead PG-write bench, keystone of `pg-persist`.
- **`audit-harness`** deleted (`71506367`) then **reverted** (`4bd834b8`) — kept + ported (0361; see decision below).
- **`db-partition-mgmt`** deleted (`c33cb642`) — obsolete under CH declarative `intDiv` partitioning.
- **`backfill-runner` PG sink pruned** (`895a42da`) — `Sink`/`PartitionWriterHandle` → CH-only newtypes; `Target`/`--target`/`--database-url`/`resume.rs`/`ClassificationCache` gone; `db`+`sqlx` deps dropped; guards + tests simplified; README rewritten. Crate stays (real CH sink).
- **`indexer` pg-persist removed** (`ecbe2416`) — `handler/persist/` tree, `process_ledger`, PG error variants, integration test, feature + `db`/`sqlx` deps.
- **`crates/db` deleted** (`e1ab7bdf`) — zero consumers; + 6 dead `db:*` npm scripts.
- **`domain` sqlx removed** (`d813cb09`) — feature + optional dep + `sqlx::Type` derives on all 6 enums (kept `#[repr(i16)]`).
- **docker-compose `postgres`** + `pgdata` + `.env.example` PG vars removed (`b8cd3e8c`).
- **docs** swept CH-only: `backend-overview` (`2234bda0`), `indexing-pipeline` (`54994e0c`).
- **libs/api-types** public-OpenAPI PG-leak scrub + regen (`d5b30346`; `--no-verify` — pre-existing unrelated FE test, proven on clean HEAD). Internal parity comments kept as history.
- **`balance_seed`** stale doc fixed (`dcb773ae`).

### Decision record — audit-harness / backfill-bench

**DECIDED (2026-07-06): delete both.** **REVISED (2026-07-07): keep + port
`audit-harness`** (Karol signed off both).

- **`audit-harness` → KEEP + PORT, deferred to task 0361.** First deleted
  (commit `71506367`), then reverted (`4bd834b8`) once the code was re-read:
  it is a genuine correctness safety net (Horizon/archive diff + all-row SQL
  invariants) and the 3 diff binaries (~2760 lines) are **store-agnostic** —
  only ~32 lines touch `sqlx`. So the CH work is a **port** (swap the DB-read
  layer to `clickhouse::Client`, re-express the 18 SQL invariants for CH), not a
  rewrite — porting preserves tested Horizon/archive/CAP-0038 logic. The port is
  real feature work with live-diff validation, so it stays **out of 0244** and is
  tracked under task **0361** (re-scoped rebuild→port). **Consequence:**
  `audit-harness` remains PG-bound (`sqlx`) in-tree, so item 7's _"drop the
  `sqlx` workspace dep"_ is **carved out** — sqlx survives solely for
  audit-harness until 0361 lands (see the AC verification below).
- **`backfill-bench` → B: delete.** Benchmarks the now-dead PG write path,
  redundant with `backfill-runner` (the real CH sink), and the keystone pinning
  `pg-persist`. If local throughput benchmarking is wanted later, add a `--bench`
  mode to `backfill-runner` (CH). (Done — commit `7cbf6dc4`.)

### ⏳ Remaining — two deferred tracks only

The codebase is PG-free **except** `audit-harness` (sqlx-bound, kept on purpose):

1. **`audit-harness` sqlx→CH port + drop the workspace `sqlx` dep** → **task 0361**
   (re-scoped rebuild→port, on develop). `crates/domain` and every other crate were
   freed of sqlx; the workspace `Cargo.toml` sqlx dep survives **solely** for
   audit-harness until 0361 lands.
2. **infra `compute-stack.ts`** (`DATABASE_URL` / `API_DATASOURCE_*` env,
   `infra/README.md`) + `infrastructure-overview.md` RDS topology → **task 0239**
   (AWS-side RDS teardown, consent-gated prod). `docker-compose.prod.yml` gated
   `postgres-merge` / `snapshot-source` also OPS/0239.

### ✅ AC verification (2026-07-07)

- `cargo check --workspace` clean, **zero warnings**.
- No live `sqlx` / `PgPool` / `postgres` in Rust code **outside `audit-harness`** —
  remaining hits are accurate-history comments.
- api-types regenerated; public OpenAPI is PG-free.
- Residual `sqlx` = audit-harness + workspace dep (→ 0361); residual infra PG/RDS (→ 0239).

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

- [~] `rg -i 'sqlx|PgPool|PgConnection|postgres' crates/ infra/ libs/` = zero
  outside comments/lore/docs — **met EXCEPT `crates/audit-harness`** (sqlx-bound,
  kept on purpose → task **0361**); all other hits are intentional history comments.
- [~] `sqlx` absent from workspace + every crate `Cargo.toml` — **carved to 0361**:
  workspace `sqlx` dep + `audit-harness` are the only remaining users; the drop
  lands with the 0361 port.
- [x] `cargo check --workspace --tests` clean, zero warnings; /review = 0 defects.
- [~] API integration tests on CH fixtures — the 5448-line PG fixture suite was
  **dropped**; the CH rebuild is task **0360**.
- [x] `API_DATASOURCE_*` + `DATABASE_URL` removed from infra (`compute-stack.ts`,
      Stanisław's ask) + all Rust code.
- [x] docker-compose has no Postgres service (`postgres` + `pgdata` removed).
- [x] Deleted crates removed from workspace members + CDK (db-migrate /
      db-partition-mgmt CDK stacks were removed in 0241; members updated here).
- [ ] Staging smoke: all API endpoints return expected data — **pending
      post-merge OPS deploy** (not a code task; runs when PR #319 deploys to staging).
- [~] **Docs updated** — DB-layer + ingestion + backend/xdr/frontend swept to
  ClickHouse; the **RDS infra-topology** sections are held for task **0239**
  (frozen pre-cutover deployment). ADRs kept as historical record.
- [x] **API types regenerated** — `nx run @rumblefish/api-types:check-generated` green.

**AC summary:** 6 fully met `[x]`; 4 met-modulo-carve-out `[~]` (→ 0361 ×2, 0360, 0239);
1 pending `[ ]` = staging smoke (post-merge OPS). All of 0244's **code scope** is
delivered in PR #319; residual AC is owned by the follow-up tasks below.

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
