---
id: '0204'
title: 'ClickHouse pilot — db-clickhouse crate, Docker service, mirrored schema'
type: FEATURE
status: active
related_adr: ['0044']
related_tasks: []
tags:
  [
    layer-backend,
    layer-db,
    layer-infra,
    clickhouse,
    schema,
    docker,
    pilot,
    non-invasive,
    effort-medium,
    priority-medium,
  ]
links:
  - lore/2-adrs/0044_clickhouse-pilot-parallel-store.md
  - sources/db-schema-snapshot.md
history:
  - date: '2026-05-08'
    status: backlog
    who: fmazur
    note: >
      Spawned from ADR 0044. Implements the pilot infrastructure:
      crates/db-clickhouse + docker-compose clickhouse service + ClickHouse
      schema mirroring the Postgres logical shape, with full-content
      soroban_events replacing soroban_events_appearances. Read-empty pilot —
      no indexer dual-write, no API reads from ClickHouse in scope of this
      task.
  - date: '2026-05-08'
    status: backlog
    who: fmazur
    note: >
      ADR 0044 review resolved 6 of 7 open questions. Step 1 (client) and
      Step 5 (migrations) are no longer choices — official `clickhouse`
      crate latest stable, single idempotent `init.sql`. Step 3 schema
      now carries concrete translation rules: engine per "category"
      (Replacing for fact + state, plain MergeTree for immutable
      lookup), `PARTITION BY intDiv(ledger_sequence, 500000)`,
      `created_at` dropped from CH side except `ledgers.closed_at` (PG
      unchanged), `nfts.metadata` dropped (CH only, PG unchanged),
      `_sqlx_migrations` not mirrored, `transaction_hash_index` kept
      and accelerated by a CH `Dictionary` (cache layout). CH net schema:
      17 tables + 1 Dictionary. ER diagram archived in
      `notes/G-clickhouse-schema-er.md`.
  - date: '2026-05-08'
    status: active
    who: fmazur
    note: 'Promoted to active to begin implementation.'
---

# ClickHouse pilot — db-clickhouse crate, Docker service, mirrored schema

## Summary

Stand up the parallel ClickHouse store described in [ADR
0044](../../../2-adrs/0044_clickhouse-pilot-parallel-store.md): a new
`crates/db-clickhouse` crate, a `clickhouse` service in `docker-compose.yml`,
and a ClickHouse schema that mirrors the current Postgres `public` schema
(snapshot dated 2026-05-08) with the **five deliberate divergences**
listed in ADR 0044 §Decision §4 — `soroban_events_appearances` replaced
by full-content `soroban_events`; `created_at` dropped from every CH
table except `ledgers`; `nfts.metadata` dropped (CH only); `_sqlx_migrations`
not mirrored (replaced by `init.sql`); `transaction_hash_index` accessed
via a CH `Dictionary`. **Postgres is unchanged by all five.**

The pilot is **read-empty** in scope of this task: no indexer dual-write,
no API reads against ClickHouse. The crate ships the schema, the migration
mechanism, and the connection layer; populating the store and using it for
queries are follow-up ADRs/tasks.

## Context

ADR 0044 captures the team's decision to evaluate ClickHouse next to
Postgres before committing to any migration. The non-invasive contract is
strict: no file under `crates/{api,indexer,domain,db,db-merge,db-migrate,
db-partition-mgmt,xdr-parser,backfill-runner,audit-harness,backfill-bench}`
may be touched by this task's PR. Allowed changes outside `crates/db-clickhouse/`:
`Cargo.toml` workspace members, `docker-compose.yml` (new service),
`docs/architecture/**` (per ADR 0032), and lore.

The Postgres schema this task mirrors is captured in
[`sources/db-schema-snapshot.md`](sources/db-schema-snapshot.md) (taken on
2026-05-08, 11 regular + 7 partitioned tables, 18 total).

## Implementation Plan

### Step 1 — ClickHouse client crate (decided)

Use the **official `clickhouse` crate from crates.io** (maintained by
ClickHouse Inc.), pinned to the **latest stable** version at the time
this task starts. Document the version pin in `crates/db-clickhouse/README.md`
along with a one-line rationale (vendor-tracked, covers HTTP + native
protocols, Serde-derive integration). Decision recorded in ADR 0044
§Decision §5; revisit only if blocked by a specific bug.

### Step 2 — Create `crates/db-clickhouse`

- New library crate, registered in workspace `Cargo.toml`
- `src/lib.rs` exposes a connection-pool factory + a typed query layer
  (or a thin wrapper around the chosen client)
- `src/schema/` holds the SQL DDL (one file per logical table grouping,
  matching the Postgres grouping convention from `crates/db/migrations/`
  if helpful)
- `src/bin/db-clickhouse-init.rs` CLI: applies the schema to a target
  ClickHouse instance, idempotently. Usable both from `docker compose`
  startup and from local dev (`cargo run -p db-clickhouse --bin db-clickhouse-init`)
- `README.md` with: client choice rationale, type-translation table
  (Postgres → ClickHouse), partitioning strategy, how to apply schema
  locally, how to drop and recreate

### Step 3 — Author the ClickHouse schema (decisions resolved in ADR 0044)

**All decisions below are CH-side only. Postgres schema is unchanged
by this task — no migration, no column drop, no engine change on the
PG side.** Reference: `sources/db-schema-snapshot.md` is the PG state
to mirror; the divergences in §Decision §4 of ADR 0044 are the **only**
deltas, and they live exclusively in `crates/db-clickhouse/schema/init.sql`.

#### Translation rules (canonical)

| Postgres                                            | ClickHouse                                                         |
| --------------------------------------------------- | ------------------------------------------------------------------ |
| `BIGINT` / `bigserial`                              | `Int64`                                                            |
| `INTEGER` / `serial`                                | `Int32`                                                            |
| `SMALLINT`                                          | `Int16`                                                            |
| `BOOLEAN`                                           | `Bool`                                                             |
| `VARCHAR(N)` / `TEXT` / variable `BYTEA`            | `String`                                                           |
| 32-byte `BYTEA` (hashes, `pool_id`)                 | `FixedString(32)`                                                  |
| `NUMERIC(28,7)`                                     | `Decimal128(7)`                                                    |
| `JSONB` (only `wasm_interface_metadata.metadata`)   | `String`                                                           |
| `JSONB` (`nfts.metadata`)                           | **OMITTED** (column dropped from CH copy of `nfts`; PG keeps it)   |
| `TIMESTAMPTZ` (only `ledgers.closed_at`)            | `DateTime64(3, 'UTC')`                                             |
| `TIMESTAMPTZ created_at` (every other table)        | **OMITTED** (column dropped from CH copy; PG keeps it)             |
| `tsvector` (only `soroban_contracts.search_vector`) | OMITTED                                                            |
| `PARTITION BY RANGE (created_at)`                   | `PARTITION BY intDiv(ledger_sequence, 500000)`                     |
| Postgres `PRIMARY KEY (…)`                          | `ORDER BY (…)` with `ledger_sequence` substituted for `created_at` |
| FK constraints                                      | OMIT (not enforceable in CH)                                       |
| CHECK constraints                                   | OMIT for the pilot                                                 |
| GIN / `pg_trgm` indexes                             | OMIT (no equivalent)                                               |
| Partial unique indexes                              | OMIT (no enforcement); document in the README                      |

#### Engine choice per table "category" (resolves Q1)

- **Append-only fact tables** (`transactions`, `operations_appearances`,
  `transaction_participants`, `nft_ownership`, `liquidity_pool_snapshots`,
  `soroban_events`, `soroban_invocations_appearances`,
  `transaction_hash_index`) → `ReplacingMergeTree`. Replay safety on
  re-ingest comes from background dedup-by-`ORDER BY`-key.
- **State tables** (`accounts`, `assets`, `account_balances_current`,
  `nfts`, `lp_positions`, `soroban_contracts`) →
  `ReplacingMergeTree(version_column)` with the version column being
  the natural "latest update" column already in the schema
  (`last_updated_ledger`, `last_seen_ledger`, `current_owner_ledger`,
  `wasm_uploaded_at_ledger`).
- **Immutable lookup** (`ledgers`, `liquidity_pools`,
  `wasm_interface_metadata`) → plain `MergeTree`.

#### Partitioning + ordering (resolves Q2)

Every partitioned fact table uses `PARTITION BY intDiv(ledger_sequence,
500000)`. State and immutable-lookup tables are typically small enough
that they don't need partitioning — pick per table during implementation
(default: no partition for tables under ~10M rows projected).

`ORDER BY` substitutes `ledger_sequence` for the dropped `created_at` in
every fact table. See `notes/G-clickhouse-schema-er.md` for the full
table-by-table ENGINE / PARTITION BY / ORDER BY matrix.

#### Tables to produce: 17 (CH side) + 1 Dictionary

| #   | CH table                               | PG counterpart                                  | Notes                                                           |
| --- | -------------------------------------- | ----------------------------------------------- | --------------------------------------------------------------- |
| 1   | `accounts`                             | `accounts`                                      | state                                                           |
| 2   | `account_balances_current`             | `account_balances_current`                      | state                                                           |
| 3   | `assets`                               | `assets`                                        | state                                                           |
| 4   | `ledgers`                              | `ledgers`                                       | immutable; **only** table with wall-clock `closed_at`           |
| 5   | `liquidity_pools`                      | `liquidity_pools`                               | immutable post-create                                           |
| 6   | `liquidity_pool_snapshots`             | `liquidity_pool_snapshots`                      | append-only fact, no `created_at`                               |
| 7   | `lp_positions`                         | `lp_positions`                                  | state                                                           |
| 8   | `nfts`                                 | `nfts`                                          | state, **drops `metadata` column**                              |
| 9   | `nft_ownership`                        | `nft_ownership`                                 | append-only fact, no `created_at`                               |
| 10  | `operations_appearances`               | `operations_appearances`                        | append-only fact, no `created_at`                               |
| 11  | `soroban_contracts`                    | `soroban_contracts`                             | state, drops `search_vector`                                    |
| 12  | `soroban_events`                       | **NEW** (replaces `soroban_events_appearances`) | full-content per-event row                                      |
| 13  | `soroban_invocations_appearances`      | `soroban_invocations_appearances`               | append-only fact, no `created_at`                               |
| 14  | `transactions`                         | `transactions`                                  | append-only fact, no `created_at`                               |
| 15  | `transaction_hash_index`               | `transaction_hash_index`                        | source for the Dictionary, no `created_at`                      |
| 16  | `transaction_participants`             | `transaction_participants`                      | append-only fact, no `created_at`                               |
| 17  | `wasm_interface_metadata`              | `wasm_interface_metadata`                       | immutable; `metadata` is `String`                               |
| —   | ~~`_sqlx_migrations`~~                 | `_sqlx_migrations`                              | **NOT MIRRORED** — pilot uses `init.sql`                        |
| D1  | `transaction_hash_dict` (`DICTIONARY`) | —                                               | sourced from `transaction_hash_index`, complex_key_cache layout |

#### `soroban_events` canonical DDL

PG reference (this is the v3 spec; **PG itself does not have this table**
— the spec is the design target for CH only):

```sql
CREATE TABLE soroban_events (
    contract_id     BIGINT       NOT NULL REFERENCES soroban_contracts(id),
    transaction_id  BIGINT       NOT NULL,
    ledger_sequence BIGINT       NOT NULL,
    event_index     SMALLINT     NOT NULL,
    event_type      SMALLINT     NOT NULL,
    signature       TEXT,
    topics_xdr      BYTEA        NOT NULL,
    data_xdr        BYTEA        NOT NULL,
    created_at      TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (contract_id, created_at, transaction_id, event_index),
    FOREIGN KEY (transaction_id, created_at)
        REFERENCES transactions (id, created_at) ON DELETE CASCADE,
    CONSTRAINT ck_se_v3_type_range CHECK (event_type BETWEEN 0 AND 2),
    CONSTRAINT ck_se_v3_index_pos  CHECK (event_index >= 0)
) PARTITION BY RANGE (created_at);
```

ClickHouse equivalent (this is what the pilot writes to `init.sql`):

```sql
CREATE TABLE soroban_events (
    contract_id     Int64,
    transaction_id  Int64,
    ledger_sequence Int64,
    event_index     Int16,
    event_type      Int16,
    signature       Nullable(String),
    topics_xdr      String,
    data_xdr        String
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (contract_id, ledger_sequence, transaction_id, event_index);
```

(`created_at` omitted — recover via JOIN on `ledgers.closed_at` when
needed; typed columns in CH version reflect the type translation rules
above.)

#### `transaction_hash_dict` Dictionary DDL (resolves Q7)

```sql
CREATE DICTIONARY transaction_hash_dict (
    hash FixedString(32),
    ledger_sequence Int64
)
PRIMARY KEY hash
SOURCE(CLICKHOUSE(TABLE 'transaction_hash_index' DB 'default'))
LIFETIME(MIN 300 MAX 360)
LAYOUT(COMPLEX_KEY_CACHE(SIZE_IN_CELLS 1000000));
```

API queries use `dictGet('transaction_hash_dict', 'ledger_sequence',
tuple(toFixedString(?, 32)))` to resolve `hash → ledger_sequence` in
microseconds. Misses fall back to scanning `transaction_hash_index`
itself (~5 ms).

### Step 4 — Add `clickhouse` service to `docker-compose.yml`

- Image: `clickhouse/clickhouse-server:24` (pin a recent stable major)
- Ports: `8123:8123` (HTTP), `9000:9000` (native)
- Volume: `clickhouse-data:/var/lib/clickhouse`
- Healthcheck: `wget -qO- http://localhost:8123/ping || exit 1`
- No compose profile — starts on `docker compose up` like Postgres
- Default credentials: matches the local-dev posture of the Postgres
  service (no production secrets in the file)

### Step 5 — Wire schema apply into local dev (decision resolved)

**Schema is applied via a single idempotent `init.sql`** (resolves Q5
in ADR 0044). No numbered migration ladder for the pilot — the schema
is read-empty and will iterate; numbered migrations are deferred to
the dual-write follow-up task.

Mechanism: `db-clickhouse-init` CLI runs from a **sidecar compose
service** that depends on `clickhouse` healthy and applies
`init.sql`. Survives `docker compose down -v` cleanly because the CLI
is idempotent (`CREATE TABLE IF NOT EXISTS` + `CREATE DICTIONARY`
re-applies as no-op).

The dev iteration workflow is documented in
`crates/db-clickhouse/README.md`:

1. Edit `crates/db-clickhouse/schema/init.sql`
2. `docker compose down -v` (nukes volume — pilot is read-empty so
   safe)
3. `docker compose up`
4. Sidecar applies the new `init.sql` to fresh ClickHouse

### Step 6 — Documentation

- `crates/db-clickhouse/README.md` — client choice, translation rules,
  how to apply schema locally, how to query
- `docs/architecture/database-schema/clickhouse-pilot.md` (new) — pilot
  scope, schema parity table (Postgres ↔ ClickHouse), divergences
  (`soroban_events` shape, constraints/indexes that don't translate)
- `docs/architecture/database-schema/database-schema-overview.md` — add
  a "ClickHouse pilot" subsection linking to the new doc
- `docs/architecture/infrastructure/infrastructure-overview.md` — note
  the new compose service in the local-dev section
- `docs/architecture/technical-design-general-overview.md` — one-line
  pointer to the pilot in the data-store section
- Each touched doc carries a link back to ADR 0044

### Step 7 — Smoke test

A `cargo test -p db-clickhouse` integration test that:

1. Connects to a running ClickHouse at `${CLICKHOUSE_URL:-http://localhost:8123}`
2. Applies the schema (or asserts it's already applied)
3. Inserts one row into each of the 17 tables (with FK-shaped data
   referencing plausible ID values — but not enforced in CH)
4. Reads each row back
5. **Verifies the `transaction_hash_dict` Dictionary**: insert a row
   into `transaction_hash_index`, wait for Dictionary refresh window
   (or call `SYSTEM RELOAD DICTIONARY transaction_hash_dict`), confirm
   `dictGet('transaction_hash_dict', 'ledger_sequence', tuple(...))`
   returns the expected `ledger_sequence`
6. Drops the test data

Gate the test on `CLICKHOUSE_URL` env so CI without a ClickHouse
instance skips it cleanly (`#[ignore]` if env unset).

## Acceptance Criteria

- [ ] `crates/db-clickhouse/` exists in the workspace, builds with
      `cargo check -p db-clickhouse` clean, lints with
      `cargo clippy -p db-clickhouse -- -D warnings` clean
- [ ] `docker compose up` starts both `postgres` and `clickhouse`
      services healthy without manual flags
- [ ] All **17** CH tables (16 mirrored from PG + new `soroban_events`)
      apply to a fresh ClickHouse instance without error
- [ ] `transaction_hash_dict` Dictionary applies and answers
      `dictGet(...)` lookups against rows inserted into
      `transaction_hash_index` (smoke-test verified)
- [ ] **Postgres schema unchanged** — no migration added under
      `crates/db/migrations/`, no column dropped from any PG table
      (verified via `git diff` against `crates/db/`)
- [ ] `db-clickhouse-init` CLI is idempotent (running it twice on the
      same instance is a no-op)
- [ ] Translation table (Postgres → ClickHouse) lives in
      `crates/db-clickhouse/README.md` and matches what the schema
      actually does, including the four CH-side drops
      (`_sqlx_migrations`, `nfts.metadata`, `created_at` on fact
      tables, `soroban_contracts.search_vector`)
- [ ] Engine-per-`category` rule applied (append-only fact + state →
      `ReplacingMergeTree`; immutable lookup → `MergeTree`); each
      `CREATE TABLE` carries the right ENGINE per ADR 0044 §Decision §5
- [ ] All partitioned tables use
      `PARTITION BY intDiv(ledger_sequence, 500000)`
- [ ] Smoke test inserts and reads back one row in each of the 17
      tables (gated on `CLICKHOUSE_URL` env)
- [ ] No file under `crates/{api,indexer,domain,db,db-merge,db-migrate,
  db-partition-mgmt,xdr-parser,backfill-runner,audit-harness,
  backfill-bench}` is modified by this PR (verified via
      `git diff --stat`)
- [ ] **Docs updated** — `docs/architecture/database-schema/clickhouse-pilot.md`
      created; `database-schema-overview.md`,
      `infrastructure/infrastructure-overview.md`,
      `technical-design-general-overview.md` updated per ADR 0032; each
      links back to ADR 0044
- [ ] **API types regenerated** — `N/A — pilot does not touch `crates/api/**`,
  `Cargo.{toml,lock}`workspace-member additions only, no schema
  change to`libs/api-types/**`

## Out of Scope

- Indexer dual-write to ClickHouse — separate ADR + task once the pilot
  schema is stable
- API read-path A/B against ClickHouse — separate ADR + task
- Backfilling existing Postgres data into ClickHouse — separate task
- Any retirement / deprecation of Postgres tables — explicitly deferred
  to the migrate-or-retire decision after pilot measurements
- Performance benchmarking — schema-only landing in this task; the
  follow-up ADR with success criteria comes first

## Notes

- The Postgres schema this task mirrors is captured in
  [`sources/db-schema-snapshot.md`](sources/db-schema-snapshot.md) (dated
  2026-05-08). If that snapshot is regenerated and tables move/add/drop
  before this task lands, sync the ClickHouse schema before merging.
- ADR 0044 §"Open Questions" originally listed seven open items.
  After the 2026-05-08 ADR review, six were resolved (Q1 engine choice,
  Q2 partitioning, Q3 JSONB, Q4 client, Q5 migrations, Q7 lookup
  tables) and folded into ADR 0044 §Decision §4–§5. A new Q8 (bucket
  size for `intDiv(ledger_sequence, ?)`) surfaced from Q2 and was
  closed in the same session at **500 000 ledgers ≈ 29 days**. Only
  Q6 (pilot success criteria) remains genuinely open — deferred to a
  follow-up ADR after first measurements. Implementer follows every
  resolved decision; doesn't relitigate.
- Full table-by-table ENGINE / PARTITION BY / ORDER BY matrix +
  Mermaid ER diagram lives in
  [`notes/G-clickhouse-schema-er.md`](notes/G-clickhouse-schema-er.md).
- The compose service will increase the local-dev memory footprint by
  the ClickHouse server's idle baseline (a few hundred MB). Acceptable;
  flag in the README if it becomes a problem.
