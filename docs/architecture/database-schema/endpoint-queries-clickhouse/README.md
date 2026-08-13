# ClickHouse endpoint SQL query reference set

> ⚠️ **Task 0331 (unified balances) supersedes the supply/holders/account-balance
> queries here.** `06_get_accounts_by_id`, `08_get_assets_list`, `09_get_assets_by_id`
> now read from the unified `balances` table + `balance_aggregates` MV (keyed by the
> re-added `assets.id` surrogate); `asset_aggregates`, `account_balances_current`, and
> `soroban_token_*` are retired/renamed. The authoritative queries live in
> `crates/api/src/{assets,accounts}/queries_ch.rs`. The banners in those three files
> point to them; the SQL bodies here are pre-0331 and pending a full refresh.

Hand-tuned read queries — **one script per public REST endpoint** defined in
[`backend-overview.md §6.2`](../../backend/backend-overview.md#62-endpoint-inventory).
This is the sole read-query reference set; the retired Postgres set was removed
with the PG backend (task 0244).
Schema reference: [ADR 0044 ClickHouse pilot — parallel store](../../../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md).
Driving task: [0207](../../../../lore/1-tasks/archive/0207_FEATURE_clickhouse-endpoint-queries-reference-set.md).

These files are the canonical ClickHouse-side read plan for the pilot store.
They are **reference SQL**, not migration scripts — nothing in this directory
is executed by the runtime. The canonical schema lives at
`crates/db-clickhouse/schema/init.sql`.

## Conventions

Every file must:

- Carry the header block (see [§Header](#header))
- Read against a **canonical ADR 0044 table** (`crates/db-clickhouse/schema/init.sql`); never against the local `ch-mirror` exploration container — its schema differs deliberately
- Use `FINAL` on every `ReplacingMergeTree` read (see [§FINAL discipline](#final-discipline))
- Partition-prune via `intDiv(ledger_sequence, 500000) BETWEEN ...` on the 8 partitioned tables wherever the input gives a ledger range
- Resolve `transactions.hash → ledger_sequence` via the `transaction_hash_dict` Dictionary (`dictGet`), not by scanning `transaction_hash_index` directly
- JOIN `ledgers` for `closed_at` display — per ADR 0044 §5.2 only `ledgers` retains a timestamp column; all other fact tables dropped `created_at`
- Use keyset (cursor) pagination — never `OFFSET`, never full-history `COUNT(*)`
- Declare expected indexes + Dictionaries in the header
- Compare enum columns to `Int16` literals in `WHERE` (enum decoding happens in the API layer — CH has no `*_name(smallint)` SQL helper)

## Header

```sql
-- Endpoint:     <method> <path>
-- Purpose:      <one-paragraph description, may note CH-specific divergence>
-- Source:       <backend/frontend overview cross-ref>
-- Schema:       <CH table list>; ADR 0044
-- Data sources: DB-only / DB + Archive / DB + SEP-1 ...
-- Inputs:
--   $1 :name  TYPE  description
--   $2 :name  TYPE  description (NULL on first page)
--   ...
-- Indexes:      <PK / Dictionary / bloom filter list>
-- CH Engine:    <ReplacingMergeTree(version_col) | MergeTree | Dictionary>
-- CH Pattern:   <dictGet / FINAL / intDiv prune / LEAD window / scalar subquery>
-- ADR 0044 §:   §4.N (engine), §5.N (divergence vs PG)
-- Notes:
--   • <CH-specific caveat>
--   • ...
```

## FINAL discipline

| Table                                | Engine                                         | `FINAL` required?                                                                                                                                                               |
| ------------------------------------ | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ledgers`                            | `ReplacingMergeTree` (no version)              | no — unique/immutable per `sequence`; dedup-on-merge (lore-0293, was `MergeTree`)                                                                                               |
| `liquidity_pools`                    | `ReplacingMergeTree(last_updated_ledger)`      | **yes** (doc was stale: schema is RMT since task 0208)                                                                                                                          |
| `wasm_interface_metadata`            | `ReplacingMergeTree` (no version)              | no — immutable per `wasm_hash`; dedup-on-merge (lore-0293, was `MergeTree`)                                                                                                     |
| `transaction_hash_dict` (Dictionary) | `Dictionary`                                   | no (`dictGet` returns latest by Dict lifecycle)                                                                                                                                 |
| `accounts`                           | `ReplacingMergeTree(last_seen_ledger)`         | **yes**                                                                                                                                                                         |
| `assets`                             | `ReplacingMergeTree` (no version)              | **yes** — identity only since lore-0310; supply/holders come from `balance_aggregates`, name/icon from `asset_enrichment`                                                       |
| `asset_aggregates`                   | `MergeTree` (refreshable MV from balances)     | no — pre-computed per-asset `total_supply`/`holder_count`, read via a 1:1 LEFT JOIN; `Nullable` cols (NULL on miss). Refreshed on a cadence (eventually consistent) (lore-0293) |
| `account_balances_current`           | `ReplacingMergeTree(last_updated_ledger)`      | **yes**                                                                                                                                                                         |
| `soroban_contracts`                  | `ReplacingMergeTree(wasm_uploaded_at_ledger)`  | **yes**                                                                                                                                                                         |
| `nfts`                               | `ReplacingMergeTree(current_owner_ledger)`     | **yes**                                                                                                                                                                         |
| `lp_positions`                       | `ReplacingMergeTree(last_updated_ledger)`      | **yes**                                                                                                                                                                         |
| `transactions`                       | `ReplacingMergeTree` (no version, partitioned) | **yes**                                                                                                                                                                         |
| `transaction_hash_index`             | `ReplacingMergeTree` (no version, partitioned) | **yes** — but use `dictGet` on hash lookups                                                                                                                                     |
| `operations_appearances`             | same                                           | **yes**                                                                                                                                                                         |
| `transaction_participants`           | same                                           | **yes**                                                                                                                                                                         |
| `soroban_events`                     | same                                           | **yes** (ORDER BY is unique by `(contract_id, ledger_sequence, transaction_id, event_index)`; FINAL ensures replay idempotency)                                                 |
| `soroban_invocations_appearances`    | same                                           | **yes**                                                                                                                                                                         |
| `nft_ownership`                      | same                                           | **yes**                                                                                                                                                                         |
| `liquidity_pool_snapshots`           | same                                           | **yes**                                                                                                                                                                         |

**Rationale:** `ReplacingMergeTree` deduplicates by ORDER BY key on background
merges. Between ingestion and merge, the same logical row can appear N times.
`FINAL` forces row-level deduplication at read time — correct semantics with a
read-amplification cost. The cost is bounded for state tables (small) and is
acceptable for partitioned fact tables when the WHERE clause restricts to
narrow `(ledger_sequence)` ranges. If a downstream perf task replaces `FINAL`
with `argMax`-aggregation, that's a separate change and out of scope here.

## Dictionary use (E03 hot path)

`transaction_hash_dict` is a `COMPLEX_KEY_CACHE` Dictionary over
`transaction_hash_index`, RAM-bounded (1 000 000 cells), refreshes every
5 minutes. Replaces the Postgres `transaction_hash_index` partition-PK seek
([ADR 0044 §5.5](../../../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md)).

```sql
-- Resolve hash → (ledger_sequence) without scanning any partition.
SELECT
    dictGet('transaction_hash_dict', 'ledger_sequence', toString(unhex($1))) AS ledger_sequence
FROM (SELECT 1);  -- dictGet is a function, but a scalar wrapper is the idiomatic form.
```

The dictionary attribute is declared as `String` (not `FixedString`) because
CH 26.x rejects FixedString in dictionary attribute slots; the source table
keeps `FixedString(32)` and the loader coerces transparently. Callers pass
`toString(unhex(hex_param))` so the conversion is explicit.

On Dictionary miss (cache eviction + concurrent read), the bloom filter
`idx_tx_hash_bloom` on `transactions` is the fallback — but the canonical
pattern stays `dictGet`.

## ADR 0044 §5 divergences quick-ref

| §                                          | PG                                                                                | CH                                                                                                                           |
| ------------------------------------------ | --------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| §5.1 events full-content                   | `soroban_events_appearances` (folded index, payload via Archive XDR per ADR 0033) | `soroban_events` (full per-event row, `topics_xdr` + `data_xdr` inlined) — **no Archive overlay required in CH**             |
| §5.2 `created_at` dropped except `ledgers` | every partitioned table carries `created_at` for partition prune                  | only `ledgers.closed_at` exists; partition prune via `intDiv(ledger_sequence, 500000)`; closed_at displayed via JOIN ledgers |
| §5.3 `nfts.metadata` dropped               | `nfts.metadata JSONB`                                                             | column absent — project as `NULL` or via off-chain enrichment table                                                          |
| §5.4 `_sqlx_migrations` dropped            | exists                                                                            | absent (init.sql IS the migration)                                                                                           |
| §5.5 `transaction_hash_index` → Dictionary | per-partition PK on (hash)                                                        | base table preserved; Dictionary `transaction_hash_dict` overlays for hot path                                               |

## Cursor encoding

Same convention as PG ([README §Cursor encoding](../endpoint-queries/README.md#cursor-encoding-shared-convention)):
keyset on the natural ORDER BY tuple, tuple comparison
`(a, b) < ($cursor_a, $cursor_b)`, `NULL` on first page covered by
`($cursor_a IS NULL OR (a, b) < ($cursor_a, $cursor_b))`.

CH-specific: cursor tuples drop the `created_at` term where the PG cursor
used it, because CH partitioned tables don't carry it (§5.2). Replacement:
ledger-based tuple `(ledger_sequence, application_order, id)` or similar.

## Statement separator

Same as PG: literal token `-- @@ split @@` on its own line splits multi-
statement files. Single-statement files have no separator.

## Index

| #   | File                                      | Endpoint                                  | Source                                                                  |
| --- | ----------------------------------------- | ----------------------------------------- | ----------------------------------------------------------------------- |
| 01  | `01_get_network_stats.sql`                | `GET /network/stats`                      | DB-only                                                                 |
| 02  | `02_get_transactions_list.sql`            | `GET /transactions`                       | DB-only                                                                 |
| 03  | `03_get_transactions_by_hash.sql`         | `GET /transactions/:hash`                 | DB-only **CH wins:** full event payload inline (§5.1)                   |
| 04  | `04_get_ledgers_list.sql`                 | `GET /ledgers`                            | DB-only                                                                 |
| 05  | `05_get_ledgers_by_sequence.sql`          | `GET /ledgers/:sequence`                  | DB-only                                                                 |
| 06  | `06_get_accounts_by_id.sql`               | `GET /accounts/:account_id`               | DB-only                                                                 |
| 07  | `07_get_accounts_transactions.sql`        | `GET /accounts/:account_id/transactions`  | DB-only                                                                 |
| 08  | `08_get_assets_list.sql`                  | `GET /assets`                             | DB-only                                                                 |
| 09  | `09_get_assets_by_id.sql`                 | `GET /assets/:id`                         | DB + SEP-1                                                              |
| 10  | `10_get_assets_transactions.sql`          | `GET /assets/:id/transactions`            | DB-only                                                                 |
| 11  | `11_get_contracts_by_id.sql`              | `GET /contracts/:contract_id`             | DB-only                                                                 |
| 12  | `12_get_contracts_interface.sql`          | `GET /contracts/:contract_id/interface`   | DB-only                                                                 |
| 13  | `13_get_contracts_invocations.sql`        | `GET /contracts/:contract_id/invocations` | DB-only                                                                 |
| 14  | `14_get_contracts_events.sql`             | `GET /contracts/:contract_id/events`      | DB-only **CH wins:** full payload inline (§5.1)                         |
| 15  | `15_get_nfts_list.sql`                    | `GET /nfts`                               | DB-only **(no metadata, §5.3)**                                         |
| 16  | `16_get_nfts_by_id.sql`                   | `GET /nfts/:id`                           | DB-only **(no metadata, §5.3)**                                         |
| 17  | `17_get_nfts_transfers.sql`               | `GET /nfts/:id/transfers`                 | DB-only                                                                 |
| 18  | `18_get_liquidity_pools_list.sql`         | `GET /liquidity-pools`                    | DB-only                                                                 |
| 19  | `19_get_liquidity_pools_by_id.sql`        | `GET /liquidity-pools/:id`                | DB-only                                                                 |
| 20  | `20_get_liquidity_pools_transactions.sql` | `GET /liquidity-pools/:id/transactions`   | DB-only                                                                 |
| 21  | `21_get_liquidity_pools_chart.sql`        | `GET /liquidity-pools/:id/chart`          | DB-only                                                                 |
| 22  | `22_get_search.sql`                       | `GET /search`                             | DB-only **(StrKey-prefix only; free-text deferred — no pg_trgm in CH)** |
| 23  | `23_get_liquidity_pools_participants.sql` | `GET /liquidity-pools/:id/participants`   | DB-only                                                                 |

## Running

Local Docker:

```bash
# Boot the canonical pilot CH (applies init.sql via sidecar):
docker compose up -d clickhouse db-clickhouse-init

# Verify schema:
docker compose exec clickhouse clickhouse-client \
    --user=default --password=clickhouse --query="SHOW TABLES"
# 17 tables + transaction_hash_dict expected.

# Run a single endpoint:
./run_endpoint_ch.sh 03                 # E03 tx by hash
./run_endpoint_ch.sh 04                 # E04 ledgers list
./run_endpoint_ch.sh 03 --explain       # with EXPLAIN PLAN actions=1

# Tier 1 parse-check every endpoint (CI gate):
./run_endpoint_ch.sh all --syntax-only

# Smoke-run every endpoint:
./run_endpoint_ch.sh all
```

## Validation tiers

| Tier | What                                                                                                                                 | Status as of task 0207                                                                                                                                                                                                                          |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | Schema parse — `clickhouse-client --format=Null` returns exit 0 against canonical schema                                             | **Partial.** 28 of 38 statements (23 files) parse against the canonical schema. Four endpoints fail, for three unrelated reasons — see [§Tier-1 failures](#tier-1-failures). Measured 2026-08-12 with `./run_endpoint_ch.sh all --syntax-only`. |
| 2    | Row-count equivalence — same params against PG (audit DB) and CH (mirror of same ledger range) → row counts match within tolerance   | **Deferred.** Gated on the CH writer becoming non-stub (`db_clickhouse::persist::persist_ledger_clickhouse` is a no-op per task 0205). Smoke-tested end-to-end on E01/E04/E08 with hand-inserted rows.                                          |
| 3    | Sample-row diff — 10 random keys from result set, column-by-column PG vs CH compare. Expected diffs per §5 documented in each header | **Deferred** — same gate as Tier 2.                                                                                                                                                                                                             |
| 4    | Aggregate equivalence — aggregating queries (E01 stats, E22 search) compare totals PG vs CH; tolerance per §5                        | **Deferred** — same gate as Tier 2.                                                                                                                                                                                                             |

The scaffold helper `compare_pg_ch.sh` is in place so the Tier 2-4 work
is a small follow-up once the CH writer lands — it does not require
re-deriving the per-endpoint binding logic.

### Tier-1 failures

Reproduce with `docker compose up -d clickhouse db-clickhouse-init` then
`./run_endpoint_ch.sh all --syntax-only`. Four endpoints fail, and they are
three separate problems, not one:

| Endpoint               | Error                                   | Cause                                                                                                                                |
| ---------------------- | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `01_get_network_stats` | `Syntax error at '}'` on `{head} - 200` | A Rust `format!` brace survived the copy out of `crates/api/src/network/queries.rs`. The documented SQL is not standalone.           |
| `08_get_assets_list`   | `Unknown table 'asset_aggregates'`      | The table was retired by task 0331 (unified `balances` + `balance_aggregates`). These two files are pre-0331 — see the banner above. |
| `09_get_assets_by_id`  | `Unknown table 'asset_aggregates'`      | Same.                                                                                                                                |
| `22_get_search`        | `Syntax error at ':'` on `:q_hex`       | A named placeholder; `substitute_params` only handles the positional `$N` form.                                                      |

None is caused by the endpoint's read path being wrong in the API — these are
documentation and runner gaps. Fixing 08/09 means refreshing them onto the
unified balance model; 01 means finishing the extraction; 22 means teaching the
runner named placeholders or converting the file to `$N`.

## Reviewer guide

Per-query checklist when reviewing a new or modified `.sql` file here:

1. **Header completeness.** Every line of the header convention is filled.
   `ADR 0044 §:` MUST cite every applicable rule (§4.N for engines, §5.N
   for divergences). Empty `Notes:` is OK; empty `ADR 0044 §:` is NOT.
2. **`FINAL` discipline.** Cross-check every read against the FINAL table
   above. Forgotten `FINAL` on a `ReplacingMergeTree` read returns dup
   rows pre-merge; over-applied `FINAL` on a plain `MergeTree` (`ledgers`,
   `liquidity_pools`, `wasm_interface_metadata`) is harmless but wastes a
   merge pass.
3. **§5.1 anti-pattern.** No JOIN to `soroban_events_appearances` — that
   table does not exist in CH. The full-content `soroban_events` is the
   only events table.
4. **§5.2 anti-pattern.** No `*.created_at` projection or filter on tables
   other than `ledgers`. If the query needs `closed_at`, JOIN `ledgers` on
   `ledger_sequence`.
5. **§5.3 anti-pattern.** No `nfts.metadata` projection. The column is
   absent. Metadata is fetched at the API layer via Soroban RPC
   `token_uri()` (ADR 0043).
6. **§5.5 hot path.** For hash → ledger_sequence lookups, prefer
   `dictGet('transaction_hash_dict', 'ledger_sequence', toString($1))`
   over a full scan of `transaction_hash_index`.
7. **Partition predicate.** Every read against a partitioned fact table
   that has a ledger range available should include
   `intDiv(ledger_sequence, 500000) BETWEEN intDiv($a, 500000) AND intDiv($b, 500000)`
   (or `=` for single-ledger queries). Missing the predicate forces the
   planner to scan all partitions.
8. **Cursor shape.** Keyset cursors drop the `created_at` term that
   PG-side equivalents use. CH cursors are tuples of integer columns
   (`ledger_sequence`, `application_order`, `id`, etc.).
9. **Enum decoding.** SMALLINT enum columns (`asset_type`, `event_type`,
   `contract_type`, etc.) project as raw `Int16` — no `*_name()` SQL
   helper exists in CH; decode happens in the API layer.
10. **`pg_trgm` regression awareness (§R3).** Substring search uses
    `positionCaseInsensitiveUTF8(col, $q) > 0` not `ILIKE '%q%'`. The
    cost is a linear scan after FINAL — acceptable for small tables
    (assets, NFTs) only.
11. **Tier 1 parse-check.** Reviewer runs `./run_endpoint_ch.sh <id> --syntax-only`
    against a populated local CH. Exit 0 = parses + plans cleanly.
12. **Anti-pattern grep.** `grep -nE 'NOW\(\)|encode\(|decode\(|ILIKE|::float8|::bigint|created_at|ON CONFLICT|soroban_events_appearances|n\.metadata|nfts\.metadata|LATERAL' NN_*.sql` should return no hits (PG idioms / §5 violations).

## Adding a new query

1. Pick the matching `NN_get_*.sql` filename from PG `endpoint-queries/` for naming parity.
2. Copy the header template (above) and fill every line — empty `Notes:` is OK; empty `ADR 0044 §:` is NOT.
3. Apply the PG → CH translation rules from the [§5 quick-ref](#adr-0044-5-divergences-quick-ref) above, plus the standard CH idiom swaps (`now()` for `NOW()`, `INTERVAL N UNIT` for `'N units'`, `lower(hex(b))` for `encode(b, 'hex')`, `unhex(s)` for `decode(s, 'hex')`, `toFloat64()` for `::float8`, etc.).
4. `FINAL` discipline check (see table above).
5. Partition predicate check (every `ledger_sequence` range read).
6. `./run_endpoint_ch.sh <id> --syntax-only` → exit 0 = Tier 1 OK.
7. Once CH writer is non-stub, populate a small range via `cargo run -p backfill-runner -- --target clickhouse --start S --end E` → run Tier 2-4 via `compare_pg_ch.sh <id>`.
8. Update [Index](#index) row above.
