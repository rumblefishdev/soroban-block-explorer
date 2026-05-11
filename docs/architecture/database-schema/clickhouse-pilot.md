# Stellar Block Explorer — ClickHouse Pilot

> Companion to [`database-schema-overview.md`](./database-schema-overview.md).
> The Postgres schema described there is the production source of truth; this
> document describes the **parallel ClickHouse store** that stands next to it
> for evaluation, **not** as a replacement.

> **Status:** read-empty pilot. The schema and connection layer are landed;
> indexer dual-write and API reads are deliberately deferred to follow-up
> ADRs/tasks.

---

## Table of Contents

1. [Why this exists](#1-why-this-exists)
2. [Scope and non-scope](#2-scope-and-non-scope)
3. [Schema parity with Postgres](#3-schema-parity-with-postgres)
4. [Deliberate divergences](#4-deliberate-divergences)
5. [Engine, partitioning, and ordering](#5-engine-partitioning-and-ordering)
6. [Type translation](#6-type-translation)
7. [Schema apply mechanism](#7-schema-apply-mechanism)
8. [How this fits the rest of the system](#8-how-this-fits-the-rest-of-the-system)

---

## 1. Why this exists

Two pressures push the team to evaluate a columnar OLAP store next to
Postgres ([ADR 0044](../../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md)):

- Event analytics on `/contracts/{id}/events` and any future
  "what-happened-on-chain-in-window-X" question scan a wide, append-only
  fact set with high compression potential. The folded
  `soroban_events_appearances` design ([ADR 0033](../../../lore/2-adrs/0033_soroban-events-appearances-read-time-detail.md))
  hides the heavy event payload behind an S3 round-trip. A columnar store
  could plausibly hold the full XDR cheaply enough to drop the S3 hop.
- Dashboards want to slice the same tables by different dimensions
  concurrently with the indexer write traffic. A columnar read replica
  isolates that workload.

The ClickHouse pilot lets us measure both before any reversible decision
becomes hard to reverse.

## 2. Scope and non-scope

**In scope (this pilot):**

- A `crates/db-clickhouse` crate (connection layer, schema apply CLI)
- A `clickhouse` service in `docker-compose.yml`
- A schema mirroring the Postgres `public` snapshot from 2026-05-08, with
  the five deliberate divergences in §4
- A smoke test that verifies the schema applies and reads/writes work

**Explicitly out of scope:**

- Indexer dual-write to ClickHouse — separate ADR + task
- API read-path A/B against ClickHouse — separate ADR + task
- Backfill of existing Postgres data into ClickHouse — separate task
- Any retirement of Postgres tables — deferred to the migrate-or-retire
  decision after pilot measurements
- Performance benchmarking — schema-only landing; the follow-up ADR with
  PASS/FAIL success criteria comes first

**Non-invasive contract:** no file under
`crates/{api,indexer,domain,db,db-merge,db-migrate,db-partition-mgmt,xdr-parser,backfill-runner,audit-harness,backfill-bench}`
is modified by the pilot landing PR. Allowed changes outside
`crates/db-clickhouse/`: workspace `Cargo.toml`, `docker-compose.yml`, the
docs/architecture files updated for ADR 0032 traceability, and lore.

## 3. Schema parity with Postgres

The pilot copies the Postgres logical shape table-for-table, sharing
column names and surrogate-key conventions so the two stores are mentally
1:1 wherever possible. The PG snapshot the pilot mirrors lives in the
task directory:
[`sources/db-schema-snapshot.md`](../../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/sources/db-schema-snapshot.md).

Every PG table from the snapshot that survived the divergences in §4
appears in the CH schema with the same name, the same column set (modulo
type translation in §6), and the same composite identity columns
(`ORDER BY` substitutes for `PRIMARY KEY`).

The full ER diagram and table-by-table ENGINE / PARTITION BY / ORDER BY
matrix lives in the task notes:
[`notes/G-clickhouse-schema-er.md`](../../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/notes/G-clickhouse-schema-er.md).

| Postgres counterpart                                  | ClickHouse copy                       | Category                | Notes                                                                      |
| ----------------------------------------------------- | ------------------------------------- | ----------------------- | -------------------------------------------------------------------------- |
| `accounts`                                            | `accounts`                            | state                   | natural version: `last_seen_ledger`                                        |
| `assets`                                              | `assets`                              | state                   | no natural version                                                         |
| `account_balances_current`                            | `account_balances_current`            | state                   | `allow_nullable_key=1` for nullable issuer/code in ORDER BY                |
| `ledgers`                                             | `ledgers`                             | immutable lookup        | only CH table that retains a wall-clock column (`closed_at`)               |
| `liquidity_pools`                                     | `liquidity_pools`                     | immutable post-create   | unpartitioned                                                              |
| `liquidity_pool_snapshots`                            | `liquidity_pool_snapshots`            | append-only fact        | partitioned                                                                |
| `lp_positions`                                        | `lp_positions`                        | state                   | natural version: `last_updated_ledger`                                     |
| `nfts`                                                | `nfts`                                | state                   | drops `metadata`; coerces `current_owner_ledger` to `Int64 DFLT 0`         |
| `nft_ownership`                                       | `nft_ownership`                       | append-only fact        | partitioned                                                                |
| `operations_appearances`                              | `operations_appearances`              | append-only fact        | partitioned                                                                |
| `soroban_contracts`                                   | `soroban_contracts`                   | state                   | drops `search_vector`; coerces `wasm_uploaded_at_ledger` to `Int64 DFLT 0` |
| `soroban_events_appearances` (folded ADR 0033 design) | `soroban_events` **(NEW)**            | append-only fact        | full-content per-event row                                                 |
| `soroban_invocations_appearances`                     | `soroban_invocations_appearances`     | append-only fact        | partitioned                                                                |
| `transactions`                                        | `transactions`                        | append-only fact        | partitioned; bloom-filter skip index on `hash`                             |
| `transaction_hash_index`                              | `transaction_hash_index` + Dictionary | append-only fact + dict | RAM-bounded `complex_key_cache` for hot `hash → ledger_sequence`           |
| `transaction_participants`                            | `transaction_participants`            | append-only fact        | partitioned                                                                |
| `wasm_interface_metadata`                             | `wasm_interface_metadata`             | immutable lookup        | `metadata` is `String` (was JSONB)                                         |
| `_sqlx_migrations`                                    | **NOT MIRRORED**                      | —                       | replaced by idempotent `init.sql`                                          |

CH net schema: **17 tables + 1 `Dictionary`** (PG had 18; `_sqlx_migrations` dropped).

## 4. Deliberate divergences

All five are CH-side schema choices. **Postgres is unchanged by every one.**

### 4a. `soroban_events_appearances` → full-content `soroban_events`

ADR 0033 deliberately folded events behind an S3 round-trip in PG because
storing per-event `topics_xdr` + `data_xdr` blew up the heap. In a columnar
store, the row-width penalty disappears — the pilot tests whether full
event content per row is competitive on storage and query latency.

The CH `soroban_events` table holds: `contract_id`, `transaction_id`,
`ledger_sequence`, `event_index`, `event_type`, `signature`, `topics_xdr`,
`data_xdr`. ORDER BY `(contract_id, ledger_sequence, transaction_id,
event_index)` matches the PG primary key with `created_at` substituted by
`ledger_sequence`. PG keeps `soroban_events_appearances` exactly as today.

### 4b. `created_at` dropped from every CH table except `ledgers`

CH partitions by `ledger_sequence`, not by wall-clock time. The
denormalized `created_at` column on `transactions`,
`operations_appearances`, `transaction_participants`, `nft_ownership`,
`liquidity_pool_snapshots`, `soroban_events`,
`soroban_invocations_appearances`, and `transaction_hash_index` is omitted
on the CH side. Wall-clock time is recovered via JOIN to
`ledgers.closed_at`. This eliminates ~50–100 GB of redundant
denormalization at full Stellar scale.

### 4c. `nfts.metadata` dropped (CH only)

The JSONB metadata blob is not carried in the CH copy of `nfts`. PG keeps
it unchanged.

### 4d. `_sqlx_migrations` dropped

The pilot uses an idempotent `init.sql` (every statement is
`CREATE … IF NOT EXISTS`), not a numbered migration ladder. PG continues
to use `sqlx` migrations as today.

### 4e. `transaction_hash_index` exposed as a Dictionary

The PG `transaction_hash_index` table exists in CH 1:1 (minus
`created_at`), and a `transaction_hash_dict` `DICTIONARY` is layered on
top with `complex_key_cache` layout, RAM-bounded
(`SIZE_IN_CELLS 1000000`), refreshing every 5 minutes. API reads do
`dictGet('transaction_hash_dict', 'ledger_sequence', tuple(toString(hash)))`
for microsecond-class point lookups; misses fall through to scanning the
source table.

The dictionary's source clause carries inline `USER`/`PASSWORD` literals
matching the docker-compose default (`default` / `clickhouse`) — for the
pilot only. Production deployment would replace this with a named
collection so the credential is not version-controlled.

### Cosmetic non-translatable PG features (CH-side OMIT)

- `soroban_contracts.search_vector` (tsvector) — no CH equivalent
- GIN / `pg_trgm` indexes (`assets.asset_code`, `nfts.collection_name`,
  `nfts.name`) — no analogue
- Partial unique indexes (`uidx_abc_credit`, `uidx_abc_native`,
  `uidx_assets_classic_asset`, `uidx_assets_native`,
  `uidx_assets_soroban`) — no enforcement
- FK constraints — not enforceable in CH
- CHECK constraints — omitted for the pilot

PG keeps all of these.

## 5. Engine, partitioning, and ordering

Resolved in
[ADR 0044 §Decision §5](../../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md):

- **Append-only fact tables** → `ReplacingMergeTree` (dedup by ORDER BY
  key on background merge)
- **State tables** → `ReplacingMergeTree(version_column)` where a natural
  NOT NULL ledger column exists (`last_seen_ledger`,
  `last_updated_ledger`, `current_owner_ledger`,
  `wasm_uploaded_at_ledger`); plain `ReplacingMergeTree` otherwise
- **Immutable lookup tables** → plain `MergeTree`

Every fact table uses `PARTITION BY intDiv(ledger_sequence, 500000)`.
500 000 ledgers is ≈ 29 days at Stellar's 5 s ledger time — mirrors the
PG monthly partition mental model. State and immutable tables small enough
to skip partitioning are not partitioned (default below ~10M projected
rows). The bucket size is locked at 500 000 for the pilot; revisit only
if measurements reveal merge backlog.

`ORDER BY` of each fact table substitutes `ledger_sequence` for the
dropped `created_at`. State tables use the same ORDER BY as the PG
PRIMARY KEY.

## 6. Type translation

See the canonical table in
[`crates/db-clickhouse/README.md`](../../../crates/db-clickhouse/README.md#type-translation-table)
or in
[ADR 0044 §Decision §5](../../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md).
Highlights:

- 32-byte `BYTEA` columns (hashes, `pool_id`, `wasm_hash`) become
  `FixedString(32)` in CH
- `NUMERIC(28,7)` → `Decimal128(7)`
- The only `TIMESTAMPTZ` column that survives is
  `ledgers.closed_at`, which becomes `DateTime64(3, 'UTC')`

## 7. Schema apply mechanism

`crates/db-clickhouse/schema/init.sql` holds the entire schema in one
idempotent file. Two paths apply it; both share the file via
`include_str!`:

1. **Local dev:** `cargo run -p db-clickhouse --bin db-clickhouse-init`.
   The Rust CLI reads `CLICKHOUSE_URL`, `CLICKHOUSE_USER`,
   `CLICKHOUSE_PASSWORD`, `CLICKHOUSE_DATABASE` and applies the file
   over the HTTP interface using the official `clickhouse` crate.
2. **Compose boot:** the `db-clickhouse-init` sidecar runs
   `clickhouse-client --queries-file /init.sql` against the `clickhouse`
   service after it reports healthy. Same SQL as the Rust CLI; uses
   `clickhouse-client` only to avoid a workspace compile during boot.

`docker compose down -v` is safe: the volume is rebuilt cleanly and the
sidecar re-applies the schema.

Numbered migrations (the `crates/db-migrate` analogue) are deferred until
the dual-write follow-up task lands. For now the pilot is read-empty and
schema iteration is "edit `init.sql`, nuke the volume, restart compose."

## 8. How this fits the rest of the system

The pilot is read-empty by design. Today:

- Indexer writes only to Postgres (unchanged).
- API reads only from Postgres (unchanged).
- Public archive S3 reads are unchanged (the heavy-field endpoints E3, E14
  still go through `crates/xdr-parser` per ADR 0029).

Once a follow-up ADR turns on dual-write, the indexer will gain a
ClickHouse path next to the existing PG write. Once a further follow-up
ADR turns on read A/B, the API will gain an opt-in CH read path next to
the existing PG read. Both are deliberately gated on first measurements
from the pilot.

If the pilot fails to outperform Postgres on storage and query latency
once measurements exist, the whole crate plus the compose service
deletes in one PR; nothing else has changed.

### Writers (stubbed)

[`crates/backfill-runner`](../../../crates/backfill-runner/README.md)
accepts `--target {postgres,clickhouse}` (task
[0205](../../../lore/1-tasks/archive/0205_FEATURE_backfill-runner-clickhouse-target-flag.md)).
The CH path runs the full parse pipeline against `aws s3 sync`'d
ledgers but **writes nothing**: `db_clickhouse::persist::persist_ledger_clickhouse`
is a no-op stub that logs per-ledger context and returns `Ok`.

The stub-driven phase is intentional. It validates the flag-based
plumbing — `Sink` enum, dispatch across preflight / load_completed /
persist — and lets us compare parse-side timings between the two
targets without committing to a write-shape that still has open design
questions. Real INSERTs for the 17 mirrored tables land in a follow-up
task gated on this path being green end-to-end.

The indexer Lambda is unchanged — no ClickHouse dual-write yet.

---

## References

- [ADR 0044](../../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md) — pilot decision and resolved open questions
- [ADR 0033](../../../lore/2-adrs/0033_soroban-events-appearances-read-time-detail.md) — folded events design that this pilot deliberately reverses on the CH side
- [ADR 0032](../../../lore/2-adrs/0032_docs-architecture-evergreen-maintenance.md) — evergreen docs maintenance policy
- [Task 0204](../../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/README.md) — implementation task
- [`crates/db-clickhouse/README.md`](../../../crates/db-clickhouse/README.md) — crate-level README with translation table and dev workflow
- [`notes/G-clickhouse-schema-er.md`](../../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/notes/G-clickhouse-schema-er.md) — full ER diagram + ENGINE/PARTITION BY/ORDER BY matrix
