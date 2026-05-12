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

The CH schema mirrors Postgres's logical entity model (same tables,
same column meanings) but uses a **hybrid key design** post-empirical
measurement: surrogate `id Int64` on three high-cardinality FK hubs
(`accounts`, `soroban_contracts`, `transactions`) derived via
`cityhash64(natural_key)` for cheap integer joins; natural / composite
primary keys on the other 12 tables where StrKey-hash composites are
already cheap. PG snapshot the pilot was sized against:
[`sources/db-schema-snapshot.md`](../../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/sources/db-schema-snapshot.md).

| Postgres counterpart                                  | ClickHouse copy                       | Category                | Notes                                                                                         |
| ----------------------------------------------------- | ------------------------------------- | ----------------------- | --------------------------------------------------------------------------------------------- |
| `accounts`                                            | `accounts`                            | state                   | surrogate `id Int64`; ORDER BY `account_id`; version = `last_seen_ledger`                     |
| `assets`                                              | `assets`                              | state                   | PK = `(asset_type, asset_code, issuer_id, contract_id)` w/ Int64=0 sentinel                   |
| `account_balances_current`                            | `account_balances_current`            | state                   | PK = `(account_id, asset_type, asset_code, issuer_id)` w/ Int64=0 sentinel                    |
| `ledgers`                                             | `ledgers`                             | immutable lookup        | only CH table that retains a wall-clock column (`closed_at`)                                  |
| `liquidity_pools`                                     | `liquidity_pools`                     | state                   | PK = `pool_id`; version = `last_updated_ledger` (was immutable in pilot)                      |
| `liquidity_pool_snapshots`                            | `liquidity_pool_snapshots`            | append-only fact        | PK = `(pool_id, ledger_sequence)`; no surrogate id                                            |
| `lp_positions`                                        | `lp_positions`                        | state                   | PK = `(pool_id, account_id)`; version = `last_updated_ledger`                                 |
| `nfts`                                                | `nfts`                                | state                   | PK = `(contract_id, token_id)`; drops `metadata`                                              |
| `nft_ownership`                                       | `nft_ownership`                       | append-only fact        | PK = `(contract_id, token_id, ledger_sequence, event_order)`                                  |
| `operations_appearances`                              | `operations_appearances`              | append-only fact        | PK = `(ledger_sequence, transaction_id, application_order)`; FK Int64                         |
| `soroban_contracts`                                   | `soroban_contracts`                   | state                   | surrogate `id Int64`; ORDER BY `contract_id`; version = `wasm_uploaded_at_ledger`             |
| `soroban_events_appearances` (folded ADR 0033 design) | `soroban_events` **(NEW)**            | append-only fact        | full-content per-event row (ADR 0044 §4a unfold); `ZSTD(3)` on JSON cols                      |
| `soroban_invocations_appearances`                     | `soroban_invocations_appearances`     | append-only fact        | PK = `(contract_id, ledger_sequence, transaction_id)`                                         |
| `transactions`                                        | `transactions`                        | append-only fact        | surrogate `id Int64`; ORDER BY `(ledger_sequence, application_order)`; bloom-filter on `hash` |
| `transaction_hash_index`                              | `transaction_hash_index` + Dictionary | append-only fact + dict | RAM-bounded `complex_key_cache` for hot `hash → ledger_sequence`                              |
| `transaction_participants`                            | `transaction_participants`            | append-only fact        | PK = `(account_id, ledger_sequence, transaction_id)`; FK Int64                                |
| `wasm_interface_metadata`                             | `wasm_interface_metadata`             | immutable lookup        | `metadata` is `String CODEC(ZSTD(3))` (was JSONB)                                             |
| `_sqlx_migrations`                                    | **NOT MIRRORED**                      | —                       | replaced by idempotent `init.sql`                                                             |

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

**Codec:** `topics_xdr` and `data_xdr` use `CODEC(ZSTD(3))` (every other
`String` column in the schema stays on CH-default LZ4). The two event
columns carry ScVal-decoded JSON
(`[{"type":"sym","value":"transfer"},{"type":"address","value":"G…"},…]`).
The repeated `"type":` / `"value":` wrapper plus shared address
prefixes give a long-range dictionary pattern that LZ4's 64 KiB
sliding window cannot exploit. Measured on the first 100 mainnet
ledgers post-writer landing: `topics_xdr` LZ4 ratio was 6.29×; ZSTD(3)
reaches ~20–40× on the same shape. `data_xdr` is lighter on
redundancy (LZ4 already 11.12×) but carries the codec for symmetry —
zero downside, marginal positive gain. ZSTD(3) was picked as the
default-encode CH ships with; bump to ZSTD(9) only if measurement
warrants (one-time write CPU, identical read-path cost). Documented
in the [ADR 0044](../../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md)
history.

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

### Writers

Real writes land in task
[0206](../../../lore/1-tasks/active/0206_FEATURE_clickhouse-persist-real-inserts/README.md),
on top of the runner plumbing task
[0205](../../../lore/1-tasks/archive/0205_FEATURE_backfill-runner-clickhouse-target-flag.md)
shipped. The writer lives in `crates/db-clickhouse/src/persist/` and
consumes the same `Extracted*` slices the PG persist path does, with
three structural differences:

1. **No surrogate-ID resolution against a table.** PG resolves
   StrKey → BIGSERIAL via `accounts.id` lookup; CH derives every
   surrogate ID inline (see §"Surrogate ID derivation" below).
2. **`soroban_events` is unfolded** per ADR 0044 §Decision §4a — one
   CH row per `ExtractedEvent`, not per `(contract, tx, ledger)` trio.
3. **`Decimal128(7)` scaling happens at staging time.** PG accepts
   NUMERIC text; CH RowBinary takes the underlying `i128` scaled by
   10⁷.

The cross-reference doc at
[`notes/G-coverage-mapping.md`](../../../lore/1-tasks/active/0206_FEATURE_clickhouse-persist-real-inserts/notes/G-coverage-mapping.md)
enumerates every `Extracted*` field with its CH target column (or
"out of scope — matches PG").

#### Partition-aligned streaming inserts

`db_clickhouse::persist::writer::PartitionWriter` holds one
long-lived `clickhouse::Insert<RowT>` per table, lazy-initialised on
the first row written to that table within a backfill partition, and
ended once at `commit()`. The shape is:

```text
open() → write_ledger() × 64_000 → commit()
                                     └─ ends every non-`ledgers` insert
                                        in PG-FK order, THEN opens +
                                        writes + ends the `ledgers`
                                        insert as the commit marker.
```

##### Why per-ledger inserts are wrong here

ClickHouse `MergeTree` creates exactly one "part" per `INSERT`
statement. With 14 tables × 11 M ledgers the naive per-ledger
pattern produces ~150 M parts and trips
`parts_to_throw_insert = 3000` (per `(table, CH-partition)`) after
the first ~3 k ledgers — about 0.03 % of an 11 M backfill. The
background merger cannot fold parts faster than they're produced at
parse-bound throughput, so the ingest path stalls.

Partition-aligned streaming holds the request open across the whole
64 k-ledger backfill partition. ~172 partitions × 14 tables ≈ 2 400
`INSERT` statements over the entire 11 M-ledger backfill — well
within the merger's comfort zone.

Loopback transport is irrelevant to this design choice — server-side
part economics are the load-bearing constraint, not HTTP round-trip
count. (See the
[`Buffer` engine vs. `async_insert` rejection notes](#alternatives-considered)
below for the alternatives we ruled out.)

##### Commit-marker pattern

`ledgers` rows are buffered in RAM during `write_ledger()`. At
`commit()` every other table's `Insert::end()` is awaited first;
only after every one ack's does the `ledgers` insert open and end.

Mid-partition failure ⇒ no `ledgers` rows ⇒ `Sink::load_completed`
returns nothing for this partition's range ⇒ resume re-does the
whole partition cleanly. Orphan rows (if any) from the partial first
attempt dedupe under `ReplacingMergeTree` on the next background
merge.

##### Memory budget

Each `clickhouse::Insert<T>` buffers 256 KiB and chunk-flushes when
full. 14 inserts × 256 KiB ≈ 3.5 MiB peak per writer, independent
of partition row count. Comfortable headroom even at K=16 parallel
partition runners on a laptop.

##### Server-side bulk-ingest settings

The writer applies these CH settings on every per-table insert it
opens (see `db_clickhouse::persist::writer::apply_bulk_ingest_settings`):

| Setting                       | Value         | Why                                                                                                                                                                                                                                                                                                                                              |
| ----------------------------- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `async_insert`                | `0`           | Client-side batching. Server-side async-buffer adds latency variance without gain at our batch size.                                                                                                                                                                                                                                             |
| `max_insert_block_size`       | `1_048_576`   | Pinned against future CH default drift.                                                                                                                                                                                                                                                                                                          |
| `min_insert_block_size_rows`  | `1_000_000`   | Coalesce small chunked pieces into 1 M-row blocks before the part-create path.                                                                                                                                                                                                                                                                   |
| `min_insert_block_size_bytes` | `268_435_456` | Same coalescing knob, byte side (256 MiB).                                                                                                                                                                                                                                                                                                       |
| `insert_deduplicate`          | `0`           | We rely on `ReplacingMergeTree` ORDER-BY dedup, not per-block dedup hash.                                                                                                                                                                                                                                                                        |
| `http_receive_timeout`        | `7200` (2 h)  | CH default 30 s closes the socket between sparse chunks on tables like `nfts` / `wasm_interface_metadata` / `lp_positions` whose row rate doesn't fill the client's 256 KiB buffer fast. Without this, `Network("channel closed")` surfaces on real-mainnet partitions; 64 k partitions (~80 min wall-clock) needed the bump from 30 min to 2 h. |
| `http_send_timeout`           | `7200` (2 h)  | Same axis, response side.                                                                                                                                                                                                                                                                                                                        |

`enable_http_compression` stays at the CH default (off) — loopback
transport, compression CPU on both sides for no measurable gain.

#### Hybrid surrogate / natural keys

After empirical measurement on a 10 k-ledger smoke (full-natural-key
variant added ~500 MB on-disk + +10 ms persist / ledger), the
production schema settled on a **hybrid**: surrogate `id Int64` on
the three central FK hubs, natural / composite primary keys on the
other 12 tables.

| Table                             | ORDER BY                                                      | Surrogate `id`? |
| --------------------------------- | ------------------------------------------------------------- | --------------- |
| `accounts`                        | `account_id` (StrKey G…)                                      | **yes — Int64** |
| `soroban_contracts`               | `contract_id` (StrKey C…)                                     | **yes — Int64** |
| `transactions`                    | `(ledger_sequence, application_order)`                        | **yes — Int64** |
| `assets`                          | `(asset_type, asset_code, issuer_id, contract_id)`            | no              |
| `account_balances_current`        | `(account_id, asset_type, asset_code, issuer_id)`             | no              |
| `nfts`                            | `(contract_id, token_id)`                                     | no              |
| `liquidity_pools`                 | `pool_id` (FixedString(32) hash)                              | no              |
| `lp_positions`                    | `(pool_id, account_id)`                                       | no              |
| `transaction_hash_index`          | `hash` (FixedString(32))                                      | no              |
| `operations_appearances`          | `(ledger_sequence, transaction_id, application_order)`        | no              |
| `transaction_participants`        | `(account_id, ledger_sequence, transaction_id)`               | no              |
| `soroban_events`                  | `(contract_id, ledger_sequence, transaction_id, event_index)` | no              |
| `soroban_invocations_appearances` | `(contract_id, ledger_sequence, transaction_id)`              | no              |
| `nft_ownership`                   | `(contract_id, token_id, ledger_sequence, event_order)`       | no              |
| `liquidity_pool_snapshots`        | `(pool_id, ledger_sequence)`                                  | no              |

The three surrogate `id` values are deterministic
`cityhash64(natural_key)` (lower 64 bits of CityHash 1.0.2 128-bit).
All `_id` FK columns across the schema (`source_id`, `contract_id`,
`transaction_id`, `caller_id`, `issuer_id`, etc.) carry the same
derived Int64. Cross-table joins are cheap integer equality.

ORDER BY on the hub tables uses the natural key (`account_id`,
`contract_id`, `(ledger_sequence, application_order)`) so direct
queries like `WHERE account_id = 'GDMOSA…'` granule-prune cheaply.
The surrogate `id` is for FK joins, not granule pruning.

##### Why hybrid, not full-natural or full-surrogate

Full-natural (StrKey FK columns + `LowCardinality(String)`)
measured ~500 MB on-disk regression + ~10 ms persist / ledger
slowdown on the 10 k-ledger smoke vs. surrogate-Int64 FK baseline.
At 11 M scale that extrapolates to ~550 GB + ~30 h slowdown — too
expensive for the readability win.

Full-surrogate (all 7 tables originally proposed) brings opaque
`Int32` collision posture on `assets.id` / `nfts.id` (4.3 B hash
space, projected 10 M+ unique values long-term) and the
"our cityhash ≠ CH SQL `cityHash64()`" footgun (different algorithm
variant). The hybrid drops surrogate IDs from the tables where
natural composite keys are already cheap (`assets`,
`liquidity_pool_snapshots`, etc.) and keeps them on the three real
FK hubs.

##### Deliberate divergence from CH SQL `cityHash64()`

The writer's hash is `cityhash-rs::cityhash_102_128` lower 64 bits.
CH's built-in `cityHash64()` SQL function is the **64-bit variant**
of CityHash v1.0.2 — a different algorithm from the lower-half of
the 128-bit variant. Future CH-side `JOIN ... ON cityHash64(...) =
id` queries need a UDF wrapping the writer's helper. Documented in
ADR 0044 history.

##### Compression of repeated StrKeys + low-cardinality columns

`LowCardinality(String)` applies to columns where per-block
cardinality stays bounded:

- `soroban_events.signature` — handful of event names (transfer,
  mint, burn, fee, …)
- `assets.asset_code` / `account_balances_current.asset_code` /
  `liquidity_pools.asset_*_code` — few thousand unique codes
- `accounts.home_domain` — handful of unique SEP-1 issuer domains
  across tens of millions of accounts

State table primary keys (e.g. `accounts.account_id` with tens of
millions of unique values long-term) use plain `String` —
`LowCardinality` overhead would dominate at that cardinality scale.

##### Empty-string + Int64=0 sentinels for composite-PK "no value"

`assets` and `account_balances_current` have composite primary keys
that include optional columns. CH `ORDER BY Nullable(*)` requires
the `allow_nullable_key` setting and is meaningfully slower than
plain types. Conventions:

- `''` (empty string) for missing `asset_code`
- `0` (Int64) for missing `issuer_id` / `contract_id` (corresponds
  to `cityhash64("")`, which is never a real StrKey hash)

Native XLM asset row: `(asset_type=0, asset_code='', issuer_id=0,
contract_id=0)`. Classic credit asset: `(asset_type=1|2,
asset_code='USDC', issuer_id=cityhash64('GAB…'), contract_id=0)`.
Soroban-native: `(asset_type=3, asset_code='', issuer_id=0,
contract_id=cityhash64('CAB…'))`.

#### Trustline removal model

`account_balances_current` is `ReplacingMergeTree(last_updated_ledger)`
with no tombstone semantics. The writer translates each
`ExtractedAccountState.removed_trustlines` entry into a
`balance = 0` row at the current ledger. RMT keeps the zero-balance
row (newest version wins). Read-time convention:

```sql
SELECT * FROM account_balances_current
WHERE account_id = ? AND balance > 0
```

`OPTIMIZE TABLE account_balances_current FINAL` collapses superseded
zero-balance rows on demand; background merge handles it
asynchronously otherwise. No `CollapsingMergeTree` / engine change
needed.

#### Alternatives considered

- **Per-ledger `Buffer` engine target** — rejected. Buffer tables
  hold rows in CH server memory until flushed, doubling RAM
  pressure during heavy backfills. The pilot's measurement intent
  is to see real on-disk part shapes, which Buffer obscures.
  Modern CH best practice favours client-side batching for bulk
  ingest.
- **`async_insert = 1` server-side batching** — rejected for the
  backfill path. Pushes batching decisions into CH server memory
  and adds latency variance that hides parse-vs-write timing
  analysis. Documented as a candidate for the indexer Lambda
  hot-path (which has different per-ledger constraints).
- **`clickhouse::Inserter` with auto-flush** — rejected. The
  crate's auto-flush re-opens a new HTTP request each flush, which
  defeats the part-count target. The manual lifecycle (open once
  per partition, close once per partition) is the only way to
  guarantee one INSERT per table per partition.

#### Concurrency

Each open `PartitionWriter` holds 14 long-lived inserts in flight
against CH. Default `max_concurrent_queries = 100` is comfortable
for K=4 parallel runners (14 × 4 = 56). Scaling to K=8+ bumps the
budget; raise `max_concurrent_queries` to ~200 in
`docker-compose.yml`'s `clickhouse` service config when
testing K=8+ in one process group. Loopback transport itself never
bottlenecks.

The indexer Lambda is unchanged — no ClickHouse dual-write yet.

---

## References

- [ADR 0044](../../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md) — pilot decision and resolved open questions
- [ADR 0033](../../../lore/2-adrs/0033_soroban-events-appearances-read-time-detail.md) — folded events design that this pilot deliberately reverses on the CH side
- [ADR 0032](../../../lore/2-adrs/0032_docs-architecture-evergreen-maintenance.md) — evergreen docs maintenance policy
- [Task 0204](../../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/README.md) — implementation task
- [`crates/db-clickhouse/README.md`](../../../crates/db-clickhouse/README.md) — crate-level README with translation table and dev workflow
- [`notes/G-clickhouse-schema-er.md`](../../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/notes/G-clickhouse-schema-er.md) — full ER diagram + ENGINE/PARTITION BY/ORDER BY matrix
