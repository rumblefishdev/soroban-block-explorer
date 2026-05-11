# db-clickhouse

ClickHouse pilot store for the Soroban block explorer
([ADR 0044](../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md),
[task 0204](../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/README.md)).

This crate is **read-empty** in scope: it ships the schema, the schema-apply
mechanism, and the connection layer. Indexer dual-write and API read-path
A/B against ClickHouse are deliberately deferred to follow-up ADRs/tasks.

## Quick start (local dev)

All commands run from the repo root.

### 1. Prepare `.env`

Copy the template once. The defaults match `docker-compose.yml`.

```bash
cp .env.example .env
```

The relevant block (already in `.env.example`):

```dotenv
CLICKHOUSE_HTTP_PORT=8123
CLICKHOUSE_NATIVE_PORT=9000
CLICKHOUSE_URL=http://localhost:8123
CLICKHOUSE_USER=default
CLICKHOUSE_PASSWORD=clickhouse
```

If something already binds `8123` / `9000` on your host (e.g. an unrelated
ClickHouse container), bump the host ports — e.g. `CLICKHOUSE_HTTP_PORT=8124`,
`CLICKHOUSE_NATIVE_PORT=9001`, `CLICKHOUSE_URL=http://localhost:8124`.

> **Do not change `CLICKHOUSE_PASSWORD`** without also editing the literal
> in `crates/db-clickhouse/schema/init.sql` (`transaction_hash_dict` SOURCE
> clause). Diverging breaks the dictionary load.

### 2. Bring up the stack

```bash
docker compose up -d clickhouse db-clickhouse-init
docker compose logs db-clickhouse-init    # expect: "exited with code 0"
```

Two containers come up:
- `sorban-block-explorer-clickhouse-1` — the long-running ClickHouse server
- `sorban-block-explorer-db-clickhouse-init-1` — sidecar that applies
  `schema/init.sql` once and exits 0. Idempotent: re-running compose
  re-applies the file as a no-op.

### 3. Verify

```bash
# 18 = 17 tables + transaction_hash_dict
docker exec sorban-block-explorer-clickhouse-1 \
  clickhouse-client --user=default --password=clickhouse \
  -q "SELECT count() FROM system.tables WHERE database='default'"

# Interactive client
docker exec -it sorban-block-explorer-clickhouse-1 \
  clickhouse-client --user=default --password=clickhouse

# End-to-end smoke (insert + read in each table + Dictionary lookup)
set -a; source .env; set +a
cargo test -p db-clickhouse --test smoke -- --nocapture
```

> If your VS Code Python extension popup said "enable
> `python.terminal.useEnvFile`", turning it on auto-injects `.env` into
> every integrated terminal — you can skip the `set -a; source .env`
> line for cargo commands.

### 4. Iterate on the schema

The pilot is read-empty, so wiping the ClickHouse volume is safe — but
do it **scoped to ClickHouse only**, not the whole compose project (see
the warning in §6):

```bash
# 1. Edit crates/db-clickhouse/schema/init.sql
# 2. Reset only ClickHouse, then bring the pilot back up
docker compose stop clickhouse db-clickhouse-init
docker compose rm -f clickhouse db-clickhouse-init
docker volume rm sorban-block-explorer_clickhouse-data
docker compose up -d clickhouse db-clickhouse-init
```

Postgres + any other running compose service stays up and untouched.

Once a follow-up ADR introduces dual-write, this loop must be replaced
by a numbered migration ladder (see "What this crate intentionally does
NOT do" near the bottom).

### 5. Tear down

Stop only the ClickHouse pilot (Postgres + others keep running):

```bash
docker compose stop clickhouse db-clickhouse-init     # pause; volume + schema retained
docker compose rm -f clickhouse db-clickhouse-init    # also remove the containers
```

To stop **everything** in the compose file (including Postgres), and
optionally wipe **all** project volumes (incl. `pgdata`!), see
`docker compose down [-v]` — but be sure that's actually what you
want. For the ClickHouse-only nuke, use §6.

### 6. Full reset (clean local ClickHouse only)

Use this when you want to start over with a guaranteed-empty ClickHouse —
e.g. after diverging schema edits, when testing replay logic, or when the
container state has gotten weird. **The recipe below is surgical: it
touches only `clickhouse` and `db-clickhouse-init`. Postgres, db-merge,
and any other service in `docker-compose.yml` are left running with
their data intact.**

> **Why not `docker compose down -v`?** That command (without a service
> name) tears down **every** service in the compose file and wipes
> **every** named volume — including `pgdata`. If you have local
> Postgres data you care about (indexer-populated dev DB, db-merge
> snapshots), that's catastrophic. Always scope by service name when
> resetting just one DB.


The clickhouse volume is named `<project>_clickhouse-data` where
`<project>` defaults to the directory name docker compose was invoked
from (e.g. `soroban-block-explorer_clickhouse-data` for a fresh clone).
Confirm the exact name with `docker volume ls | grep clickhouse`
before removing — typos silently succeed.

```bash
docker compose rm -sfv clickhouse db-clickhouse-init
docker volume ls | grep clickhouse   # confirm the actual name
docker volume rm <copy-the-name-from-the-line-above>
docker compose up -d clickhouse db-clickhouse-init
docker compose logs db-clickhouse-init | tail -5
docker exec -i $(docker ps -qf name=clickhouse) clickhouse-client \
  -u default --password clickhouse -q \
  "SELECT count() FROM system.tables WHERE database='default'"

cargo run --release -p backfill-runner -- \
--target clickhouse \
--keep-partitions \
--verbose \
run --start 62016000 --end 62016099

```

```bash
# 1. Stop + remove only the ClickHouse-related containers.
docker compose stop clickhouse db-clickhouse-init
docker compose rm -f clickhouse db-clickhouse-init

# 2. Remove only the ClickHouse named volume by exact name.
#    (`pgdata`, `pgdata-merge`, `pgdata-snapshot-source` survive.)
docker volume rm sorban-block-explorer_clickhouse-data

# 3. Sanity check — both should print nothing.
docker ps -a --filter 'name=sorban-block-explorer-clickhouse'
docker volume ls | grep clickhouse-data

# 4. (Optional) inspect any other ClickHouse containers/volumes from
#    older sessions you may want to prune separately. Read before
#    removing — this is irreversible.
docker ps -a --filter 'ancestor=clickhouse/clickhouse-server' \
              --format '{{.ID}}\t{{.Names}}\t{{.Image}}'
docker volume ls --filter 'name=clickhouse'

# 5. Bring the pilot back up. The sidecar re-applies init.sql to a
#    fresh, empty database.
docker compose up -d clickhouse db-clickhouse-init
docker compose logs db-clickhouse-init    # expect: exited with code 0

# 6. Verify the clean schema applied correctly (18 = 17 tables + dict).
docker exec sorban-block-explorer-clickhouse-1 \
  clickhouse-client --user=default --password=clickhouse \
  -q "SELECT count() FROM system.tables WHERE database='default'"
```

> **Destructive — no undo for ClickHouse rows.** Steps 1–2 permanently
> delete every row in the pilot ClickHouse. The pilot is read-empty by
> design (no indexer dual-write, no API reads), so the only thing you
> can lose is your own ad-hoc dev data. Postgres + every other compose
> service is untouched.

### Common pitfalls

| Symptom                                                       | Cause                                           | Fix                                                              |
| ------------------------------------------------------------- | ----------------------------------------------- | ---------------------------------------------------------------- |
| `port is already allocated` on `8123`/`9000`                  | Another local ClickHouse holds the port         | Bump `CLICKHOUSE_HTTP_PORT`/`CLICKHOUSE_NATIVE_PORT` in `.env`    |
| `Authentication failed: password is incorrect`                | `.env` `CLICKHOUSE_PASSWORD` ≠ compose value    | Match the value in `docker-compose.yml` (default `clickhouse`)   |
| Dictionary load fails after schema apply                      | `CLICKHOUSE_PASSWORD` ≠ literal in `init.sql`   | Either revert the env, or change both `init.sql` + compose       |
| `Storage MergeTree doesn't support FINAL` in queries          | `FINAL` only valid on `ReplacingMergeTree`      | Drop `FINAL` for plain MergeTree (`ledgers`, `liquidity_pools`,  |
|                                                               |                                                 | `wasm_interface_metadata`)                                       |
| Smoke test reports `CLICKHOUSE_URL not set — skipping`        | env not exported to the cargo subshell          | `set -a; source .env; set +a` first                              |

## What lives here

```
crates/db-clickhouse/
├── Cargo.toml
├── README.md                       (this file)
├── schema/
│   └── init.sql                    17 tables + 1 Dictionary, idempotent
├── src/
│   ├── lib.rs                      Config + client factory + `apply_init_sql`
│   └── bin/db-clickhouse-init.rs   CLI: applies init.sql to a target instance
└── tests/
    └── smoke.rs                    end-to-end smoke (gated on CLICKHOUSE_URL)
```

## Client choice

[`clickhouse`](https://crates.io/crates/clickhouse) crate v0.15.0 — the
official client maintained by ClickHouse Inc. (resolves
[ADR 0044 §Decision §5 Q4](../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md)).
Pinned exactly in `Cargo.toml`; bump deliberately in a follow-up PR if blocked
by a specific bug. Covers HTTP and native protocols and integrates with serde
for typed inserts.

## ClickHouse server

`clickhouse/clickhouse-server:26.3` (current LTS at task 0204 implementation
time, 2026-05-08). Runs as the `clickhouse` service in `docker-compose.yml`,
exposing HTTP (`8123`) and native (`9000`) ports.

Local-dev credentials: `default` / `clickhouse` (matches the Postgres service
posture — no production secrets in the file). Override via
`CLICKHOUSE_HTTP_PORT`, `CLICKHOUSE_NATIVE_PORT`, `CLICKHOUSE_USER`,
`CLICKHOUSE_PASSWORD` env vars when there is already something on those ports
(e.g. an unrelated ClickHouse running on the host).

## Schema overview

17 tables + 1 Dictionary, mirroring the Postgres `public` schema snapshot
taken on 2026-05-08
([sources/db-schema-snapshot.md](../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/sources/db-schema-snapshot.md))
with the five deliberate divergences from
[ADR 0044 §Decision §4](../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md):

1. `soroban_events_appearances` is replaced by full-content
   `soroban_events` (per-event row, full XDR inlined).
2. `created_at` is dropped from every CH table except `ledgers`. Wall-clock
   time is recovered via JOIN to `ledgers.closed_at` when needed.
3. `nfts.metadata` is dropped (CH only).
4. `_sqlx_migrations` is not mirrored — `init.sql` is the migration.
5. `transaction_hash_index` is exposed as a `Dictionary`
   (`transaction_hash_dict`, complex_key_cache layout) for fast
   `hash → ledger_sequence` lookups.

**Postgres is unchanged by all five divergences.**

The full table-by-table ENGINE / PARTITION BY / ORDER BY matrix lives in
[`notes/G-clickhouse-schema-er.md`](../../lore/1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/notes/G-clickhouse-schema-er.md).

## Type translation table

| PostgreSQL                                          | ClickHouse                                                                  |
| --------------------------------------------------- | --------------------------------------------------------------------------- |
| `BIGINT` / `bigserial`                              | `Int64`                                                                     |
| `INTEGER` / `serial`                                | `Int32`                                                                     |
| `SMALLINT`                                          | `Int16`                                                                     |
| `BOOLEAN`                                           | `Bool`                                                                      |
| `VARCHAR(N)` / `TEXT` / variable `BYTEA`            | `String`                                                                    |
| 32-byte `BYTEA` (hashes, `pool_id`, `wasm_hash`)    | `FixedString(32)`                                                           |
| `NUMERIC(28,7)`                                     | `Decimal128(7)`                                                             |
| `JSONB` (only `wasm_interface_metadata.metadata`)   | `String`                                                                    |
| `JSONB` (`nfts.metadata`)                           | **OMITTED** (column dropped on the CH side; PG keeps it)                    |
| `TIMESTAMPTZ` (only `ledgers.closed_at`)            | `DateTime64(3, 'UTC')`                                                      |
| `TIMESTAMPTZ created_at` (every other table)        | **OMITTED** (column dropped on the CH side; PG keeps it)                    |
| `tsvector` (only `soroban_contracts.search_vector`) | OMITTED                                                                     |
| `PARTITION BY RANGE (created_at)`                   | `PARTITION BY intDiv(ledger_sequence, 500000)` (~29 days)                   |
| Postgres `PRIMARY KEY (…)`                          | `ORDER BY (…)` with `ledger_sequence` substituted for `created_at`          |
| FK constraints                                      | OMIT (not enforceable in CH)                                                |
| CHECK constraints                                   | OMIT for the pilot                                                          |
| GIN / `pg_trgm` indexes                             | OMIT (no equivalent)                                                        |
| Partial unique indexes                              | OMIT (no enforcement); composite ORDER BY uses `allow_nullable_key` instead |

### Two CH-side coercions for `ReplacingMergeTree` version columns

ClickHouse rejects `Nullable` columns as the version slot of
`ReplacingMergeTree(version_column)`. The natural version columns on
`soroban_contracts.wasm_uploaded_at_ledger` and `nfts.current_owner_ledger`
are Nullable in Postgres, so the CH copy declares them as
`Int64 DEFAULT 0`. The semantic shift ("0 means unknown" vs "NULL means
unknown") only matters on the CH side and is reflected in `init.sql`
comments.

## How the schema applies

`schema/init.sql` is the single source of truth — every statement is
`CREATE … IF NOT EXISTS`, so applying twice is a no-op. Two paths apply
it; both `include_str!` the same file:

- **`db-clickhouse-init` sidecar** (default in `docker-compose.yml`):
  runs `clickhouse-client --queries-file` against the `clickhouse`
  service after it reports healthy. Used by `docker compose up`.
- **Rust CLI** (`cargo run -p db-clickhouse --bin db-clickhouse-init`):
  same SQL via the official `clickhouse` Rust crate over HTTP. Useful
  when iterating on `init.sql` without restarting compose, or when
  ClickHouse runs outside Docker.

### Environment variables

| Env var                  | Default                 | Used by                   |
| ------------------------ | ----------------------- | ------------------------- |
| `CLICKHOUSE_HTTP_PORT`   | `8123`                  | docker compose            |
| `CLICKHOUSE_NATIVE_PORT` | `9000`                  | docker compose            |
| `CLICKHOUSE_URL`         | `http://localhost:8123` | Rust CLI + smoke test     |
| `CLICKHOUSE_USER`        | `default`               | Rust CLI + smoke test     |
| `CLICKHOUSE_PASSWORD`    | `clickhouse`            | Rust CLI + smoke test     |
| `CLICKHOUSE_DATABASE`    | `default`               | Rust CLI + smoke test     |

Defaults live in `.env.example` at the repo root and are exercised by the
Quick start above.

## Smoke test (what it covers)

`cargo test -p db-clickhouse --test smoke` exercises the full schema:

- Applies `init.sql` to the target ClickHouse (idempotent)
- Inserts one sentinel row into each of the 17 tables
- Reads the row back via `SELECT count() … WHERE …`
- Reloads `transaction_hash_dict` and verifies
  `dictGet('transaction_hash_dict', 'ledger_sequence', tuple(...))`
  returns the expected `ledger_sequence`
- Cleans up via `ALTER TABLE … DELETE` (sync mutation) before and after,
  so a previous failed run does not poison the next attempt

The test is **gated on `CLICKHOUSE_URL`**: if the env var is not set, it
exits cleanly without failing. CI without a ClickHouse instance is green
by default. Run command lives in Quick start §3.

## What this crate intentionally does NOT do

Per the non-invasive contract in
[ADR 0044 §Decision §7](../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md):

- Indexer does not write to ClickHouse — separate ADR + task
- API does not read from ClickHouse — separate ADR + task
- No backfill from Postgres — separate task
- No Postgres retirement — explicitly deferred to the migrate-or-retire
  decision after pilot measurements

If a PR under task 0204 modifies any file outside this crate, the workspace
`Cargo.toml`, `docker-compose.yml`, `docs/architecture/**`, or `lore/`,
**reject it**. The pilot's contract is that everything else stays exactly
as it is.
