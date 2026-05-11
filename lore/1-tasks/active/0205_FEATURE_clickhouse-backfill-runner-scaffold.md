---
id: '0205'
title: 'ClickHouse backfill runner — crate scaffold with stub persist entrypoint'
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
    scaffold,
    mock,
    effort-medium,
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
      Spawned manually after task 0204 (ClickHouse pilot schema) landed. Stand
      up a parallel `crates/clickhouse-backfill-runner` that mirrors the shape
      of `crates/backfill-runner` but targets the pilot ClickHouse instead
      of Postgres. The persist entrypoint ships as a stub (no-op body, logs
      only) — actual CH write logic comes in a follow-up task once the
      stub-driven plumbing is validated end-to-end.
  - date: '2026-05-10'
    status: active
    who: fmazur
    note: >
      Promoted to active. Implementation lives on the same branch as the
      0204 archive (feat/0204_clickhouse-pilot-crate-docker-schema) — that
      branch is destined for merge into develop, so promoting on it
      is equivalent to promoting on develop directly.
---

# ClickHouse backfill runner — crate scaffold with stub persist entrypoint

## Summary

Build `crates/clickhouse-backfill-runner` as a sibling of
`crates/backfill-runner`. Same CLI shape (`run` / `status`), same partition
prefetch + per-ledger parse pipeline, same resume semantics — but the
write target is the pilot ClickHouse store from task 0204 instead of
Postgres.

This task lands the **scaffold only**: the binary compiles, the CLI runs,
the parse path executes end-to-end, and a new `persist_ledger_clickhouse`
function is invoked per ledger with the **body left as a no-op stub** (logs
inputs and returns Ok). Real ClickHouse INSERT logic for each of the 17
mirrored tables is out of scope here and lands in a follow-up task once we
can iterate against the stub-driven runner.

## Context

- Task 0204 stood up the pilot ClickHouse store: `crates/db-clickhouse`
  with `init.sql` (17 tables + `transaction_hash_dict`), connection layer,
  and a docker-compose service. It is **read-empty** — no writer exists
  yet.
- `crates/backfill-runner` is the production Postgres backfill: pulls
  64k-ledger partitions from `aws-public-blockchain` via `aws s3 sync`,
  invokes `indexer::handler::process::process_ledger` per ledger (which
  parses + calls `indexer::handler::persist::persist_ledger`), and reuses
  `crates/db` for the PgPool.
- ADR 0044 §Decision §6 explicitly defers "populating the store" to a
  follow-up task. This is that task — for the backfill flavor.
- The PG persist entrypoint signature
  (`indexer::handler::persist::persist_ledger`) accepts 17+ slices of
  `Extracted*` types covering every fact and state table. The CH stub
  mirrors that signature so the wiring is in place end-to-end before any
  real INSERTs are written.

## Implementation Plan

### Step 1 — New crate `crates/clickhouse-backfill-runner`

- Register in workspace `Cargo.toml` members list.
- `Cargo.toml` dependencies:
  - `indexer` (path) — reuse parsing only; **do not** call
    `process_ledger`, which is bound to PG. Either factor out a
    parse-only helper (preferred) or duplicate the parse orchestration
    locally in the new runner.
  - `xdr-parser` (path) — direct parser access for the local
    orchestration.
  - `db-clickhouse` (path) — new dep, exposes the CH client + the new
    `persist_ledger_clickhouse` stub.
  - `clickhouse` (crates.io, version inherited from `db-clickhouse` or
    workspace) — replaces `sqlx`.
  - `tokio`, `tracing`, `tracing-subscriber`, `clap`, `chrono`,
    `thiserror`, `indicatif`, `tracing-indicatif` — identical to
    `backfill-runner`.
- Mirror `src/` layout from `backfill-runner`:
  - `main.rs` — clap entrypoint with `run` / `status` subcommands
  - `run.rs` — orchestrator: partition loop + N+1 prefetch
  - `ingest.rs` — per-ledger parse + invoke
    `persist_ledger_clickhouse`
  - `partition.rs`, `resume.rs`, `sync.rs`, `status.rs`,
    `dashboard.rs`, `error.rs` — adapt 1:1 from `backfill-runner`
- `README.md` — mirror the existing runner's README; flag the **stub
  persist** prominently and link to the follow-up task ID for real
  INSERTs.

### Step 2 — Stub `persist_ledger_clickhouse` in `crates/db-clickhouse`

The new persist function lives in the CH crate, not in the runner —
matches the `db` / `indexer::handler::persist` ownership split on the
PG side (writer logic owned by the DB-layer crate, runner just calls
it).

Place it at `crates/db-clickhouse/src/persist.rs`, exposed from
`lib.rs`. Signature mirrors `indexer::handler::persist::persist_ledger`
but with `clickhouse::Client` instead of `&PgPool`, and using
`indexer::xdr_parser::Extracted*` types (already re-exported by
indexer) for the slice parameters. **For this task, the body is a
no-op:**

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

Drop `classification_cache` from the signature — it's a PG-specific NFT
classification helper (task 0118 Phase 2); the CH stub doesn't need it
for now. Reintroduce when the real INSERTs are wired up if/when
classification turns out to be needed for `nft_ownership` writes.

### Step 3 — Wire the runner to the stub

In `clickhouse-backfill-runner::ingest`, replace the call to
`process_ledger` with:

1. The same parse orchestration that `process_ledger` performs (extract
   ledger, transactions, operations, events, invocations, etc.).
2. A direct call to
   `db_clickhouse::persist::persist_ledger_clickhouse` with the parsed
   slices.

Preferred shape: refactor the parse half of `process_ledger` into a
shared helper in `indexer` (e.g.
`indexer::handler::process::parse_ledger -> ParseOutput { … }`) and
call that helper from both PG and CH ingest paths. If that refactor
turns out to be more than a small move, **do not block this task** —
duplicate the parse orchestration inline in the CH runner and spawn a
de-duplication follow-up task. The point of this task is to land the
stub-driven scaffold.

### Step 4 — Resume semantics for the CH side

The PG runner uses the `ledgers` table as the sole source of truth
for resume (`HashSet<u32>` of completed sequences built at startup).
The CH runner does the same against the **CH** `ledgers` table.

Stub note: while the persist function is a no-op, the resume filter
will never skip anything (CH `ledgers` stays empty). That's fine —
the scaffold runs end-to-end on a small range and exercises the
sync + parse path even with no actual writes. Once real INSERTs land
in the follow-up, the resume filter starts pruning naturally.

### Step 5 — CLI parity with `backfill-runner`

Same subcommands and flags as `backfill-runner`:

- `clickhouse-backfill-runner run --start N --end M [--clickhouse-url …]
[--temp-dir …] [-v]`
- `clickhouse-backfill-runner status --start N --end M
[--clickhouse-url …]`

Reuse `CLICKHOUSE_URL` / `CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` /
`CLICKHOUSE_DATABASE` env vars from `db-clickhouse::Config` so the
runner picks up the same `.env` the smoke test uses.

### Step 6 — Tests

- Unit test for the stub: assert it's a no-op and the signature compiles
  with the expected `Extracted*` types.
- Integration smoke (gated on `CLICKHOUSE_URL`): run the runner against a
  tiny synthetic range (e.g. one already-parsed sample ledger file
  shipped under `tests/fixtures/` — if no such fixture exists, defer the
  integration smoke and only ship the unit test).
- Reuse `crates/db-clickhouse/tests/smoke.rs` as a reference for env
  gating and connection setup.

### Step 7 — Documentation

- `crates/clickhouse-backfill-runner/README.md` — mirror the PG runner's
  README, flag the stub status.
- Update `docs/architecture/database-schema/clickhouse-pilot.md` —
  add a new subsection "Writers (stubbed)" under §8 noting the runner
  exists but its persist is a no-op; link forward to the follow-up
  task ID when real INSERTs land.
- Per ADR 0032, update `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`
  if the new runner introduces a topology bullet (it doesn't run in
  prod; only local-dev). Likely `N/A` — note that explicitly in the
  task checklist.

## Acceptance Criteria

- [ ] `crates/clickhouse-backfill-runner/` exists in the workspace and
      builds clean (`cargo check -p clickhouse-backfill-runner`,
      `cargo clippy -p clickhouse-backfill-runner -- -D warnings`)
- [ ] CLI parity: `cargo run -p clickhouse-backfill-runner -- run --start
  N --end M` runs against a healthy local ClickHouse and parses
      ledgers end-to-end (no errors), even though no rows are written
- [ ] New `db_clickhouse::persist::persist_ledger_clickhouse` function
      exists, signature mirrors the PG `persist_ledger` modulo
      classification_cache, body is a `tracing::info!` line + `Ok(())`
- [ ] Parse orchestration is invoked per ledger (verified via tracing
      output showing tx/op/event counts)
- [ ] Resume filter queries CH `ledgers` (will return empty set while
      stub is no-op — that's expected)
- [ ] Unit test asserts the stub returns `Ok(())` on a synthetic
      `ExtractedLedger` and doesn't touch the client
- [ ] `crates/clickhouse-backfill-runner/README.md` exists; flags stub
      status prominently; documents CLI usage matching
      `crates/backfill-runner/README.md`
- [ ] Updates to `docs/architecture/database-schema/clickhouse-pilot.md`
      noting the runner stub; other docs marked N/A with reason per
      ADR 0032
- [ ] `Cargo.lock` regenerated and committed
- [ ] No production crate (`indexer`, `api`, `db`, `db-merge`,
      `db-migrate`, `db-partition-mgmt`, `xdr-parser`,
      `backfill-runner`) is modified beyond the optional `parse_ledger`
      helper extraction in Step 3. If that extraction proves bigger than
      a small move, leave it for a follow-up task and duplicate parse
      logic inline.

## Out of Scope

- **Real CH INSERT logic for any of the 17 tables.** Spawned as a
  follow-up after this scaffold lands and is validated.
- Indexer Lambda dual-write to ClickHouse — separate ADR + task; this
  task only adds a CH-flavored backfill path.
- API read-path A/B against ClickHouse — separate ADR + task.
- Performance benchmarking — comes after real writes work.
- ADR 0044 Q6 "pilot success criteria" — that ADR is gated on having
  real CH data to measure against, which depends on the follow-up task,
  not this one.

## Notes

- The new persist function lives in `crates/db-clickhouse`, not in
  `crates/clickhouse-backfill-runner`, deliberately. The PG side has the
  same split (`crates/db` exposes the connection layer,
  `crates/indexer/src/handler/persist/` owns the writer logic, and
  `crates/backfill-runner` is a thin orchestrator). For the CH side we
  collapse the writer into the DB-layer crate because the indexer crate
  doesn't yet have a CH path and we don't want to introduce one until
  the dual-write ADR exists.
- Once this task lands, the follow-up to fill in real INSERTs is
  effectively "for each of the 17 tables, write the typed Row struct +
  INSERT batch logic, exercising the `clickhouse` crate's
  `insert<T>` API". That's a big-but-mechanical task — gating it on this
  scaffold means we can iterate against a working runner.
- Keep the runner stub-driven for as long as possible. Even before real
  INSERTs, this gives us value: the sync + parse path can be benchmarked
  against the PG runner to verify CH-side overhead is purely on the
  write side (the hypothesis ADR 0044 wants to test).
