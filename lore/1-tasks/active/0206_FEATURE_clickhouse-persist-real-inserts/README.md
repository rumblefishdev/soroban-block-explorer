---
id: '0206'
title: 'ClickHouse writer — populate the 0204 schema with parser data (real `persist_ledger_clickhouse`)'
type: FEATURE
status: active
related_adr: ['0044']
related_tasks: ['0204', '0205']
tags:
  [
    layer-backend,
    layer-db,
    clickhouse,
    persist,
    backfill,
    pilot,
    effort-medium,
    priority-medium,
  ]
links:
  - lore/2-adrs/0044_clickhouse-pilot-parallel-store.md
  - lore/1-tasks/archive/0204_FEATURE_clickhouse-pilot-crate-docker-schema/README.md
  - lore/1-tasks/archive/0205_FEATURE_backfill-runner-clickhouse-target-flag.md
history:
  - date: '2026-05-11'
    who: fmazur
    status: backlog
    note: >
      Spawned after task 0205 landed the stub-driven plumbing. Replaces the
      no-op `persist_ledger_clickhouse` in `crates/db-clickhouse/src/persist.rs`
      with a real writer that populates the 17 CH tables + Dictionary built
      in task 0204. Schema is fixed — this task does not modify init.sql.
      Three owner-set requirements:
      (1) zero parser-data loss relative to the PG persist coverage contract;
      (2) bulk-ingest performance via partition-aligned streaming inserts
      (target is the 11M-ledger public-archive backfill against local Docker
      ClickHouse; per-ledger INSERTs are a CH anti-pattern that causes
      MergeTree parts explosion and trips `parts_to_throw_insert`);
      (3) writer code at senior-developer quality — no unwraps on
      parser-derived input, deterministic surrogate ID derivation,
      explicit error semantics, replay-idempotent.
  - date: '2026-05-11'
    who: fmazur
    status: active
    note: 'Promoted to active — starting implementation.'
---

# ClickHouse writer — populate the 0204 schema from parser data

## Summary

Replace the stub `db_clickhouse::persist::persist_ledger_clickhouse`
(task 0205) with a real writer that populates the 17 ClickHouse tables
and the `transaction_hash_dict` Dictionary that task 0204 stood up. The
schema is fixed: this task does **not** modify
`crates/db-clickhouse/schema/init.sql`. Every architectural decision
about column shapes, engines, partition keys, codecs, and the
`soroban_events` unfold lives in ADR 0044 + 0204 and is treated as
contract.

Two owner-set requirements drive the work:

1. **Zero parser-data loss relative to PG persist coverage.** Every
   field the indexer's PG persist path reads off the parser's
   `Extracted*` types lands in the matching CH column. The PG
   write path (`indexer::handler::persist::staging` +
   `indexer::handler::persist::write`) is the **content audit
   anchor**: the writer must reproduce the same field-to-column
   mapping, with the CH unfold of `soroban_events` per ADR 0044
   §Decision §4a. Fields PG explicitly defers to S3 archive
   (`envelope_xdr`, `result_meta_xdr`, etc.) are out of scope here
   because the CH schema does not have those columns either.

2. **Bulk-ingest performance for the 11M-ledger public-archive
   backfill against local Docker ClickHouse.** Per-ledger
   `client.insert()` is a CH anti-pattern — MergeTree creates one
   "part" per INSERT, regardless of transport, and the default
   `parts_to_throw_insert = 3000` per (table, partition) trips
   after the first ~3 000 ledgers. The writer therefore uses
   **partition-aligned streaming inserts**: one long-lived
   `clickhouse::Insert<RowT>` per table per 64k-ledger backfill
   partition, streamed via chunked transfer encoding, closed at
   partition boundary. ~172 partitions × 14 tables ≈ **2 400 parts
   created over the entire 11M-ledger backfill** — well within
   CH's comfort zone for the background merger.

Lands the writer plus the `backfill-runner::Sink` partition-writer
lifecycle in one task. Schema is untouched.

## Context

- **Where we are.** Task 0204 stood up the CH pilot crate + Docker
  service + `init.sql` with 17 tables (mirroring PG with the five
  divergences from ADR 0044 §Decision §4) + the
  `transaction_hash_dict` Dictionary. Task 0205 wired `backfill-runner
--target clickhouse` end-to-end against a no-op
  `persist_ledger_clickhouse`. Both shipped; the pilot is read-empty.
- **Why this task now.** The plumbing is green; the next ADR 0044
  gate is "populate the store so we can measure". Real INSERTs
  unlock the follow-up benchmarks (parse + write timings, on-disk
  footprint per 500k-ledger partition, point-lookup latency).
- **Schema is treated as contract.** All column shapes, engine
  choices, partition keys, ORDER BY columns, `soroban_events`
  unfolding, the folded `soroban_invocations_appearances` shape,
  the Nullable / DEFAULT decisions, and the four CH-side drops
  (`_sqlx_migrations`, `nfts.metadata`, `created_at` on fact
  tables, `soroban_contracts.search_vector`) are settled by ADR
  0044 + task 0204. This task does not propose changes; if the
  writer surfaces a hard blocker against the schema, escalate via
  ADR 0044 amendment, do not patch init.sql inline.
- **PG persist is the content audit anchor.** The PG writer
  (`indexer::handler::persist::staging` + `…::write`, ~4000 LOC)
  consumes a fixed slice of fields from each `Extracted*` type and
  writes them to PG columns. The CH writer must consume the same
  parser-side fields and write them to the matching CH columns
  (with the documented divergence: `soroban_events` is unfolded
  per ADR 0044 §Decision §4a — the writer emits one row per
  `ExtractedEvent` instead of folding into appearances). Fields
  the PG path drops to S3 (`envelope_xdr` etc.) stay out of scope
  here.

## Parser-output → CH coverage doc

Before writer code lands, the implementer produces
`notes/G-coverage-mapping.md`: for every field on every `Extracted*`
type in `crates/xdr-parser/src/types.rs`, the target is one of:

- A specific CH column (table.column from the existing init.sql), or
- "Consumed by writer staging, not directly stored — feeds <other
  column>" (e.g. `ExtractedAccountState.balances` JSON →
  `account_balances_current` rows), or
- "Not stored in CH or PG — out of scope" with a one-line reason
  matching the PG behaviour (e.g. `ExtractedTransaction.envelope_xdr`
  — PG defers to S3 archive; CH schema does not carry this column
  either).

This is a **cross-check document**, not a gate that approves schema
edits. Its purpose is to catch a writer-side dropping bug ("oh, we
forgot to write `home_domain`") before it ships, and to give
reviewers a single-page mapping to validate.

Known parser↔schema reconciliation points the writer has to resolve:

| #   | Where                                                                                                                                                                               | Resolution                                                                                                                                                                                                                                                                                                                |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `ExtractedEvent.topics` / `data` are `serde_json::Value`; CH columns are `topics_xdr` / `data_xdr` `String`                                                                         | **Writer stores `serde_json::to_string(&value)` in the `*_xdr` column.** Column name is a historical artefact from ADR 0044 (parser used to expose raw XDR; current parser only emits decoded JSON). Renaming the columns is out of scope here — flag in coverage doc; ADR 0044 amendment if owner wants to rename.       |
| 2   | Surrogate `Int64`/`Int32` ID columns (`accounts.id`, `transactions.id`, `soroban_contracts.id`, `assets.id`, `nfts.id`, `operations_appearances.id`, `liquidity_pool_snapshots.id`) | CH has no sequences. **Writer derives IDs deterministically via `cityHash64(natural_key)`**, truncated to `i32` for Int32 columns. Determinism is load-bearing: ReplacingMergeTree dedupes by ORDER BY, which is the surrogate id — non-deterministic ids would defeat replay-idempotency. See §"ID derivation strategy". |
| 3   | `ExtractedNftEvent.*`, `ExtractedLpPosition.*` slices are empty until task 0202 / 0126 land                                                                                         | Writer handles empty slices as `Ok(())` — no inserts opened, no rows written. When the parser starts producing rows, no writer change is needed.                                                                                                                                                                          |
| 4   | `ExtractedOperation.details` `serde_json::Value` carries destination, amount, asset, pool                                                                                           | Writer unpacks the JSON into the typed CH columns (`destination_id`, `amount`, `asset_code`, `asset_issuer_id`, `pool_id`), same as PG staging does today.                                                                                                                                                                |
| 5   | `ExtractedEvent.source` (`TxLevel` / `PerOp` / `Diagnostic`)                                                                                                                        | Diagnostic-source events are dropped before staging (CAP-67 — they would double-count consensus events). Same rule the PG path applies.                                                                                                                                                                                   |

## Writer architecture — partition-aligned streaming inserts

```
crates/db-clickhouse/src/
├── lib.rs                       # unchanged (apply_init_sql, Config, client)
├── persist.rs                   # legacy stub entrypoint (kept; calls into
│                                # the new PartitionWriter for back-compat
│                                # with 0205's Sink::persist_ledger path
│                                # while the lifecycle refactor lands)
├── persist/                     # NEW module dir
│   ├── stage.rs                 # CH-side staging — turns Extracted* slices
│   │                            # into typed Row structs; no DB calls
│   ├── rows.rs                  # one #[derive(clickhouse::Row, Serialize)]
│   │                            # struct per CH table
│   ├── writer.rs                # PartitionWriter — owns 14 long-lived
│   │                            # Insert<RowT> handles; lifecycle:
│   │                            # open → write_ledger(N) → commit
│   └── tests.rs                 # gated integration tests
└── schema/init.sql              # unchanged (frozen at 0204)
```

### ID derivation strategy

The 0204 schema carries surrogate `Int64`/`Int32` ID columns on
`accounts`, `transactions`, `soroban_contracts`, `assets`, `nfts`,
`operations_appearances`, and `liquidity_pool_snapshots`. PG fills
these via `BIGSERIAL`/`SERIAL`; CH has no sequence generator. The
writer derives every surrogate ID deterministically from the row's
**natural key**:

| Table                      | Surrogate column | Natural-key input                                        | Hash                               |
| -------------------------- | ---------------- | -------------------------------------------------------- | ---------------------------------- |
| `accounts`                 | `id Int64`       | `account_id` (StrKey, G… / M…)                           | `cityHash64(account_id) as Int64`  |
| `transactions`             | `id Int64`       | `hash` (tx hash, 32 bytes)                               | `cityHash64(hash_bytes) as Int64`  |
| `soroban_contracts`        | `id Int64`       | `contract_id` (StrKey, C…)                               | `cityHash64(contract_id) as Int64` |
| `assets`                   | `id Int32`       | `(asset_type, asset_code, issuer_id, contract_id)` tuple | `cityHash64(tuple).cast::<i32>()`  |
| `nfts`                     | `id Int32`       | `(contract_id, token_id)` tuple                          | `cityHash64(tuple).cast::<i32>()`  |
| `operations_appearances`   | `id Int64`       | `(transaction_hash, operation_index)` tuple              | `cityHash64(tuple) as Int64`       |
| `liquidity_pool_snapshots` | `id Int64`       | `(pool_id, ledger_sequence)` tuple                       | `cityHash64(tuple) as Int64`       |

`cityHash64` is the algorithm CH itself uses internally (available as
`cityHash64()` in CH SQL and as the `cityhasher` crate in Rust);
choosing it lets a future CH-side query reproduce any of these IDs
via SQL without depending on the writer.

**Why deterministic, not a counter:** ReplacingMergeTree dedupes rows
by `ORDER BY` key, which on every state table is the surrogate id.
Replays of the same ledger (after a partition restart) must produce
the same id for the same natural key — otherwise the "replacement"
semantic doesn't kick in and the table accumulates duplicates that
the merger never folds.

**Collision posture:** for `Int32` columns (`assets.id`, `nfts.id`)
the hash space is ~4.3 billion. Stellar mainnet has under 50k unique
assets and well under 10M unique NFTs across all collections —
collision probability is negligibly small (birthday-bound ~`n²/2³³`).
The writer logs a `tracing::warn!` if it ever inserts an
`(id, natural_key)` pair where a previously-staged different natural
key in the same partition hashed to the same id. (Cross-partition
collisions surface only at read time as an extra row a query
filtering by natural key would discard — flag as a known limitation
in `docs/architecture/database-schema/clickhouse-pilot.md`.)

FK columns referencing these IDs (e.g. `transactions.source_id`,
`operations_appearances.destination_id`,
`account_balances_current.account_id`,
`assets.issuer_id`, etc.) are derived from the same natural keys via
the same hash, so cross-table joins remain consistent within a
backfill run.

### Why per-ledger inserts are wrong here

Deployment context: local Docker `clickhouse/clickhouse-server:26.3`
on loopback (compose service from task 0204). Transport-level
arguments don't apply — HTTP-on-loopback is ~50–100 µs per request.
The constraint is **server-side**: CH MergeTree creates exactly one
"part" per `INSERT` statement, where a part is a directory under
`/var/lib/clickhouse/data/<db>/<table>/` containing one `.bin` +
`.mrk` pair per column plus a `columns.txt`, `count.txt`,
`primary.idx`, etc. (~20–40 files per part on the pilot schema).

With 14 tables × 11M ledgers the naive per-ledger pattern produces
~154M parts. Two server-side limits bite, in this order:

1. **`parts_to_throw_insert = 3000`** (CH default, per (table,
   partition)). Once breached for any table, every subsequent
   `INSERT` to that table fails with `Too many parts`. With
   ~3M tx-rows per 64k-ledger partition and 1 part per ledger,
   the cap trips after ~3000 ledgers (≈ 0.03% of the 11M
   backfill).
2. **Background merger throughput**. The merger needs to fold many
   small parts into fewer larger ones. Empirically the merger
   sustains a few hundred merges/sec on modest hardware; creating
   ~10k parts/sec (the naive pattern's birth rate at parse-bound
   throughput) overwhelms it indefinitely.

The partition-aligned streaming pattern below is the textbook CH
remedy: **few long-lived inserts, many rows per insert.** The
choice of HTTP vs Native TCP is irrelevant to this design — both
transports map one `INSERT` statement to one part.

### `PartitionWriter` lifecycle

```rust
pub struct PartitionWriter {
    inserts: PerTableInserts,   // 14 open Insert<RowT> handles (lazy-init on first row)
    ledger_buffer: Vec<LedgerRow>,  // commit markers, flushed last in commit()
}

impl PartitionWriter {
    pub fn open(client: &Client) -> Result<Self, SchemaError> { … }

    /// Stream one ledger's rows into the open inserts. Cheap — rows
    /// are written into the crate's per-insert RowBinary buffer
    /// (default ~1 MB) and chunk-sent over HTTP transparently. The
    /// `ledgers` row for `staged.sequence` is *buffered*, not written,
    /// so we can apply the commit-marker pattern at partition end.
    pub async fn write_ledger(&mut self, staged: StagedLedger<'_>)
        -> Result<(), SchemaError> { … }

    /// End every non-ledger insert (sends final HTTP chunk + waits for
    /// CH ack), then write all buffered `ledgers` rows in one final
    /// insert. Order matters: a partition-mid failure leaves the
    /// `ledgers` rows un-written → resume re-does the whole partition.
    pub async fn commit(self) -> Result<(), SchemaError> { … }

    /// Abort the partition cleanly (best-effort cancel of in-flight
    /// HTTP requests). Called on any per-ledger write error so the
    /// partial state doesn't leave open connections.
    pub async fn abort(self) { … }
}
```

Memory budget: the crate buffers ~1 MB per `Insert<T>` (configurable),
streams via chunked transfer encoding when the buffer fills. 14
inserts × 1 MB = ~14 MB peak in-process buffer per partition writer,
**independent of partition row count**. A 64k-ledger partition with
~3.2M tx rows + millions of events fits this budget without spilling.

### Write order within a partition

Per-table inserts run lazily — the `Insert<T>` opens on the **first**
row written to that table within the partition (so tables with zero
rows in this partition issue zero HTTP requests). At partition
commit:

1. End every non-ledger insert in PG-FK order (accounts → wasm →
   contracts → transactions → hash_index → participants →
   pools/snapshots/positions → operations → events → invocations →
   assets → nfts/ownership → balances). The FK order is read-time
   friendly even though CH doesn't enforce it.
2. End the `ledgers` insert **last** as the commit marker.

If any `end()` between steps 1 and 2 fails, `abort()` runs; resume
finds no `ledgers` rows for this partition's range and re-does the
whole partition. ReplacingMergeTree dedupes duplicate rows from the
partial first attempt on the next background merge.

### `backfill-runner::Sink` refactor

`Sink::persist_ledger(&self, …)` (per-ledger) is the wrong granularity
for CH. The Sink interface gains a partition-writer lifecycle:

```rust
pub enum Sink {
    Postgres(PgPool),
    Clickhouse(clickhouse::Client),
}

pub enum PartitionWriterHandle<'a> {
    Postgres { pool: &'a PgPool, classification_cache: &'a ClassificationCache },
    Clickhouse(db_clickhouse::persist::PartitionWriter),
}

impl Sink {
    pub fn open_partition<'a>(&'a self, /* classification_cache: &'a ClassificationCache */)
        -> Result<PartitionWriterHandle<'a>, BackfillError> { … }
}

impl PartitionWriterHandle<'_> {
    pub async fn write_ledger(&mut self, meta: &LedgerCloseMeta)
        -> Result<LedgerTimings, BackfillError>;
    pub async fn commit(self) -> Result<(), BackfillError>;
    pub async fn abort(self);
}
```

- **Postgres variant**: `open_partition` is a no-op (returns a struct
  borrowing the pool); `write_ledger` calls today's `process_ledger`
  exactly as 0205 wires it (each ledger is its own PG tx, fast
  commits); `commit` is a no-op. PG behaviour byte-for-byte
  unchanged.
- **Clickhouse variant**: `open_partition` constructs a
  `PartitionWriter`; `write_ledger` streams the staged rows into the
  open inserts; `commit` ends the inserts + writes ledger commit
  markers.

`ingest.rs::index_partition` flips from `for ledger { sink.persist_ledger
(…) }` to `let mut pw = sink.open_partition()?; for ledger {
pw.write_ledger(…) }; pw.commit().await?;`. ~30 LOC change in
backfill-runner.

### CH server-side configuration

The CH client is constructed with bulk-ingest-friendly settings,
appended in `db_clickhouse::client(&cfg)` (one-time change):

| Setting                       | Value                 | Why                                                                                                                                                                                     |
| ----------------------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `async_insert`                | `0`                   | We batch client-side; server-side async-buffer would add latency variance without gain at this batch size                                                                               |
| `max_insert_block_size`       | `1_048_576`           | Server-side block size during INSERT processing. Default 1M is fine; explicit to lock against future CH default changes                                                                 |
| `min_insert_block_size_rows`  | `1_000_000`           | Coalesce small chunked pieces into 1M-row blocks before the part-create path                                                                                                            |
| `min_insert_block_size_bytes` | `268_435_456` (256MB) | Same coalescing knob, byte side                                                                                                                                                         |
| `insert_deduplicate`          | `0`                   | We rely on ReplacingMergeTree ORDER-BY dedup, not the per-block dedup hash (which would consume RAM for our row volumes)                                                                |
| `enable_http_compression`     | `0` (default off)     | **Loopback transport — compression on/off doesn't move the needle.** Default off (no compression CPU on client + server). Reconsider if profiling shows kernel-buffer copies dominating |

`parts_to_throw_insert` is a per-table DDL setting in init.sql which
this task does not modify. CH default (3 000 active parts per
(table, partition)) is comfortable for the partition-aligned design:
each backfill partition contributes ~1 part per (table, CH-partition),
so even running the entire 11M backfill without merger help leaves
total active parts well under the limit.

Note on block coalescing → parts: depending on CH server version,
one streaming `INSERT` may emit one part or one part per
`max_insert_block_size` rows. The acceptance criterion (`SELECT
count() FROM system.parts WHERE active …` returns single-digit per
table per partition) is the empirical gate — set the value so the
result holds, don't over-claim "exactly one part per INSERT".

These are documented in `crates/db-clickhouse/README.md`.

### Why this is fast (back-of-envelope, local Docker target)

- 11M ledgers / 64 000 per partition = ~172 partitions.
- 14 `INSERT` statements per partition (one per table, end-of-partition)
  ⇒ ~2 400 parts created over the entire backfill — comfortable for
  CH's background merger, never approaches `parts_to_throw_insert`.
- Per-partition CH-side write cost on modest hardware: 0.5–2 s for
  ~3M tx rows + ~10M event rows. Dominated by RowBinary encode CPU
  - sequential disk write of the part files; loopback transport is
    noise.
- **Bottleneck is XDR parse, not the writer**: at ~100–300 ms per
  ledger on the parse path (existing measurement from PG runs),
  parse alone is 11M × 200 ms ≈ 25 days **sequential**.
- ⇒ Parallelism is the real lever. Out of scope for this task but
  named explicitly: run K backfill-runner instances on disjoint
  ledger ranges; local CH accepts concurrent inserts to the same
  table without coordination (each instance owns its own
  PartitionWriter). K=8 → ~3 days; K=16 → ~1.5 days. Bound by
  CPU + local disk + S3 sync throughput, not CH.

### Row struct conventions (rows.rs)

```rust
#[derive(clickhouse::Row, serde::Serialize)]
pub(super) struct TransactionRow {
    pub id: i64,                          // cityHash64(hash)
    pub hash: [u8; 32],                   // FixedString(32)
    pub ledger_sequence: i64,
    pub application_order: i16,
    pub source_id: i64,                   // cityHash64(source_account StrKey)
    pub fee_charged: i64,
    pub inner_tx_hash: Option<[u8; 32]>,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub parse_error: bool,
}
```

- Column count + order matches init.sql byte-for-byte — RowBinary is
  positional, a mismatch silently corrupts every row.
- `[u8; 32]` for `FixedString(32)` columns; `String` for variable.
- `i64` surrogate IDs come from `cityHash64(natural_key)` (see ID
  derivation section).
- Borrowed fields (`&'a str`) are used where a column accepts a
  reference into the staged data — avoids per-ledger allocation
  churn. Owned fields (the example above) are used where the
  staging owns the bytes already.

## Implementation Plan

### Step 1 — Coverage mapping doc (`notes/G-coverage-mapping.md`)

Walk every field on every `Extracted*` type in
`crates/xdr-parser/src/types.rs`. For each: target CH table.column
from the existing init.sql, **or** "consumed by writer staging,
feeds <other column>", **or** "not stored — out of scope, matches
PG". Cross-check against the PG `staging.rs` + `write.rs` to
confirm every field PG reads is mirrored on the CH side (with the
documented `soroban_events` unfold exception).

The five reconciliation points from the §"Parser-output → CH
coverage doc" table above are spelled out in the doc with the
chosen resolution. The doc is not a gate on subsequent steps — it
is the artefact reviewers use to spot a missed column.

### Step 2 — Row structs in `persist/rows.rs`

One `#[derive(clickhouse::Row, serde::Serialize)]` struct per CH
table. Field types and order match init.sql byte-for-byte. Doc string
per struct cites the originating `Extracted*` type. Borrow-where-cheap
conventions.

### Step 3 — Staging in `persist/stage.rs`

Synchronous pre-write transform. Owns:

- StrKey set collection (union of every account referenced in this
  ledger — sources, destinations, participants, balance owners,
  pool participants, NFT owners, invocation callers).
- JSON-unpacking of `ExtractedOperation.details` into typed columns
  (mirrors the PG `OpRow` shape from
  `indexer::handler::persist::staging`).
- Event-source filtering — drops the whole `EventSource::Diagnostic`
  container before staging (CAP-67 byte-identical mirrors of per-op
  events would double-count otherwise).
- Tx-participant union — `source` + op `destination`s + invocation
  callers + balance owners that touched this tx.
- Asset identity resolution — folds `sac_asset` from contract
  deployments into the assets emitted for this ledger.
- Balance-row preparation — splits native vs. credit per the sentinel
  scheme (`asset_code = ''` for native).
- LP-snapshot reserves decomposition.

**Reuse of `indexer::handler::persist::staging`.** That module is
`pub(super)` today. Two options the implementer picks at Step 4:

a. **Promote `Staged` and `*Row` types to `pub` in `indexer`** —
`db-clickhouse` adds `indexer = { path = "../indexer" }` as a dep,
imports `Staged::prepare`, drops PG-id-resolution by ignoring the
`*_str_key` columns and writing them straight. Pro: zero
duplication. Con: adds the heavy `indexer` crate to
`db-clickhouse`'s dep graph (currently zero indexer deps;
`xdr-parser` is the only shared crate).

b. **Re-implement CH-shaped staging in `db-clickhouse`** — copies the
field-extraction logic but emits CH `*Row` directly. Pro: keeps
`db-clickhouse` dep-graph clean. Con: ~600 LOC of duplicated
staging logic that has to stay in lockstep with PG staging as
parser evolves.

Implementer picks (b) as the default — the duplication is bounded
(staging logic is mostly straight field copies + 4–5 small helpers),
and the parallel-store pilot's whole point is independence from the
PG path. (a) re-opens if duplication grows past ~800 LOC during
implementation.

### Step 4 — `PartitionWriter` in `persist/writer.rs`

One struct that owns 14 `clickhouse::Insert<RowT>` handles (lazy-init,
opened on first write per table). Public surface:

```rust
pub struct PartitionWriter { /* private */ }

impl PartitionWriter {
    pub fn open(client: &Client) -> Result<Self, SchemaError>;
    pub async fn write_ledger(&mut self, staged: StagedLedger<'_>)
        -> Result<(), SchemaError>;
    pub async fn commit(self) -> Result<(), SchemaError>;
    pub async fn abort(self);
}
```

Each insert handle is wrapped in `Option<Insert<RowT>>` and lazily
opened on the first row destined for that table — partitions where
no NFT rows fire issue zero requests to `nfts` / `nft_ownership`.

The `ledgers` rows are **not** streamed into an open insert during
`write_ledger`; they're held in a `Vec<LedgerRow>` and written in
`commit()` after every other table's `end()` has acknowledged. This
is the commit-marker pattern that makes resume work.

`commit()` ends inserts in PG-FK order (accounts → wasm → contracts →
transactions → hash_index → participants → pools/snapshots/positions →
operations → events → invocations → assets → nfts/ownership →
balances), then opens + writes + ends the ledgers insert as the final
step. If any `end()` fails, `commit()` returns the error without
writing the ledger commit markers.

### Step 5 — Refactor `backfill-runner::Sink`

Add the partition-writer lifecycle as described in §"`backfill-runner
::Sink` refactor" above:

- `Sink::open_partition(&self, …) -> Result<PartitionWriterHandle<'_>>`
- `PartitionWriterHandle::write_ledger(&mut self, meta) -> Result<LedgerTimings>`
- `PartitionWriterHandle::commit(self) -> Result<()>`
- `PartitionWriterHandle::abort(self)`

PG variant maps these onto the existing `process_ledger` per-ledger
path verbatim (open + commit are no-ops; write_ledger is today's
call). CH variant constructs / drives a `PartitionWriter`.

Update `ingest.rs::index_partition` to use the lifecycle: open before
the loop, write_ledger in the loop, commit after. On any error,
`abort()` the handle before returning.

The legacy `Sink::persist_ledger` method stays as a thin wrapper
calling `open_partition`/`write_ledger`/`commit` for the single-ledger
case (preserves any test that calls it directly; the production path
goes through the lifecycle).

### Step 6 — `persist_ledger_clickhouse` becomes a thin wrapper

The 0205 stub's signature stays as the public API surface for any
caller that wants per-ledger semantics. Real body wraps a one-shot
`PartitionWriter`:

```rust
pub async fn persist_ledger_clickhouse(
    client: &Client,
    /* same Extracted* slice list as the stub */
) -> Result<(), SchemaError> {
    let mut pw = PartitionWriter::open(client)?;
    pw.write_ledger(stage::prepare(ledger, transactions, /* … */)?).await?;
    pw.commit().await
}
```

This path is only exercised by direct callers / tests — production
backfill goes through `Sink::open_partition`. Documented as such in
the rustdoc.

### Step 7 — Tests

**Unit (no CH):**

- Per row struct: round-trip a single representative row through
  serde + RowBinary serialization, asserting the byte layout matches
  CH's expected RowBinary positionally. Catches a column-order
  desync between rows.rs and init.sql.

**Integration (gated on `CLICKHOUSE_URL`):**

- Single-ledger smoke: synthesize one `ExtractedLedger` plus minimal
  slices covering at least one row per table (no empty paths), call
  `persist_ledger_clickhouse` (thin wrapper around PartitionWriter),
  then query each CH table and assert the row landed with correct
  values. Cleanup via partition drop for partitioned tables, `ALTER
TABLE … DELETE` for state tables.
- **Partition-writer lifecycle**: open a PartitionWriter, write 100
  synthetic ledgers, commit. Assert row counts per table match
  parser totals and **exactly one `INSERT` statement was emitted per
  table** (verified by `SELECT count() FROM system.query_log WHERE
event_type = 'QueryStart' AND type = 'INSERT' AND tables …`).
  Parts created per table is a function of CH's block coalescing
  (see CH config note above); the assertion is on INSERT count,
  with a follow-up assert that `system.parts` is single-digit per
  table.
- Replay-idempotency: drive the same 100 ledgers through twice;
  assert post-`OPTIMIZE FINAL` row count is identical to a single
  pass (ReplacingMergeTree dedup).
- Commit-marker contract: open a writer, write 50 ledgers, then
  `abort()` without committing; assert `SELECT count() FROM ledgers
WHERE sequence IN (…)` returns 0 — resume would re-do all 50.
- Mid-partition failure: open writer, write 50 ledgers, force one
  table's `end()` to fail (mocked transport). Assert `commit()`
  returns Err and `ledgers` rows are not written.

**End-to-end (gated, manual — owner runs):**

- 1-partition run: `cargo run -p backfill-runner -- --target
clickhouse run --start <P> --end <P+63_999>` against a fresh CH.
  Capture:
  - Wall-clock for the partition
  - Per-table row counts (must match parser totals, logged per-ledger)
  - **`SELECT count() FROM system.parts WHERE active AND database = 'default'`** — expect single-digit parts per active table (one fresh insert + any background merges), proving the "no parts explosion" property
- Same-range PG regression: `cargo run -p backfill-runner -- run
--start <P> --end <P+63_999>` (default `--target postgres`) on a
  fresh PG. Per-table row counts must match the CH run where the
  schemas overlap; CH `soroban_events` row count > PG
  `soroban_events_appearances` row count is expected (ADR 0044
  §Decision §4a unfold).
- 4-partition concurrent run (sanity check before scaling to 11M):
  spawn 4 `backfill-runner` instances on 4 disjoint partition ranges,
  all writing to the same CH. Final row counts = sum of per-instance
  parser totals; no `Too many parts` errors; no manual coordination
  needed.

**Parity check helper:**

A new bin `crates/db-clickhouse/src/bin/ch-pg-rowcount-diff.rs` that
takes a ledger range, queries `SELECT count() … WHERE ledger_sequence
BETWEEN ?` on both stores per-table, and diffs. Used for the manual
end-to-end sign-off.

### Step 8 — Documentation

- `crates/db-clickhouse/README.md` — add a "Writer" section pointing
  at `PartitionWriter` + the commit-marker contract + the CH client
  config table + the deterministic ID-derivation rule. Translation
  table is unchanged (schema is unchanged).
- `crates/backfill-runner/README.md` — document the partition-writer
  lifecycle change (single-paragraph; behaviour is invisible to the
  CLI user).
- `docs/architecture/database-schema/clickhouse-pilot.md` — rewrite
  §"Writers (stubbed)" → "Writers" with: PartitionWriter flow,
  one-row-per-event `soroban_events` population per ADR 0044
  §Decision §4a, commit-marker pattern, deterministic ID-derivation
  rule (with the Int32 collision posture note), **bulk-ingest
  performance design** (why per-ledger inserts are wrong, what
  11M-ledger throughput target looks like). Schema section of the
  doc is unchanged.
- `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`
  — one paragraph: `--target clickhouse` now writes real rows via
  partition-aligned streaming.
- `docs/architecture/infrastructure/infrastructure-overview.md` —
  N/A (local-dev only).
- Per ADR 0032, every doc change carries the "updated" or `N/A —
reason` mark in the task PR checklist.

### Step 9 — ADR 0044 alignment

ADR 0044 is `proposed` and explicitly defers two things this task
touches:

- §Open Questions Q6 ("pilot success criteria") — still open;
  re-defer with a `notes/R-success-criteria-stub.md` if any
  ergonomic-vs-numeric trade-off surfaces during writer dev.
- §"Future Work" first bullet ("populating the store") — this task
  closes that bullet. Update ADR 0044's history with a one-line
  entry on landing.

## Acceptance Criteria

- [ ] **Schema untouched.** `git diff crates/db-clickhouse/schema/init.sql`
      against `master` is empty. Any blocker against the existing
      schema is escalated via ADR 0044 amendment, not patched here.
- [ ] **Coverage mapping doc landed.** `notes/G-coverage-mapping.md`
      lists every field on every `Extracted*` type with its
      CH target (table.column from init.sql) or explicit "consumed by
      staging" / "not stored — out of scope, matches PG".
- [ ] Every CH table the writer touches has a matching
      `#[derive(clickhouse::Row, serde::Serialize)]` row struct in
      `crates/db-clickhouse/src/persist/rows.rs`; column count + order
      matches init.sql byte-for-byte (verified by unit round-trip
      tests on a representative row per table).
- [ ] **Deterministic surrogate ID derivation.** Every surrogate
      `Int64`/`Int32` ID column (`accounts.id`, `transactions.id`,
      `soroban_contracts.id`, `assets.id`, `nfts.id`,
      `operations_appearances.id`, `liquidity_pool_snapshots.id`)
      is derived via `cityHash64(natural_key)`. Verified by a unit
      test that hashes the same natural key twice and asserts byte
      equality, plus a cross-table FK consistency test
      (`transactions.source_id` derived from source account StrKey
      matches `cityHash64` of the same StrKey used in `accounts.id`).
- [ ] **`soroban_events` writer emits one row per parser `ExtractedEvent`**
      (the unfold per ADR 0044 §Decision §4a). Topics + data stored as
      `serde_json::to_string(&value)` in the `topics_xdr` / `data_xdr`
      `String` columns. Diagnostic-source events are filtered out
      before staging (CAP-67 — matches PG behaviour).
- [ ] **`PartitionWriter` lifecycle in place.** One
      `clickhouse::Insert<RowT>` handle per table opens on the first
      row written within a partition and closes at `commit()`. Verified
      by `system.query_log` showing **one INSERT statement per table
      per partition** (not per-ledger) during the 64k-ledger run.
      Active `system.parts` after the run is single-digit per table
      (no parts explosion).
- [ ] **`backfill-runner::Sink` refactored** to the partition-writer
      lifecycle (`open_partition` / `write_ledger` / `commit` /
      `abort`). PG variant maps to no-op open/commit + existing
      per-ledger persist; CH variant drives `PartitionWriter`.
      `ingest.rs::index_partition` updated.
- [ ] **CH server-side settings applied** in `db_clickhouse::client(&cfg)`:
      `async_insert=0`, `max_insert_block_size=1_048_576`,
      `min_insert_block_size_rows=1_000_000`,
      `min_insert_block_size_bytes=268_435_456`,
      `insert_deduplicate=0`. `enable_http_compression` left at
      default (`0`) for loopback. Documented with rationale in
      `crates/db-clickhouse/README.md`.
- [ ] **No parts explosion under bulk ingest.** After a full
      64k-ledger partition run against fresh CH,
      `SELECT count() FROM system.parts WHERE active AND database =
'default'` returns a single-digit per-table count.
- [ ] `cargo run -p backfill-runner -- --target clickhouse run
--start <P> --end <P+63_999>` (one full partition) against a fresh
      local CH succeeds. Per-table row counts match parser totals.
- [ ] **Concurrent-partitions sanity.** Four backfill-runner instances
      on four disjoint partition ranges write to the same CH
      simultaneously without `Too many parts` errors and without
      manual coordination. Sum of per-instance parser totals matches
      final per-table row counts.
- [ ] Replay-idempotency: running the same backfill range twice
      results in identical row counts after `OPTIMIZE TABLE …
FINAL`. ReplacingMergeTree dedupes the second pass because
      `cityHash64`-derived IDs are stable.
- [ ] Commit-marker contract: a `PartitionWriter::abort()` during a
      test run leaves `ledgers` empty for that partition's range,
      and a subsequent re-run cleanly re-does it.
- [ ] PG-side regression: `cargo run -p backfill-runner -- run
--start N --end M` (default `--target postgres`) writes identical
      counts to the pre-task PG path. Verified by per-table
      `SELECT COUNT(*)` diff.
- [ ] `cargo clippy -p db-clickhouse --all-targets -- -D warnings`
      clean; `cargo clippy -p backfill-runner --all-targets -- -D
warnings` clean (`backfill-runner` is mutated for the
      lifecycle refactor).
- [ ] **Docs updated** per ADR 0032 — `clickhouse-pilot.md`,
      `db-clickhouse/README.md`, `backfill-runner/README.md`,
      `indexing-pipeline-overview.md`. `infrastructure-overview.md`
      N/A (local-dev only).
- [ ] **API types regenerated** — N/A; this task does not touch
      `crates/api/**` or `libs/api-types/**`.
- [ ] ADR 0044 history gains a one-line entry on landing noting
      "Future Work — populate the store" is closed.

## Out of Scope

- **Cross-process partition orchestration for the 11M-ledger run.**
  The writer supports concurrent partitions out of the box (CH
  accepts parallel inserts on the same table without coordination,
  ReplacingMergeTree dedupes), but spawning K runner processes on
  disjoint ranges + their dashboard aggregation is a separate
  operational concern. Document the manual recipe in
  `backfill-runner/README.md` (one block: "to parallelize, invoke
  with `--start N --end M` on K disjoint ranges; CH side requires no
  setup"). Full orchestration (k8s job, range carving, retry on
  partition failure) is a follow-up.
- **Indexer Lambda dual-write to CH.** This task is the
  `backfill-runner` writer only. Lambda dual-write needs its own
  ADR (writer topology, hot-path overhead budget, rollback) before
  a task. The Lambda hot-path is per-ledger by definition (one
  ledger per S3 event) — it cannot use the partition-writer
  pattern; the right answer there is async_insert or a Buffer engine
  table, decided in its own ADR.
- **API read-path A/B against CH.** Separate ADR + task. This task
  only fills the store.
- **CH-side `db-merge` analogue.** Backfill consolidation is a PG-only
  concept right now; CH's background merges handle the analogous job
  on its own clock. Revisit if pilot measurements show otherwise.
- **`ExtractedLedgerEntryChange.*` per-row storage.** Neither PG nor
  CH stores these as rows today; the parser feeds them into
  downstream typed extractions (`account_states`, `contract_deployments`,
  `liquidity_pools`, `assets`, `nfts`). Coverage doc lists them as
  explicit "not stored — matches PG, derivative data".
- **Any change to `crates/db-clickhouse/schema/init.sql`.** Schema is
  frozen at 0204. If the writer surfaces a hard blocker against the
  schema (e.g. a column type that physically cannot hold the parser's
  output), escalate via ADR 0044 amendment proposal — do not patch
  init.sql in this task's PR.
- **Schema migration ladder for CH.** ADR 0044 §Decision §5 keeps the
  pilot on a single idempotent `init.sql`. A numbered migration
  ladder lands when CH is no longer read-empty _outside_ of
  backfills the team is willing to re-run.
- **Performance benchmarks vs PG.** Comes after this task lands —
  ADR 0044 Q6 "pilot success criteria" follow-up. The write-side
  throughput claim ("CH write is cheaper than XDR parse") is
  asserted by the partition-aligned design here but only proved
  numerically after a real backfill run.
- **`transaction_hash_dict` Dictionary changes.** Dictionary +
  source table both unchanged.
- **`Buffer` engine table fallback.** Considered as an alternative to
  the client-side `PartitionWriter` pattern. Rejected because (a)
  Buffer tables hold rows in CH server memory, doubling RAM
  pressure during heavy backfills, (b) the pilot's measurement
  intent is to see real on-disk part shapes, which Buffer obscures,
  (c) modern CH best practice favours client-side batching for
  bulk-ingest scenarios. Documented in `clickhouse-pilot.md`.
- **`async_insert=1` server-side batching.** Considered as a simpler
  client-code alternative. Rejected for the backfill path because
  it pushes batching decisions into CH server memory + adds latency
  variance that hides parse-vs-write timing analysis. Documented in
  `clickhouse-pilot.md` as a candidate for the indexer Lambda
  hot-path (which has different constraints).

## Notes

- The PG staging logic
  (`indexer::handler::persist::staging`, ~1665 LOC) is the
  **content audit anchor**: the implementer reads it end-to-end to
  confirm every field path the parser → staging → write pipeline
  has on the PG side is reproduced (or explicitly excluded) on the
  CH side. PG staging is **not** the structural template — CH
  staging emits CH-shaped rows directly, with `cityHash64`-derived
  surrogate IDs replacing the StrKey → `BIGSERIAL` resolution that
  fills half of PG staging's lines.
- Senior-code expectations the owner has flagged:
  no `unwrap`/`expect` on parser-derived input, every reconciliation
  point (the 5-row table above) carries a comment in the writer
  citing the rule, every TODO has a follow-up task ID. The writer
  uses borrowed `&str` / `&[u8]` against staged outputs to avoid
  per-ledger allocation churn.
- Replay-determinism is the cornerstone of the "no atomicity"
  trade-off. The implementer audits the staging layer for any
  source of non-determinism (HashMap iteration order leaking into
  row order, wall-clock leaking into a column, random IDs, etc.).
  The replay-idempotency test (same ledger run twice) is the
  cheap regression catcher.
- Effort size is **medium** (`effort-medium` tag). Rough breakdown:
  coverage mapping doc + read-PG-staging end-to-end (~1 day), row
  structs + ID derivation helpers (~half day), CH-side staging
  (~1.5 days; mostly straight field copies + 5 reconciliation
  helpers), per-table writers (~half day; mechanical), `PartitionWriter`
  - `Sink` lifecycle refactor (~1.5 days; touches `Sink`,
    `ingest.rs`, resume tests), tests (~1 day; row roundtrip + CH
    integration + e2e parity), documentation + ADR alignment
    (~half day). Total ~6 days of focused work, with a buffer of
    ~30% for issues encountered (task 0205 had 4 surprise issues
    during landing — stage similar headroom).
- **Why not just use `clickhouse::insert::WithPeriod` /
  auto-flush?** The crate's auto-flush re-opens a new HTTP request
  each flush — that defeats the part-count target. Manual lifecycle
  (open once per partition, close once per partition) is the only
  way to guarantee one part per table per partition.
- **Server-side concurrency budget.** Each open `PartitionWriter`
  holds 14 long-lived inserts in flight against CH. Default
  `max_concurrent_queries = 100` is comfortable for K=4 parallel
  runners (14×4=56); bump to ~200 if scaling to K=8+. Documented in
  `clickhouse-pilot.md`. Loopback transport itself never bottlenecks.
