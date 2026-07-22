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

| Postgres counterpart                                  | ClickHouse copy                       | Category                | Notes                                                                                               |
| ----------------------------------------------------- | ------------------------------------- | ----------------------- | --------------------------------------------------------------------------------------------------- |
| `accounts`                                            | `accounts`                            | state                   | surrogate `id Int64`; ORDER BY `account_id`; version = `last_seen_ledger`                           |
| `assets`                                              | `assets`                              | state                   | PK = `(asset_type, asset_code, issuer_id, contract_id)` w/ Int64=0 sentinel                         |
| `account_balances_current`                            | `account_balances_current`            | state                   | PK = `(account_id, asset_type, asset_code, issuer_id)` w/ Int64=0 sentinel                          |
| `ledgers`                                             | `ledgers`                             | immutable lookup        | only CH table that retains a wall-clock column (`closed_at`)                                        |
| `liquidity_pools`                                     | `liquidity_pools`                     | state                   | PK = `pool_id`; version = `last_updated_ledger` (was immutable in pilot)                            |
| `liquidity_pool_snapshots`                            | `liquidity_pool_snapshots`            | append-only fact        | PK = `(pool_id, ledger_sequence)`; no surrogate id                                                  |
| `lp_positions`                                        | `lp_positions`                        | state                   | PK = `(pool_id, account_id)`; version = `last_updated_ledger`                                       |
| `nfts`                                                | `nfts`                                | state                   | PK = `(contract_id, token_id)`; drops `metadata`                                                    |
| `nft_ownership`                                       | `nft_ownership`                       | append-only fact        | PK = `(contract_id, token_id, ledger_sequence, event_order)`                                        |
| `operations_appearances`                              | `operations_appearances`              | append-only fact        | PK = `(ledger_sequence, transaction_id, application_order)`; FK Int64; `pool_ids` Array (0261/0268) |
| `soroban_contracts`                                   | `soroban_contracts`                   | state                   | surrogate `id Int64`; ORDER BY `contract_id`; version = `wasm_uploaded_at_ledger`                   |
| `soroban_events_appearances` (folded ADR 0033 design) | `soroban_events` **(NEW)**            | append-only fact        | full-content per-event row (ADR 0044 §4a unfold); `ZSTD(3)` on JSON cols                            |
| `soroban_invocations_appearances`                     | `soroban_invocations_appearances`     | append-only fact        | PK = `(contract_id, ledger_sequence, transaction_id)`                                               |
| `transactions`                                        | `transactions`                        | append-only fact        | surrogate `id Int64`; ORDER BY `(ledger_sequence, application_order)`; bloom-filter on `hash`       |
| `transaction_hash_index`                              | `transaction_hash_index` + Dictionary | append-only fact + dict | RAM-bounded `complex_key_cache` for hot `hash → ledger_sequence`                                    |
| `transaction_participants`                            | `transaction_participants`            | append-only fact        | PK = `(account_id, ledger_sequence, transaction_id)`; FK Int64                                      |
| `wasm_interface_metadata`                             | `wasm_interface_metadata`             | immutable lookup        | `metadata` is `String CODEC(ZSTD(3))` (was JSONB)                                                   |
| `_sqlx_migrations`                                    | **NOT MIRRORED**                      | —                       | replaced by idempotent `init.sql`                                                                   |

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

### 4c-bis. `nfts_pending` + `nft_ownership_pending` quarantine (task 0217 + 0220)

CH carries the same `_pending` quarantine pair as PG so the routing
semantics defined in
[`crates/indexer/src/handler/persist/write.rs`](../../../crates/indexer/src/handler/persist/write.rs)
land symmetrically on both writers.

> **Writer parity status:** Task 0217 (PR #180) shipped the CH schema
>
> - PG writer routing. Task 0220 ships the **CH writer parity** for the
>   routing — `crates/db-clickhouse/src/persist/stage.rs` now reads the
>   per-contract WASM classifier verdict (built alongside
>   `wasm_interface_metadata` staging) and routes NFT-candidate rows
>   into hot vs. pending vs. drop buckets in lockstep with PG.
>
> **Atomicity asymmetry that remains:** PG runs a promotion hook
> (`reclassify_contracts_from_wasm` + `promote_pending_nfts_to_hot`)
> inside the persist tx when an `Other → Nft` transition is observed.
> CH `ReplacingMergeTree` has no per-row UPDATE, so the CH promotion
> path is **re-emission on next observation** plus the post-backfill
> drain runbook for stragglers
> ([`docs/runbooks/0217_nfts_pending_migration_and_drain.md`](../../runbooks/0217_nfts_pending_migration_and_drain.md)).
> In short: PG promotes inline; CH promotes when the contract is
> observed again with a definitive verdict, with the runbook as the
> long-tail catch-all.

Verdict-based routing (verdict source: `soroban_contracts.contract_type`,
which is `Nullable(Int16)` and tracks the `domain::ContractType` enum):

| Classifier verdict   | Target tables                            |
| -------------------- | ---------------------------------------- |
| `Nft` (=2)           | `nfts` + `nft_ownership` (hot)           |
| `Fungible` / `Token` | _none_ (filtered out)                    |
| `Other` (=1) / NULL  | `nfts_pending` + `nft_ownership_pending` |

CH-side schema (see [`init.sql`](../../../crates/db-clickhouse/schema/init.sql)):

```sql
CREATE TABLE IF NOT EXISTS nfts_pending (
    contract_id           Int64,
    token_id              String,
    collection_name       Nullable(String),
    name                  Nullable(String),
    media_url             Nullable(String),
    minted_at_ledger      Nullable(Int64),
    current_owner_id      Nullable(Int64),
    current_owner_ledger  Int64 DEFAULT 0
)
ENGINE = ReplacingMergeTree(current_owner_ledger)
ORDER BY (contract_id, token_id);

CREATE TABLE IF NOT EXISTS nft_ownership_pending (
    contract_id      Int64,
    token_id         String,
    ledger_sequence  Int64,
    event_order      Int16,
    transaction_id   Int64,
    owner_id         Nullable(Int64),
    event_type       Int16
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (contract_id, token_id, ledger_sequence, event_order);
```

Notes:

- The shape is identical to `nfts` / `nft_ownership`; promotion is a
  column-projection `INSERT INTO nfts SELECT … FROM nfts_pending`. The
  partition layout on `nft_ownership_pending` matches `nft_ownership`
  so promotion can move whole parts cleanly under a future part-level
  optimization (not done in the pilot — promotion currently uses a
  row-level INSERT/DELETE pair).
- API endpoints never read the `_pending` tables.
- Operational lifecycle (initial migration + post-backfill drain) is
  documented in
  [`docs/runbooks/0217_nfts_pending_migration_and_drain.md`](../../runbooks/0217_nfts_pending_migration_and_drain.md).
- The architectural decision record for the quarantine pattern
  (alternatives considered, design rationale, consequences) lives in
  [ADR 0046](../../../lore/2-adrs/0046_classifier-quarantine-tables-nfts-pending.md).

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

### Writer-only behaviours not yet ported to CH

- **`soroban_contracts.is_sac` forward-derivation (task 0218)** — the
  PG writer flips `is_sac=true` + `contract_type=Token` on pre-existing
  SAC skeleton rows by deriving the SAC contract_id from every
  observed classic / native asset and running an UPDATE inside the
  persist transaction
  (`crates/indexer/src/handler/persist/write.rs::apply_sac_overrides_for_skeleton_contracts`).
  The CH writer does not yet apply the same override; CH `is_sac` for
  pre-window SACs therefore remains `false` until parity work lands
  (different atomicity model — `ReplacingMergeTree` has no per-row
  UPDATE, so the path is either an `ALTER TABLE … UPDATE` mutation
  or a re-insert that relies on `wasm_uploaded_at_ledger` version
  semantics to absorb the corrected row).

### 4f. Unified balance model (task 0331)

Soroban-token (`asset_type = 3`) `total_supply` / `holder_count` rendered `—` (no
trustlines). The fix is a CH-only **unified per-holder balance model** that also
re-keys classic balances off PG's `Decimal128(7)` `account_balances_current` onto a
raw representation. Two objects, none in PG:

- **`balances`** (`holder_id`, `asset_id`, `amount Int128`, `last_updated_ledger`)
  `ReplacingMergeTree(last_updated_ledger)` — unified per-holder balance, raw
  `Int128` (scale by the asset's `decimals` at read; type-3 decimals vary — PIKA
  43224 overflows any `Decimal`). Read from `ContractData Balance(Address)` ledger
  STATE, never an event-fold (folds drift on vault / rebasing / non-SEP-41-event
  tokens — see README DECISION 2026-06-29). `removed` / spent → `amount = 0`.
  `holder_id = cityhash64(holder StrKey)` — a `G…` account or `C…` contract (~34% of
  type-3 holders are contracts), in the one surrogate space shared with `accounts.id`
  / `soroban_contracts.id`. Resolution back to a StrKey (for portfolio / top-holders)
  is via `accounts` (G) / `soroban_contracts` (C); there is no dedicated address
  dimension.
- **`balance_aggregates`** (`asset_id`, `total_supply Nullable(Int128)`,
  `holder_count Nullable(Int32)`) + **`balance_aggregates_mv`**
  (`REFRESH EVERY 2 MINUTE`) — `sum(amount)` / `countIf(amount > 0)` over
  `balances FINAL`, keyed by the `assets.id` surrogate. One 1:1 read join for ALL
  asset types. **`total_supply` = `sum(amount)` is the SOLE supply source** — task
  0331 Option A: a mint always credits a holder balance (often a contract treasury,
  summed because holders include `C…`), so the sum equals real supply. No per-token
  `TotalSupply` key read (it was optional — only ~73% of wasm expose it — and
  seed-only/stale); the narrow residue (TTL-archived tail + true rebasing) is the
  accepted non-100% cost. See README DECISION 2026-06-30.

The historical set is seeded once by `backfill-runner balance-seed` (RPC snapshot:
holders enumerated from `soroban_events`, `Balance(Address)` entries read via
`getLedgerEntries`); live ingest maintains `balances` forward and supersedes the seed
on catch-up. See the [indexing-pipeline overview §6.2](../indexing-pipeline/indexing-pipeline-overview.md).

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
- `operations_appearances.pool_id` (PG scalar `BYTEA`, nullable) becomes
  `pool_ids Array(FixedString(32))` (task 0261/0268): path payments
  contribute every pool crossed by their result claim atoms, LP
  deposit/withdraw a single element, `[]` = no pool involvement. Filter
  with `has(pool_ids, unhex(...))`; PG keeps the legacy scalar (path
  payments stay NULL there) pending retirement
- `liquidity_pool_snapshots` carries the CH-only `gross_volume_a
Nullable(Decimal128(7))` — per-(pool, ledger) trade volume in asset-A
  units from path-payment claim atoms (derived at ingest by the live
  indexer and, for history, by the 0266 backfill; USD `volume`/`fee_revenue`
  stay NULL until the Prices API, ADR 0053)
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

There are no numbered migrations: the schema is the single `init.sql`
applied by the init sidecar — and production does the same, so the PG-era
`crates/db-migrate` Lambda was removed in the PG→CH cutover (task 0241).
Schema iteration in the pilot is "edit `init.sql`, nuke the volume, restart
compose."

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
[0206](../../../lore/1-tasks/archive/0206_FEATURE_clickhouse-persist-real-inserts/README.md),
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
[`notes/G-coverage-mapping.md`](../../../lore/1-tasks/archive/0206_FEATURE_clickhouse-persist-real-inserts/notes/G-coverage-mapping.md)
enumerates every `Extracted*` field with its CH target column (or
"out of scope — matches PG").

#### State-side ingestion (initial-snapshot mechanism)

The CH writer inherits the parser's emit-on-observed-change pattern
for state-side tables (`accounts`, `account_balances_current`,
`assets`, `liquidity_pools`, `lp_positions`, `soroban_contracts`).
That pattern is bullet-proof when the indexed range happens to
contain every entity's `LedgerEntry*` update — which it usually
doesn't for short backfill windows. The 2026-05-12 CH pilot audit
([§E06](../../audits/2026-05-12-ch-pilot-endpoint-audit.md))
quantified the gap:

- Most accounts that appear in `transaction_participants` over a
  64 k-ledger window are referenced **only** as participants — their
  `AccountEntry` is never touched in the same range. The CH `accounts`
  row therefore persists as a skeleton: `sequence_number = 0,
home_domain = null, account_balances_current rows = 0`.
- Example from the audit: `GARDNV3Q7…` shows skeleton state in CH
  while Horizon reports `seqnum = 148e15, home_domain =
"ultracapital.xyz", balance = 12 861 XLM`.

Task 0214 closes the gap with an **initial-snapshot mechanism** that
runs once per backfill window in
`crates/backfill-runner/src/{rpc_snapshot,bootstrap}.rs`:

1. **Discovery** — JOIN `transaction_participants` (window-filtered)
   against `accounts FINAL` and keep rows where `sequence_number =
0`. The Phase 2 incremental top-up gate is intrinsic to the JOIN:
   already-populated rows are skipped, so a window re-run only
   touches the rows that still need it.
2. **RPC fetch** — batch `LedgerKey::Account(...)` keys (≤ 200 per
   call) and POST `getLedgerEntries` to Soroban RPC. Decode each
   returned `AccountEntry` into `(account_id, sequence_number,
balance, home_domain)`.
3. **Stage** — INSERT into `accounts` (overwriting the skeleton via
   `ReplacingMergeTree(last_seen_ledger)`) and into
   `account_balances_current` for the native XLM row. Snapshot rows
   carry `last_seen_ledger = window_start` as their watermark; a
   per-ledger parser emit at a higher sequence inside the same window
   overwrites the snapshot naturally on the next background merge.
4. **Skip-and-warn** — RPC failure downgrades to a warn-and-return
   instead of failing the run. Bootstrap is opportunistic enrichment;
   the per-ledger ingest path is the load-bearing one.

The runner CLI gates this step on `--soroban-rpc-url` /
`SOROBAN_RPC_URL`. PG-target runs always skip the bootstrap (PG's
account-state coverage is handled by task 0119 + ADR 0027 §7 via a
different path). CH-target runs without the URL log
`bootstrap_account_state skipped — no --soroban-rpc-url configured`
and proceed; the accounts that need enrichment stay as skeletons
until the operator re-runs with the flag set.

##### Why the RPC client lives inside `crates/backfill-runner`

Task 0214 is the first concrete consumer of Soroban RPC. Adjacent
tasks 0218 (SAC override) and 0219 (classic-credit assets) ship
cheaper non-RPC layers and only fall back to RPC for stragglers, so
they don't drive a shared-crate decision yet. The inline module
keeps the blast radius small; the refactor to a
`crates/soroban-rpc-client` crate is a one-day move once a second
concrete consumer lands.

##### What this does not cover (yet)

The Phase 1 implementation snapshots `AccountEntry` only — including
the native XLM balance. The trustline pass
(`LedgerKey::Trustline(...)` for each `(account, asset)` pair
referenced in the window) is left as a Phase 3 follow-up; the
`decode_trustline_snapshot` and `rebuild_trustline_asset` helpers
are already on the public surface of `rpc_snapshot.rs` ready to wire
in when the asset-aggregate port of PG task 0194's
`recompute_asset_aggregates` lands on the CH side.

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
statement. The writer streams into 16 non-ledger tables; `ledgers`
is opened once at commit as the commit marker (17 tables total).
With 16 streaming tables × 11 M ledgers the naive per-ledger pattern
produces ~176 M parts and trips
`parts_to_throw_insert = 3000` (per `(table, CH-partition)`) after
the first ~3 k ledgers — about 0.03 % of an 11 M backfill. The
background merger cannot fold parts faster than they're produced at
parse-bound throughput, so the ingest path stalls.

Partition-aligned streaming holds the request open across the whole
64 k-ledger backfill partition. ~172 partitions × 16 streaming
tables ≈ 2 750 `INSERT` statements over the entire 11 M-ledger
backfill (plus one `ledgers` INSERT per partition as the commit
marker) — well within the merger's comfort zone.

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
full. 16 streaming inserts × 256 KiB ≈ 4 MiB peak per writer (plus
the short-lived `ledgers` insert at commit), independent of
partition row count. Comfortable headroom even at K=16 parallel
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

### Read queries (reference set)

[`endpoint-queries-clickhouse/`](./endpoint-queries-clickhouse/README.md) is
the canonical reference set of read queries for the 23 public REST
endpoints (the retired PostgreSQL reference set was removed with the PG backend,
task 0244). Each query targets the ADR 0044 schema (`init.sql`), uses
`FINAL` on `ReplacingMergeTree` reads, partition-prunes via
`intDiv(ledger_sequence, 500000)`, and resolves `closed_at` via JOIN to
`ledgers` per §5.2. Driving task: [0207](../../../lore/1-tasks/archive/0207_FEATURE_clickhouse-endpoint-queries-reference-set.md).

> **CH 26.3 gotcha — no correlated subqueries (task 0243).** The reference
> set was authored as a spec and never executed against a live cluster; the
> transaction-list queries (`02`, `05`, `07`, `10`, `20`) compute
> `operation_types` / `contract_ids` with **correlated** scalar subqueries in
> the SELECT projection (`… WHERE oa.transaction_id = t.id`). ClickHouse
> 26.3.10.60 rejects that at runtime — `Code: 48 NOT_IMPLEMENTED: can't find
correlated column …`. The live read path instead fetches the page of tx
> keys, then aggregates per `(ledger_sequence, transaction_id) IN (…)` with
> `GROUP BY transaction_id` (non-correlated), merged in Rust. The shared
> implementation is
> [`crates/api/src/common/ch.rs::fetch_tx_list_aggregates`](../../../crates/api/src/common/ch.rs);
> reuse it for any new transaction-list module rather than the inline
> correlated projection the reference SQL still shows (those files carry a
> correction banner).

> **`contract_ids` REMOVED from the API (task 0386).** The per-row
> `contract_ids` array is no longer returned by any transaction-list endpoint —
> a dead PG-parity field no frontend rendered, whose only cost was a whole-table
> `JOIN soroban_contracts FINAL` (~200k rows/page). The live list response
> carries `operation_types` only. The ops-only note below is kept for history.
>
> **CH read-cost correction — `contract_ids` was ops-only (task 0243).** The
> reference SQL builds `contract_ids` from a 3-source UNION
> (`operations_appearances` + `soroban_invocations_appearances` +
> `soroban_events`) for full PG parity. Both `soroban_*` tables are
> `ORDER BY (contract_id, …)`, so the per-page
> `(ledger_sequence, transaction_id) IN (…)` key filter is a **partition scan**
> on them, not a key seek. In production a single `/transactions` page read
> ~1e8 rows and a handful of requests exhausted the `api_reader` `read_rows`
> hourly quota (`Code: 201 QUOTA_EXCEEDED`), 500-ing every CH endpoint. The
> live helper therefore sources `contract_ids` from `operations_appearances` > **only** (primary-key seek, ~hundreds of rows/page). **Parity cost:** a
> contract touched solely via a nested sub-invocation or an emitted event
> (never a root-op `contract_id`) is not listed; for the vast majority of
> Soroban transactions the invoked contract IS the root-op `contract_id`, so
> list-row `contract_ids` match PG in practice. A cheap full-parity path
> (skip-index on `transaction_id`, or a precomputed per-tx `contract_ids`
> column written at ingest) is a deferred follow-up.

> **CH read-cost correction — list reads must stay on the primary key (task
> 0243).** `... FINAL ... ORDER BY ... LIMIT` over a partition reads the
> **whole partition** — FINAL merges it before the limit applies (~1.2e8
> transactions on the mainnet head). Under frontend polling this exhausted the
> `api_reader` `read_rows` hourly quota (CH `Code: 201`). The live read paths
> were corrected to read in primary-key order:
>
> - **transactions list** (no filter, polled) drops FINAL and orders/keys on
>   `(ledger_sequence, application_order)` — the table's physical sort key — so
>   CH stops at the limit (~2e5 rows/page, validated). The cursor tie-break is
>   `application_order` (also the correct in-ledger order; the `id` hash
>   surrogate did not preserve it). The filtered statements still key on `id`.
> - **filtered transactions list** (contract / op_type) must not join the
>   driver to an unpruned `transactions FINAL` — that merges the whole 3.6B-row
>   table per request (measured ~1e9+ rows; blew the quota). `transactions` is
>   pruned to the driver's partition and streamed, the driver is the hash side
>   (~2e8 rows/page, validated). Making the driver itself a seek (it scans the
>   partition by `type` / `contract_id`, neither a PK prefix) needs a
>   skip-index — deferred follow-up.
> - **ledgers list** + **network stats** are ORDER BY `sequence`, not
>   `closed_at`; both now drive off `sequence` (monotonic with `closed_at`) to
>   stay on the primary key instead of scanning the ~12M-row table per request.
>
> FINAL is retained only for single-key / per-page key-seek reads (transaction
> detail, embedded ledger transactions, the aggregate helper), where it is
> cheap. This is the general rule for any new CH read path: a polled or
> list-shaped query must read in primary-key order; reserve FINAL for reads
> already bounded to a key seek.

---

## References

- [ADR 0044](../../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md) — pilot decision and resolved open questions
- [ADR 0033](../../../lore/2-adrs/0033_soroban-events-appearances-read-time-detail.md) — folded events design that this pilot deliberately reverses on the CH side
- [ADR 0032](../../../lore/2-adrs/0032_docs-architecture-evergreen-maintenance.md) — evergreen docs maintenance policy
- [Task 0204](../../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/README.md) — implementation task
- [Task 0207](../../../lore/1-tasks/archive/0207_FEATURE_clickhouse-endpoint-queries-reference-set.md) — CH endpoint queries reference set
- [`crates/db-clickhouse/README.md`](../../../crates/db-clickhouse/README.md) — crate-level README with translation table and dev workflow
- [`endpoint-queries-clickhouse/README.md`](./endpoint-queries-clickhouse/README.md) — 23 CH-side endpoint queries + FINAL/Dict/§5 conventions
- [`notes/G-clickhouse-schema-er.md`](../../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/notes/G-clickhouse-schema-er.md) — full ER diagram + ENGINE/PARTITION BY/ORDER BY matrix
