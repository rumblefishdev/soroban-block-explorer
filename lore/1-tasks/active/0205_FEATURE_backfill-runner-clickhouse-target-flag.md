---
id: '0205'
title: 'backfill-runner — `--target clickhouse` flag with stub ClickHouse persist'
type: FEATURE
status: active
related_adr: ['0044']
related_tasks: ['0204']
tags:
  [
    layer-backend,
    layer-db,
    clickhouse,
    backfill,
    mock,
    effort-small,
    priority-medium,
  ]
links:
  - lore/2-adrs/0044_clickhouse-pilot-parallel-store.md
  - lore/1-tasks/archive/0204_FEATURE_clickhouse-pilot-crate-docker-schema/README.md
history:
  - date: '2026-05-10'
    status: backlog
    who: fmazur
    note: >
      Spawned manually after task 0204 (ClickHouse pilot schema) landed.
      Adds a ClickHouse sink to the existing `backfill-runner` so the
      pilot store can be exercised end-to-end. The persist path ships
      as a stub (no-op body, logs only) — actual CH write logic comes
      in a follow-up task once the stub-driven plumbing is validated.
  - date: '2026-05-10'
    status: active
    who: fmazur
    note: >
      Promoted to active. Implementation lives on
      feat/0204_clickhouse-pilot-crate-docker-schema; that branch is
      destined for merge into develop.
  - date: '2026-05-10'
    status: active
    who: fmazur
    note: >
      Scope refined after `backfill-runner` coupling audit: 1521 LOC
      total, only ~17 lines PG-specific across 6 files. Implement as a
      `--target {postgres,clickhouse}` flag on the existing runner with
      enum-dispatched `Sink` over the 5 hookpoints.
---

# backfill-runner — `--target clickhouse` flag with stub ClickHouse persist

## Summary

Extend `crates/backfill-runner` with a `--target {postgres,clickhouse}`
flag so the same orchestrator can write to either store. Adds an enum
`Sink` that wraps `sqlx::PgPool` or `clickhouse::Client`, and replaces
the ~5 existing `&PgPool` hookpoints (preflight, resume, status,
ingest) with match dispatch on `Sink`.

The CH side ships **stubbed**: a new
`db_clickhouse::persist::persist_ledger_clickhouse` function carries the
full `persist_ledger`-shaped signature but with a no-op body (logs
inputs, returns `Ok`). Real INSERTs for the 17 mirrored tables land in
a follow-up task once the stub-driven plumbing is validated
end-to-end.

## Context

- Task 0204 stood up the pilot ClickHouse store: `crates/db-clickhouse`
  with `init.sql` (17 tables + `transaction_hash_dict`), connection
  layer, and a docker-compose service. It is **read-empty** — no
  writer exists yet.
- `crates/backfill-runner` is the production Postgres backfill: pulls
  64k-ledger partitions from `aws-public-blockchain` via `aws s3 sync`,
  invokes `indexer::handler::process::process_ledger` per ledger (which
  parses + calls `indexer::handler::persist::persist_ledger`), and
  reuses `crates/db` for the PgPool.
- Coupling audit (2026-05-10): backfill-runner is **1521 LOC** but only
  **~17 lines** carry PG-specific code (mostly in `main.rs`, `run.rs`,
  `ingest.rs`, `resume.rs`, `status.rs`, `error.rs`). `dashboard.rs`,
  `partition.rs`, `sync.rs` are PG-agnostic. The narrow surface makes a
  flag-based extension a cheap, clean change.
- ADR 0044 §Decision §6 explicitly defers "populating the store" to
  follow-up ADRs/tasks. This is that task — for the backfill flavor.
- The PG persist entrypoint signature
  (`indexer::handler::persist::persist_ledger`) accepts 17+ slices of
  `Extracted*` types covering every fact and state table. The CH stub
  mirrors that signature so the wiring is in place end-to-end before
  any real INSERTs are written.

## Hookpoints in `backfill-runner` (PG → enum dispatch)

| #   | File        | Line  | What it does today                           | After this task                                                                   |
| --- | ----------- | ----- | -------------------------------------------- | --------------------------------------------------------------------------------- |
| 1   | `main.rs`   | 32–33 | `--database-url` from `DATABASE_URL`         | Adds `--target {postgres,clickhouse}` + `--clickhouse-url` (env `CLICKHOUSE_URL`) |
| 2   | `run.rs`    | 46    | `db::pool::create_pool(database_url)`        | `Sink::Postgres(...)` or `Sink::Clickhouse(db_clickhouse::client(&cfg))`          |
| 3   | `run.rs`    | 266   | `preflight_db(&pool)` runs `SELECT 1`        | `sink.preflight().await` — both targets run `SELECT 1` (identical SQL on CH)      |
| 4   | `resume.rs` | 13    | `SELECT sequence FROM ledgers WHERE …` on PG | `sink.load_completed(start, end).await` — identical SQL on CH                     |
| 5   | `status.rs` | 14    | Same query for the `status` report           | `sink.load_completed(...)`                                                        |
| 6   | `ingest.rs` | 104   | `process_ledger(meta, pool, …)`              | `sink.persist_ledger(meta, &parse_output).await`                                  |

Hookpoint 6 needs a one-time refactor: split `process_ledger` into a
pure `parse_ledger() -> ParseOutput` plus the existing PG `persist_ledger`
call. The `Sink::Postgres` variant chains both (preserving today's
behaviour exactly); `Sink::Clickhouse` calls `parse_ledger` + the new
stub. Net change to the indexer crate is a refactor, not a behavioural
change.

## Implementation Plan

### Step 1 — Extract `parse_ledger` helper in `indexer`

Inside `crates/indexer/src/handler/process.rs`, split the existing
`process_ledger` into:

1. `pub async fn parse_ledger(meta: &LedgerCloseMeta) -> ParseOutput`
   — runs Steps 0024 through Step 3 (the parse half), returns a struct
   carrying every `Extracted*` slice that `persist_ledger` currently
   consumes plus the `parse_ms` timer.
2. `pub async fn process_ledger(meta, pool, cw_client, classification_cache)`
   — unchanged public signature; now internally calls `parse_ledger()`
   then `persist::persist_ledger()`. Behavioural net-zero for existing
   PG callers.

Package the return as a `ParseOutput` struct with named fields (15+
owned vectors as a tuple would be unmaintainable).

### Step 2 — Stub `persist_ledger_clickhouse` in `crates/db-clickhouse`

New file `crates/db-clickhouse/src/persist.rs`, exposed from
`lib.rs`. Signature mirrors
`indexer::handler::persist::persist_ledger` but takes
`&clickhouse::Client` instead of `&PgPool`. **Body is a no-op:**

```rust
pub async fn persist_ledger_clickhouse(
    client: &clickhouse::Client,
    ledger: &ExtractedLedger,
    transactions: &[ExtractedTransaction],
    operations: &[(String, Vec<ExtractedOperation>)],
    events: &[(String, Vec<ExtractedEvent>)],
    invocations: &[(String, Vec<ExtractedInvocation>)],
    contract_interfaces: &[ExtractedContractInterface],
    contract_deployments: &[ExtractedContractDeployment],
    account_states: &[ExtractedAccountState],
    liquidity_pools: &[ExtractedLiquidityPool],
    pool_snapshots: &[ExtractedLiquidityPoolSnapshot],
    assets: &[ExtractedAsset],
    nfts: &[ExtractedNft],
    nft_events: &[ExtractedNftEvent],
    lp_positions: &[ExtractedLpPosition],
    contract_name_writes: &[(String, String)],
) -> Result<(), SchemaError> {
    tracing::info!(
        ledger_sequence = ledger.sequence,
        tx_count = transactions.len(),
        ops = operations.iter().map(|(_, v)| v.len()).sum::<usize>(),
        events = events.iter().map(|(_, v)| v.len()).sum::<usize>(),
        "persist_ledger_clickhouse called (stub — no writes)"
    );
    // TODO(follow-up): implement actual INSERTs into the 17 CH tables.
    let _ = client; // keep client alive for signature parity
    Ok(())
}
```

Drop `classification_cache` from this signature — it's a PG-specific
NFT classification helper (task 0118 Phase 2); the CH stub doesn't
need it. Re-introduce if/when real INSERTs need it.

### Step 3 — Add `Sink` enum to `backfill-runner`

New module `crates/backfill-runner/src/sink.rs`:

```rust
pub enum Sink {
    Postgres(sqlx::PgPool),
    Clickhouse(clickhouse::Client),
}

impl Sink {
    pub async fn preflight(&self) -> Result<(), BackfillError> { /* match … */ }

    pub async fn load_completed(
        &self,
        start: u32,
        end: u32,
    ) -> Result<HashSet<u32>, BackfillError> { /* match … */ }

    pub async fn persist_ledger(
        &self,
        meta: &LedgerCloseMeta,
        classification_cache: &ClassificationCache,
    ) -> Result<LedgerTimings, BackfillError> { /* match … */ }
}
```

- `preflight` matches on variant; both run `SELECT 1`.
- `load_completed` matches on variant; both run
  `SELECT sequence FROM ledgers WHERE sequence BETWEEN $1 AND $2`
  (SQL portable between PG and CH for this query). CH side uses the
  `clickhouse` crate's `fetch_all` with a `Row` derive on a private
  `LedgerSeqRow { sequence: i64 }` struct.
- `persist_ledger`:
  - **Postgres variant** — calls
    `process_ledger(meta, pool, None, classification_cache)` (existing
    behaviour, unchanged).
  - **Clickhouse variant** — calls `parse_ledger(meta)` (from Step 1)
    then `db_clickhouse::persist::persist_ledger_clickhouse(client, …)`.
    `classification_cache` is ignored on this path.

`BackfillError` gains a `Ch(#[from] clickhouse::error::Error)` variant
next to the existing `Db(#[from] sqlx::Error)`.

Wire `run.rs`, `status.rs`, `resume.rs`, `ingest.rs` to take `&Sink`
in place of `&PgPool`. All ~5 hookpoints become single-line `sink.<…>()`
calls.

### Step 4 — Wire CLI flag

In `main.rs`:

```rust
#[derive(Clone, ValueEnum)]
enum Target { Postgres, Clickhouse }

#[derive(Parser)]
struct Cli {
    /// Which store to write to. Defaults to `postgres` so existing
    /// invocations keep working byte-for-byte.
    #[arg(long, value_enum, default_value = "postgres")]
    target: Target,

    /// Postgres DSN (required when --target postgres).
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,

    /// ClickHouse URL (required when --target clickhouse).
    #[arg(long, env = "CLICKHOUSE_URL")]
    clickhouse_url: Option<String>,

    // … existing flags (temp-dir, verbose, subcommand) unchanged
}
```

`run.rs` and `status.rs` build the appropriate `Sink` from the chosen
target. Panic loudly at startup if the URL required by the chosen
target is missing — same posture as the existing pre-flight panics.

For the CH target, build the `Config` via
`db_clickhouse::Config::from_env()` (already picks up `CLICKHOUSE_URL`,
`CLICKHOUSE_USER`, `CLICKHOUSE_PASSWORD`, `CLICKHOUSE_DATABASE`); the
CLI `--clickhouse-url` flag overrides the env var.

### Step 5 — Tests

- Unit test for `persist_ledger_clickhouse` stub: assert it returns
  `Ok(())` on a synthetic `ExtractedLedger` and doesn't touch the
  client (lives in `crates/db-clickhouse/src/persist.rs`).
- Unit test for `Sink::Clickhouse` preflight + load_completed against
  the live local CH (gated on `CLICKHOUSE_URL` like the existing
  `db-clickhouse` smoke test).
- The existing PG resume tests in `backfill-runner/src/resume.rs` stay
  untouched — `Sink::Postgres` is behaviourally identical to today.
- Integration: `cargo run -p backfill-runner -- --target clickhouse
run --start N --end M` on a tiny range against a fresh local CH;
  verify the runner parses ledgers end-to-end and emits the stub
  `tracing::info!` per ledger (no rows written, no errors).
- Regression: `cargo run -p backfill-runner -- run --start N --end M`
  (no `--target` flag) still writes to PG exactly as before.

### Step 6 — Documentation

- `crates/backfill-runner/README.md` — add a "Targets" section
  documenting `--target {postgres,clickhouse}`, the env var pairs, and
  the CH side's stub status. Mark clearly that CH path writes nothing
  yet.
- `docs/architecture/database-schema/clickhouse-pilot.md` — add a
  "Writers (stubbed)" subsection under §8 noting `backfill-runner`
  with `--target clickhouse` exists but persist is a no-op; link to
  the follow-up task ID when real INSERTs land.
- Per ADR 0032,
  `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`
  — one paragraph if the topology description references
  `backfill-runner` (likely yes — flag mention only, no behavioural
  change to PG path).
- `docs/architecture/infrastructure/infrastructure-overview.md` — N/A
  (CH path is local-dev only, not part of AWS topology).

## Acceptance Criteria

- [ ] `backfill-runner` accepts `--target {postgres,clickhouse}` and
      `--clickhouse-url` (env `CLICKHOUSE_URL`). Default `postgres`
      keeps current behaviour byte-for-byte.
- [ ] `cargo run -p backfill-runner -- --target clickhouse run --start
N --end M` runs against a healthy local ClickHouse and parses
      ledgers end-to-end. Stub persist logs per-ledger context; zero
      rows written.
- [ ] `cargo check -p backfill-runner` + `cargo clippy -p
backfill-runner -- -D warnings` clean.
- [ ] `cargo check -p db-clickhouse` + clippy still clean after adding
      `persist::persist_ledger_clickhouse`.
- [ ] `cargo check -p indexer` + clippy still clean after `parse_ledger`
      extraction. `process_ledger` public signature unchanged; existing
      PG callers compile without edits.
- [ ] `Sink` enum + 3 dispatch methods live in
      `crates/backfill-runner/src/sink.rs`.
- [ ] `BackfillError` gains `Ch(#[from] clickhouse::error::Error)`.
- [ ] `Sink::Clickhouse` preflight + load_completed exercised by a
      unit test gated on `CLICKHOUSE_URL`.
- [ ] Stub `persist_ledger_clickhouse` unit test asserts `Ok(())` and
      no client mutation.
- [ ] `crates/backfill-runner/README.md` documents the `--target` flag + env var pairs + stub status.
- [ ] `docs/architecture/database-schema/clickhouse-pilot.md` updated
      with a "Writers (stubbed)" subsection linking to this task.
- [ ] Other architecture docs marked N/A with reason per ADR 0032.
- [ ] `Cargo.lock` regenerated and committed.
- [ ] PG-side behaviour unchanged: existing
      `cargo run -p backfill-runner -- run --start N --end M` (no
      `--target` flag) runs against PG exactly as before.

## Out of Scope

- **Real CH INSERT logic for any of the 17 tables.** Spawned as a
  follow-up task after the stub-driven plumbing lands and is validated.
- Indexer Lambda dual-write to ClickHouse — separate ADR + task; this
  task only adds a CH-flavored backfill path.
- API read-path A/B against ClickHouse — separate ADR + task.
- Performance benchmarking — comes after real writes work. The stub
  path is useful here too: parse-only timings are directly comparable
  between `--target postgres` and `--target clickhouse`, isolating
  write-side cost when real INSERTs land.
- ADR 0044 Q6 "pilot success criteria" — gated on having real CH data
  to measure against.

## Notes

- The new persist function lives in `crates/db-clickhouse`, not in
  `crates/backfill-runner`. Matches the PG split: `crates/db` exposes
  the connection layer, `crates/indexer/src/handler/persist/` owns the
  PG writer logic, runner is a thin orchestrator. CH-side analogue:
  writer in `crates/db-clickhouse`, runner stays a thin orchestrator
  (gains a flag, not a body).
- Once real INSERTs land in the follow-up, the implementation is
  effectively "for each of the 17 tables, write the typed `Row` struct
  - INSERT batch logic via `clickhouse` crate's `insert<T>` API".
    Mechanical work, gated on having a working stub-driven runner to
    iterate against.
- Stub-driven phase has its own value: parse + sync timings on
  `--target clickhouse` benchmark identically to `--target postgres`,
  letting us measure CH-side overhead is purely on the write path
  (the hypothesis ADR 0044 wants to test).
- The `--target` flag intentionally defaults to `postgres` so existing
  CI scripts, runbooks, and aws-public-blockchain workflows keep
  working without edits.
