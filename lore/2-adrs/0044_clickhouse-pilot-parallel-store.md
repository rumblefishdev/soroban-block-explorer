---
id: '0044'
title: 'ClickHouse pilot — parallel store mirroring Postgres schema, with full-content soroban_events'
status: accepted
deciders: [fmazur]
related_tasks: ['0204', '0205', '0206']
related_adrs: ['0033', '0047']
tags:
  [
    architecture,
    clickhouse,
    schema,
    pilot,
    db-evaluation,
    non-invasive,
    soroban-events,
    olap-vs-oltp,
  ]
links: []
history:
  - date: '2026-05-20'
    status: accepted
    who: stkrolikiewicz
    note: >
      Ratified post-pivot per ADR 0047. CH on Hetzner promoted from
      "parallel pilot" to primary API datastore. Status flipped
      proposed → accepted; the pilot evaluation phase is complete and
      the architectural direction is committed. Implementation paths
      tracked in tasks 0228 (parallel-backfill merge), 0241 (indexer
      hard swap), 0243 (API rewrite), 0239 (AWS-side cutover including
      RDS decommission in Phase 6).
  - date: '2026-05-08'
    status: proposed
    who: fmazur
    note: >
      Drafted after team alignment (fmazur, skrolikiewicz, fdziubek,
      kkowalczyk). Stand up a parallel ClickHouse store mirroring the current
      Postgres schema (with `soroban_events_appearances` replaced by a
      full-content `soroban_events` table) to evaluate analytical throughput
      and storage trade-offs without touching the existing Postgres path.
      Implementation lands in spawned task 0204; this ADR scopes the new DB
      only — no indexer dual-write, no API read-path changes.
  - date: '2026-05-08'
    status: proposed
    who: fmazur
    note: >
      ADR review walked through all seven open questions; six resolved
      and folded into §Decision §4–§5. Resolutions: Q1 mixed engine per
      "category" (Replacing for fact + state, plain for immutable lookup);
      Q2 PARTITION BY intDiv(ledger_sequence, 500000) + drop `created_at`
      from every CH table except `ledgers` (Postgres unchanged);
      Q3 nfts.metadata dropped (CH only), wasm_interface_metadata.metadata
      as String; Q4 official `clickhouse` crate latest stable;
      Q5 idempotent init.sql for the pilot; Q7 keep ledgers/accounts/
      wasm_interface_metadata, drop _sqlx_migrations, keep
      transaction_hash_index served via CH `Dictionary` (cache layout,
      RAM-bounded). Q6 (success criteria) remains open — deferred to
      follow-up ADR after first measurements. New Q8 (bucket size)
      surfaced and resolved in the same session: locked at 500 000
      ledgers (~29 days). CH net schema: 17 tables + 1 Dictionary. ER
      diagram archived in task 0204 `notes/G-clickhouse-schema-er.md`.
  - date: '2026-05-12'
    status: proposed
    who: fmazur
    note: >
      Task 0206 landed the real CH writer: `db_clickhouse::persist`
      module (rows.rs + stage.rs + writer.rs + ids.rs), partition-
      aligned streaming inserts via `PartitionWriter` (one
      `clickhouse::Insert<RowT>` per table per backfill partition),
      deterministic surrogate-ID derivation
      (cityhash-rs::cityhash_102_128, lower 64/32 bits — documented
      divergence from CH SQL `cityHash64()`), and a
      `backfill-runner::Sink` partition-writer lifecycle
      (open_partition / write_ledger / commit / abort) — PG variant
      maps to no-op open/commit so behaviour is unchanged on that
      path. Schema (init.sql) untouched. Closes the §"Future Work"
      first bullet ("populating the store"). Open Q6 (success
      criteria) remains deferred — measurement task spawns once a
      partition run lands.
  - date: '2026-05-12'
    status: proposed
    who: fmazur
    note: >
      Codec amendment: `soroban_events.topics_xdr` and
      `soroban_events.data_xdr` switched from CH-default LZ4 to
      `CODEC(ZSTD(3))`. Empirical justification on the first 100
      mainnet ledgers post-task-0206: `topics_xdr` carries
      ScVal-decoded JSON with the repeated `{"type":...,"value":...}`
      wrapper on every topic — LZ4's 64-KiB window cannot exploit
      that long-range dictionary pattern (measured ratio 6.29×).
      ZSTD(3) reaches ~20–40× on the same shape. `data_xdr` is
      lighter on redundancy (LZ4 already at 11.12×) but shares the
      codec for symmetry — marginal positive gain, zero downside.
      Decompression cost matches LZ4 on the read path; insert CPU is
      a one-time append-only write. Documented in
      `docs/architecture/database-schema/clickhouse-pilot.md` §4a +
      `crates/db-clickhouse/README.md` type-translation note. This
      slightly relaxes task 0206's "schema untouched" gate — codec is
      a column-level attribute change, no table shape / column count
      / type translation impact.
  - date: '2026-05-12'
    status: proposed
    who: fmazur
    note: >
      Writer hardening — bumped `http_receive_timeout` /
      `http_send_timeout` from CH default 30s to 1800s (30 min) via
      `Insert::with_setting` in
      `db_clickhouse::persist::writer::apply_bulk_ingest_settings`.
      Empirical trigger: 10 k-ledger smoke at 62016000 failed at
      ~9.6 k with `Network("channel closed")`; root cause was server
      `http_receive_timeout = 30s` closing the socket between sparse
      chunked-HTTP body writes on tables like `nfts` (~15
      rows/ledger ⇒ ~5 min between 256 KiB-buffer flushes),
      `wasm_interface_metadata` (often 0 rows over many ledgers),
      `lp_positions`. Per-stalled-chunk timeout, not per-request,
      so steady-state never approaches it. Client-side `with_setting`
      keeps the change scoped to the writer's INSERTs.
  - date: '2026-05-12'
    status: proposed
    who: fmazur
    note: >
      Follow-up to the previous entry: per-query `with_setting` was
      verified via `system.query_log.Settings` to reach CH (recorded
      as 1800) but a second 10 k-ledger smoke failed identically at
      ~9.6 k with the same SOCKET_TIMEOUT 30000 ms. CH 26.3 does not
      propagate the per-query override to the Poco-level socket
      read on the HTTP body chunked stream — only profile-level XML
      config takes effect. Added
      `crates/db-clickhouse/config.d/timeouts.xml` (mounted into the
      `clickhouse` service in `docker-compose.yml` at
      `/etc/clickhouse-server/config.d/`) setting
      `keep_alive_timeout / http_receive_timeout / http_send_timeout
      / receive_timeout / send_timeout` to 1800 s on the `default`
      profile. Per-query `with_setting` calls in the writer retained
      as belt-and-suspenders (cost zero; future CH versions may
      propagate). Pilot-only — production deployment needs a
      narrower posture + monitoring. Documented in
      `crates/db-clickhouse/README.md` and the XML header.
  - date: '2026-05-12'
    status: proposed
    who: fmazur
    note: >
      **Production-grade schema refactor (folds in tasks 0206 + 0208).**
      Six structural changes:
      (1) Drop all surrogate `Int64`/`Int32` `id` columns (7 tables:
      accounts, transactions, soroban_contracts, assets, nfts,
      operations_appearances, liquidity_pool_snapshots). Natural-key
      `ORDER BY` everywhere. Removes the entire client-side
      `cityhash` derivation layer (`crates/db-clickhouse/src/persist/ids.rs`
      moved to .trash/), eliminates Int32 collision risk on assets/nfts,
      eliminates the "our hash ≠ CH cityHash64()" footgun.
      (2) FK columns reference natural keys directly
      (`transactions.source_account: LowCardinality(String)` instead of
      `source_id Int64`, etc.). `LowCardinality(String)` dictionary-
      encodes the per-block-repeated StrKeys for free.
      (3) `liquidity_pools` Path 2 from task 0208 — drop
      `created_at_ledger`, add `last_updated_ledger`, engine becomes
      `ReplacingMergeTree(last_updated_ledger)`. Read-time recovery of
      create-ledger via `MIN(ledger_sequence) FROM
      liquidity_pool_snapshots GROUP BY pool_id`. Task 0208
      superseded.
      (4) `operations_appearances` ORDER BY (ledger_sequence,
      transaction_hash, application_order); `nft_ownership` ORDER BY
      (contract_id, token_id, ledger_sequence, event_order);
      `liquidity_pool_snapshots` ORDER BY (pool_id, ledger_sequence).
      All surrogate-id-free.
      (5) Account-balance trustline removals emit `balance = 0` rows
      at current ledger; reads filter `WHERE balance > 0` to recover
      "active trustlines" semantics. No `CollapsingMergeTree` needed.
      (6) Composite-PK columns use empty-string sentinel for "no value"
      (`assets.asset_code = ''`, `account_balances_current.issuer_account = ''`
      for native, etc.) instead of `Nullable(String)` — CH `ORDER BY`
      on plain `String` significantly faster than on `Nullable(String)`,
      no `allow_nullable_key` setting needed. `ZSTD(3)` codec also
      applied to `wasm_interface_metadata.metadata` (analogous to
      `soroban_events.topics_xdr/data_xdr`); `LowCardinality` applied
      to event signatures, asset codes, contract StrKey FK columns.
      All implemented inline in task 0206's PR (task 0208 closed
      as folded). Schema-overview matrix in
      `docs/architecture/database-schema/clickhouse-pilot.md` §3
      updated; type-translation table in
      `crates/db-clickhouse/README.md` updated; coverage-mapping
      doc in `lore/.../0206_…/notes/G-coverage-mapping.md` updated.
  - date: '2026-05-12'
    status: proposed
    who: fmazur
    note: >
      **Hybrid surrogate / natural keys (partial revert of previous
      entry).** Empirical 10k-ledger smoke at 62016000 measured ~500
      MB extra on-disk vs a hash-i64 baseline. Projected ~550 GB at
      11M-ledger full scale. Plus +10 ms write/ledger from
      LowCardinality dictionary build on high-cardinality columns.
      Reverted surrogate `id Int64` on the **three central FK hubs**
      where the natural keys are 56-byte StrKeys and join frequency
      is very high: `accounts`, `soroban_contracts`, `transactions`.
      All FK columns to those tables become `Int64` again
      (`source_id`, `destination_id`, `contract_id`, `caller_id`,
      `transaction_id`, etc.). Other tables (`assets`, `nfts`,
      `liquidity_pools`, `lp_positions`,
      `liquidity_pool_snapshots`, `operations_appearances`,
      `transaction_participants`, `nft_ownership`) keep their natural
      / composite primary keys — for them composite (StrKey-or-hash, …)
      ORDER BYs work cheaply. ORDER BY on the three hub tables uses
      the natural key (`account_id`, `contract_id`,
      `(ledger_sequence, application_order)`) — surrogate `id` is for
      FK joins only, not granule pruning. cityhash-rs Cargo dep
      reinstated; `crates/db-clickhouse/src/persist/ids.rs`
      recreated with exactly three helpers (`account_id`,
      `contract_id`, `transaction_id`); writer / row structs /
      tests updated accordingly. Honest framing of the trade in
      `crates/db-clickhouse/README.md` §"Surrogate-id hubs".
---

# ADR 0044: ClickHouse pilot — parallel store mirroring Postgres schema, with full-content soroban_events

**Related:**

- [ADR 0033: soroban_events → soroban_events_appearances (read-time event detail from S3)](0033_soroban-events-appearances-read-time-detail.md) — the folded design this pilot deliberately reverses for the ClickHouse copy of the table
- [Task 0204: db-clickhouse crate + Docker service + mirrored schema](../1-tasks/active/0204_FEATURE_clickhouse-pilot-crate-docker-schema/README.md) — implementation of this ADR's decision

---

## Context

Today the explorer runs entirely on PostgreSQL 16 (one production instance,
plus the `db-merge` and `db-snapshot-source` profiles for the offline backfill
pipeline). Postgres is well-suited to the OLTP shape of the indexer
(per-ledger writes, point lookups by hash, keyset-paginated lists), and the
schema is intentionally narrow — most heavy event/transaction detail is
fetched from the public S3 archive at read time (ADR 0029, ADR 0033).

Two pressures are pushing the team to evaluate a columnar OLAP store next to
Postgres:

1. **Event analytics.** `/contracts/{id}/events` and any future
   "what happened on chain in window X" question is fundamentally a wide scan
   across `created_at`-partitioned data with high compression potential
   (signatures, topic hashes, amounts repeat heavily). The folded
   `soroban_events_appearances` design (ADR 0033) hides the heavy event
   payload behind an S3 round-trip — necessary in Postgres because storing
   `topics_xdr` + `data_xdr` per event blew up the heap, but penalising on
   every page render. A columnar store with native LZ4/ZSTD column codecs
   could plausibly hold the full XDR cheaply enough to drop the S3 hop.

2. **Read concurrency on hot ranges.** Dashboards (Grafana,
   future internal analytics tooling) want to slice the same tables by
   different dimensions concurrently. Postgres serves these from the OLTP
   index set, competing with indexer write traffic on shared buffers. A
   read-replica of the same data in a columnar engine isolates that
   workload entirely.

The team agreed to evaluate **ClickHouse** specifically (vs. DuckDB,
TimescaleDB, Citus columnar) because (a) it's purpose-built for partitioned
append-only event/log workloads, (b) it has first-class composite primary
key + ORDER BY semantics that map cleanly onto our `(entity_id, created_at,
…)` access patterns, and (c) it deploys as a single container alongside the
existing Postgres compose service.

This ADR scopes **only** the parallel-store decision — what the new DB looks
like, where it lives, what's in scope for the pilot. Indexer dual-write,
API A/B, and any decision to migrate or retire Postgres are explicitly
out of scope and deferred to follow-up ADRs once the pilot has data to
show.

---

## Decision

1. **Add ClickHouse as a parallel data store, not a replacement.** The
   existing Postgres path (indexer writes, API reads, db-merge backfill,
   db-partition-mgmt lambda) is unchanged by this ADR. ClickHouse stands
   next to it.

2. **New crate: `crates/db-clickhouse`.** Sibling to `db`, `db-merge`,
   `db-migrate`, `db-partition-mgmt`. Owns the schema definitions, the
   migration runner (or its ClickHouse equivalent), and the connection
   layer. It does **not** wire into `indexer`, `api`, or any other crate
   in this ADR — it ships as a self-contained library + CLI for now.

3. **New compose service: `clickhouse`** in `docker-compose.yml`. Sits
   alongside `postgres`, exposes ClickHouse's HTTP (8123) and native (9000)
   ports for local development. Volume-backed, healthcheck-gated, no
   profile (starts on `docker compose up` so devs can run both DBs at
   once). Same credentials posture as Postgres (default user/password for
   local dev only).

4. **Schema parity with Postgres, with five deliberate divergences.**
   Every table in the current Postgres `public` schema (per
   `db-schema-snapshot.md` from 2026-05-08, archived in task 0204
   `sources/`) gets a ClickHouse counterpart with the same name and
   logical column set — **except** for the five divergences below.
   **Postgres itself is unchanged by every divergence.** All five are
   CH-side schema choices; the existing PG schema, indexer write path,
   API read path, and migration ladder stay exactly as they are today.

   **4a. `soroban_events_appearances` → full-content `soroban_events`.**
   The folded appearance design from ADR 0033 stays in PG, but the CH
   copy of this table holds full event content per row. Canonical
   logical shape (expressed in Postgres DDL for reference — CH
   translation follows §5):

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

   This is the "v3 spike" design: per-event row, full XDR inlined,
   ordered for keyset pagination. The rationale is that a columnar store
   eliminates the row-width penalty that made this shape expensive in
   Postgres. ADR 0033's appearance-only folding is a Postgres-shaped
   compromise; in the columnar copy of the table, we want the full
   content.

   **4b. `created_at` dropped from every CH table except `ledgers`.**
   ClickHouse partitions by `ledger_sequence`, not by time (see §5
   partitioning). The wall-clock `created_at` column on `transactions`,
   `operations_appearances`, `transaction_participants`, `nft_ownership`,
   `liquidity_pool_snapshots`, `soroban_events`,
   `soroban_invocations_appearances`, and `transaction_hash_index` is
   **omitted from the CH schema only**. PG keeps `created_at` on every
   one of these tables unchanged. In CH, wall-clock time is recovered
   via JOIN to `ledgers.closed_at` (or via a Dictionary if hot). This
   eliminates ~50–100 GB of redundant denormalization at full Stellar
   scale and removes drift risk between fact-table copies of the
   timestamp.

   **4c. `nfts.metadata` dropped (CH only).** Team decision: the JSONB
   metadata blob is not carried in the CH copy of `nfts`. PG-side
   `nfts.metadata` stays unchanged and continues to be populated by the
   indexer. CH `nfts` simply lacks this column.

   **4d. `_sqlx_migrations` dropped.** Pilot uses an idempotent
   `init.sql` (see §5 schema apply), not a numbered migration ladder.
   The PG-side `_sqlx_migrations` table has no role in CH and is not
   mirrored. PG continues to use `sqlx` migrations as today.

   **4e. `transaction_hash_index` served as a CH `Dictionary`.** The
   table itself is mirrored 1:1 (`hash`, `ledger_sequence`), but query
   access goes through a `CREATE DICTIONARY` (complex_key_cache layout,
   bounded RAM) rather than a btree-style index. PG keeps the table
   exactly as today — used for partition-pruning. CH uses the table as
   the Dictionary's source and serves point lookups from RAM.

   **Cosmetic / non-translatable PG features.** `soroban_contracts.search_vector`
   (tsvector) is omitted in CH — no equivalent. `pg_trgm` GIN indexes
   have no analogue. Partial unique indexes and CHECK constraints don't
   translate. FK constraints are not enforceable in CH. PG keeps all of
   these.

5. **Resolved translation rules.** Decisions made during ADR review on
   2026-05-08; six of seven open questions resolved. Q6 (pilot success
   criteria) remains open and belongs to a follow-up ADR after first
   measurements. The rules below are the canonical reference — task
   0204 implements them in `crates/db-clickhouse/`.

   **Engine per table "category" (resolves Q1):**

   - **Append-only fact tables** (`transactions`, `operations_appearances`,
     `transaction_participants`, `nft_ownership`, `liquidity_pool_snapshots`,
     `soroban_events`, `soroban_invocations_appearances`,
     `transaction_hash_index`) → `ReplacingMergeTree`. Replay safety on
     re-ingest comes from background dedup-by-`ORDER BY`-key.
   - **State tables** (`accounts`, `assets`, `account_balances_current`,
     `nfts`, `lp_positions`, `soroban_contracts`) → `ReplacingMergeTree`
     with a version column (e.g. `last_updated_ledger`,
     `current_owner_ledger`, `wasm_uploaded_at_ledger`). Latest version
     per `ORDER BY` key wins after merge.
   - **Immutable lookup** (`ledgers`, `liquidity_pools`,
     `wasm_interface_metadata`) → plain `MergeTree`. Written once,
     never updated.

   **Partitioning (resolves Q2 and Q8):** every partitioned table
   uses `PARTITION BY intDiv(ledger_sequence, 500000)`. 500 000
   ledgers ≈ 29 days of Stellar protocol time (one ledger ≈ 5 s) —
   mirrors the PG monthly partition mental model and gives a
   reasonable parts-per-table count over 10+ years of history. The
   bucket size is locked at 500 000 for the pilot; revisit only if
   measurements reveal merge backlog or retention granularity issues
   (single-constant change in `init.sql` + volume nuke). `ORDER BY`
   of each fact table uses `ledger_sequence` in place of the dropped
   `created_at`.

   **Type translation:**

   | Postgres                                 | ClickHouse                                                                                              |
   | ---------------------------------------- | ------------------------------------------------------------------------------------------------------- |
   | `BIGINT` / `bigserial`                   | `Int64`                                                                                                 |
   | `INTEGER` / `serial`                     | `Int32`                                                                                                 |
   | `SMALLINT`                               | `Int16`                                                                                                 |
   | `BOOLEAN`                                | `Bool`                                                                                                  |
   | `VARCHAR(N)` / `TEXT` / variable `BYTEA` | `String`                                                                                                |
   | 32-byte `BYTEA` (hashes, `pool_id`)      | `FixedString(32)`                                                                                       |
   | `NUMERIC(28,7)`                          | `Decimal128(7)`                                                                                         |
   | `JSONB`                                  | `String` (resolves Q3 — only on `wasm_interface_metadata.metadata`; `nfts.metadata` is dropped per §4c) |
   | `TIMESTAMPTZ` (only `ledgers.closed_at`) | `DateTime64(3, 'UTC')`                                                                                  |
   | `tsvector`                               | omitted (no equivalent)                                                                                 |
   | PG `PRIMARY KEY (a, b, c)`               | `ORDER BY (a, b, c)` (CH analogue)                                                                      |
   | FK constraints                           | omitted (not enforceable in CH)                                                                         |
   | CHECK constraints                        | omitted for the pilot; revisit per-table if hot                                                         |
   | GIN / `pg_trgm` indexes                  | omitted (no equivalent)                                                                                 |
   | Partial unique indexes                   | omitted (no enforcement)                                                                                |

   **Schema apply (resolves Q5):** single idempotent
   `crates/db-clickhouse/schema/init.sql` with `CREATE TABLE IF NOT
EXISTS …` + the `CREATE DICTIONARY transaction_hash_dict`. Applied
   by `db-clickhouse-init` CLI; second run is a no-op. Numbered
   migration ladder mirroring `crates/db` is deferred until the
   dual-write follow-up task lands.

   **Client (resolves Q4):** official `clickhouse` crate from
   crates.io (maintained by ClickHouse Inc.), latest stable at task
   0204 implementation time, version pinned in `Cargo.toml` and
   documented in `crates/db-clickhouse/README.md`.

   **Dropped tables / columns vs PG snapshot (CH side only — PG
   unchanged):**

   - `_sqlx_migrations` table (replaced by `init.sql`)
   - `nfts.metadata` column
   - `created_at` column on 8 fact/index tables (`ledgers.closed_at`
     is the single source of truth)
   - `soroban_contracts.search_vector` column (no `tsvector` in CH)

   **CH net schema:** 17 tables (PG had 18; `_sqlx_migrations` dropped)
   plus 1 `Dictionary` (`transaction_hash_dict` sourced from
   `transaction_hash_index`).

6. **Pilot is read-empty for now.** No indexer writes to ClickHouse. No
   API reads from ClickHouse. The crate ships with the schema and a
   connection layer; populating it (dual-write from the indexer or batch
   ETL from Postgres) is out of scope here and will be its own ADR once
   we know the schema works locally.

7. **Non-invasive contract.** No file under `crates/{api,indexer,domain,
db,db-merge,db-migrate,db-partition-mgmt,xdr-parser,backfill-runner,
audit-harness,backfill-bench}` may be modified by the pilot landing
   PR. The only allowed changes outside `crates/db-clickhouse/` are:
   `Cargo.toml` workspace members, `docker-compose.yml` (new service),
   `docs/architecture/**` (per ADR 0032), and lore (this ADR + task
   0204). Any deviation from this is grounds to reject the PR.

---

## Rationale

### Pilot before commitment

The team has not committed to migrating off Postgres, only to evaluating
ClickHouse. Building the parallel store first — with no behavioural
coupling to the live system — lets us measure storage and query
performance on real data before any reversible decision becomes hard to
reverse. If the pilot shows ClickHouse is the wrong fit, we delete the
crate and the compose service; nothing else has changed.

### Same schema, except where it pays to differ

Mirroring the Postgres logical shape minimises mental overhead during
the evaluation: the same JOINs, the same composite keys, the same
`(transaction_id, ledger_sequence)` lineage to `transactions` (CH side
— PG keeps the additional `created_at` lineage column unchanged). The
five deliberate divergences listed in §Decision §4 each have a
specific reason: full-content `soroban_events` is the table where
columnar compression and per-row event detail change the trade-off
most sharply (and lets the pilot answer "would the v3 design have made
sense in a columnar engine?"); `created_at` dropped CH-side because
CH partitions by `ledger_sequence` and the wall-clock is functionally
derivable from `ledgers.closed_at` (eliminates ~50–100 GB of redundant
denormalization at scale); `nfts.metadata` dropped per team decision;
`_sqlx_migrations` replaced by `init.sql` because the pilot is
read-empty and iterates rapidly; `transaction_hash_index` served via
`Dictionary` because Postgres' partition-pruning workaround doesn't
translate to CH and a RAM-bounded cache is the idiomatic CH analogue.

### `crates/db-clickhouse` parallel to `crates/db`

Adding a sibling crate (rather than feature-flagging `crates/db`) keeps
the boundary explicit and the existing `db` crate untouched. Cargo
workspace already hosts five `db-*` crates with different responsibilities;
the precedent fits.

### One Docker service, no profile

ClickHouse runs as a single container with no upstream dependency; gating
it behind a compose profile (like `db-merge`) would only add friction for
developers running both DBs side by side. Starting it by default mirrors
the current `postgres` service.

---

## Alternatives Considered

### Alt 1: Skip the parallel store, add columnar features inside Postgres

**Description:** Use Citus columnar storage, Hydra, or a Timescale
hypertable for the heavy tables (events, operations). Stay
single-engine.

**Pros:** No second store to operate. Same SQL dialect, same FKs.

**Cons:** Each option carries operational and licensing baggage that's
larger than evaluating ClickHouse cleanly. Citus columnar is read-only
at write time and requires retrofitting the indexer. Hydra is a fork of
Postgres with its own upgrade path. None of them give the column-codec
density ClickHouse delivers out of the box. The whole point of the
pilot is to find out whether ClickHouse's primitives change the
event-store math; doing the evaluation inside Postgres begs the
question.

**Decision:** REJECTED — defeats the purpose of evaluating a different
engine.

### Alt 2: Start by replacing Postgres outright

**Description:** Skip the pilot. Cut the indexer over to ClickHouse,
delete `crates/db`.

**Pros:** No dual maintenance, no divergence risk.

**Cons:** ClickHouse has no FK constraints, no MERGE/UPDATE on
`MergeTree` (only `ALTER TABLE … UPDATE` async mutations), no real
serial sequences. The indexer's current write path leans on all of
those. Migrating without measurement first risks discovering a
fundamental mismatch after the rewrite. The existing Postgres path is
working — the cost of running both for a measurement window is low; the
cost of a failed cutover is high.

**Decision:** REJECTED — too much commitment too early.

### Alt 3: Mirror schema exactly, including `soroban_events_appearances`

**Description:** ClickHouse copy is byte-for-byte the Postgres logical
shape, including the folded appearances table.

**Pros:** Maximally apples-to-apples comparison.

**Cons:** Wastes the pilot. The whole reason to put events in a
columnar engine is that the per-event row stops being expensive — the
appearance-folding compromise that made sense in Postgres no longer
applies. Mirroring the compromise into ClickHouse would prove only that
ClickHouse can run the same compromise.

**Decision:** REJECTED — pilot exists to measure the version where the
compromise is removed.

### Alt 4: Compose profile gating

**Description:** Put `clickhouse` behind a `--profile clickhouse` so it
only starts when explicitly requested.

**Pros:** Slightly cheaper local boot for devs who don't care about the
pilot.

**Cons:** Most devs _do_ care during the pilot window. Profile gating
adds friction (`COMPOSE_PROFILES=clickhouse docker compose up`)
without proportional benefit. Resource cost of an idle ClickHouse
container is small.

**Decision:** REJECTED — friction outweighs the saved memory.

---

## Consequences

### Positive

- **Measurement before commitment.** The pilot produces real numbers on
  real data before the team decides anything irreversible.
- **Full-content `soroban_events` becomes available** for analytical
  queries that the Postgres folded design forecloses — `WHERE signature =
'…'`, topic filters, full-history scans for one contract.
- **Columnar workload off-loads from Postgres.** Once dual-write lands
  (out of scope here), heavy analytical queries can run against
  ClickHouse without competing with indexer writes for shared buffers.
- **Self-contained pilot infrastructure.** `crates/db-clickhouse` +
  compose service can be deleted in one PR if the evaluation fails.
- **Documents the schema-translation rules** — task 0204's translation
  table becomes the reference for any future ClickHouse work, regardless
  of the pilot's outcome.

### Negative

- **Schema duplication during the pilot.** Every change to Postgres
  schema (new column, new index, new partition) needs a parallel change
  in `crates/db-clickhouse` to keep parity meaningful. Schema drift is
  the largest ongoing cost of this ADR. Mitigated by the pilot's bounded
  scope: the parallel store is read-empty until a follow-up ADR turns
  on dual-write, so schema drift only matters once that happens.
- **No FK / CHECK enforcement on the ClickHouse side.** The constraints
  encoded in the Postgres DDL (`ck_assets_identity`, `ck_se_v3_type_range`,
  composite FKs to `transactions(id, created_at)`) cannot be enforced by
  ClickHouse. Any future writer (indexer dual-write, ETL job) is on the
  hook for upholding them.
- **`tsvector` and `pg_trgm` features have no ClickHouse counterpart.**
  The full-text search on `soroban_contracts.name` and the trigram
  indexes on `assets.asset_code`, `nfts.collection_name`, `nfts.name`
  do not translate. ClickHouse's `tokenbf_v1` skip index and `String`
  `LIKE` are different beasts. Fuzzy-search endpoints stay
  Postgres-only for now.
- **Operational surface area grows.** Two engines to monitor, two
  backups, two upgrade cadences. Acceptable for a pilot; revisited at
  the migrate-or-retire decision point.
- **Pilot may fail.** Possible outcome is "ClickHouse doesn't
  outperform Postgres enough on this workload to justify the operational
  cost." That outcome is itself a valid result and the cost of finding
  it out is bounded.

---

## Open Questions

Six of the original seven open questions were resolved during ADR
review on 2026-05-08 and moved into §Decision §4–§5. The new Q8 surfaced
during the same review and was also closed in the same session. Only
Q6 (pilot success criteria) remains genuinely open — deferred to a
follow-up ADR after first measurements.

1. ~~**`MergeTree` engine choice per table.**~~ **Resolved 2026-05-08:**
   mixed engine per "category" — append-only fact tables and state tables
   use `ReplacingMergeTree` (with version column on state tables);
   immutable lookup tables use plain `MergeTree`. See §Decision §5.
2. ~~**Partitioning granularity.**~~ **Resolved 2026-05-08:**
   `PARTITION BY intDiv(ledger_sequence, 500000)` for every
   partitioned table; `created_at` dropped from CH-side fact tables in
   favour of JOIN to `ledgers.closed_at`. See §Decision §4b, §5.
3. ~~**`JSONB` columns.**~~ **Resolved 2026-05-08:** `nfts.metadata`
   dropped from CH (PG unchanged); `wasm_interface_metadata.metadata`
   is `String` in CH. See §Decision §4c, §5.
4. ~~**ClickHouse client crate.**~~ **Resolved 2026-05-08:** official
   `clickhouse` crate from crates.io (maintained by ClickHouse Inc.),
   latest stable at task 0204 implementation time. See §Decision §5.
5. ~~**Migrations.**~~ **Resolved 2026-05-08:** single idempotent
   `init.sql` for the pilot; numbered ladder deferred to dual-write
   follow-up. See §Decision §5.
6. **What kills the pilot.** Explicit PASS/FAIL success criteria for
   the pilot — defining "ClickHouse outperforms Postgres enough to
   justify migration" vs "we abandon the experiment". Candidate
   dimensions: storage ratio (full-content `soroban_events` in CH vs
   `soroban_events_appearances` + S3 in PG), query latency on
   representative `/events`-style scans, ingest throughput. Concrete
   thresholds belong in a follow-up ADR, not 0204 — pilot itself is
   read-empty and produces no measurements; thresholds set without
   data are theatre. The follow-up ADR is gated on a dual-write or
   backfill task that gives CH real data to measure against.
7. ~~**What to do about classic Stellar tables that don't benefit
   much from columnar.**~~ **Resolved 2026-05-08:** keep `ledgers`
   (load-bearing for time resolution after Q2), `accounts` (dimension
   for account_id ↔ G-address self-contained queries), and
   `wasm_interface_metadata` (small, low-cost, useful for display);
   drop `_sqlx_migrations` (Q5); keep `transaction_hash_index` but
   serve it as a CH `Dictionary`. See §Decision §4d, §4e, §5.
8. ~~**`intDiv(ledger_sequence, 500000)` bucket size.**~~ **Resolved
   2026-05-08:** locked at **500 000 ledgers ≈ 29 days** at Stellar's
   ~5 s ledger time. Mirrors the PG monthly partition mental model and
   gives a reasonable parts-per-table count over 10+ years of history.
   Tuning down (100 000, ~6 days) or up (2 000 000, ~4 months) is a
   single-constant change in `init.sql` + volume nuke; revisit only if
   measurements reveal merge backlog or retention granularity issues
   on this specific bucket size. See §Decision §5 (partitioning).

---

## Delivery Checklist

Per [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md), any
ADR that changes the shape of the system MUST be landed together with the
corresponding updates to `docs/architecture/**`. This ADR defines pilot
_intent_ — the implementation lands in task 0204, which carries the doc
updates. Boxes below are scoped to the pilot-intent landing PR only.

- [ ] `docs/architecture/technical-design-general-overview.md` — N/A on this
      PR — task 0204 adds a "ClickHouse pilot" subsection when the schema
      and compose service land.
- [ ] `docs/architecture/database-schema/database-schema-overview.md` —
      N/A on this PR — task 0204 adds the parallel-schema section and
      the translation table when it produces them.
- [ ] `docs/architecture/backend/backend-overview.md` — N/A — pilot does
      not touch `crates/api`.
- [ ] `docs/architecture/frontend/frontend-overview.md` — N/A — frontend
      is not affected.
- [ ] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` —
      N/A — pilot does not change ingestion. A follow-up ADR will revisit
      when dual-write is on the table.
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md` —
      N/A on this PR — task 0204 adds the ClickHouse compose service to
      the local-dev infrastructure section when it lands.
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` — N/A —
      XDR parsing is not changed.
- [ ] This ADR is linked from each updated doc at the relevant section —
      N/A on this PR — done together with the updates above when task
      0204 lands.

(All boxes legitimately N/A for the ADR-only landing because the system
shape does not change until task 0204's PR. The same checklist re-runs
on task 0204's PR with concrete updates.)

---

## References

- [ADR 0033](0033_soroban-events-appearances-read-time-detail.md) — the
  Postgres folded-events design this pilot deliberately reverses for the
  ClickHouse copy of the table
- [ClickHouse documentation — MergeTree engine family](https://clickhouse.com/docs/en/engines/table-engines/mergetree-family)
- [ClickHouse documentation — Partitioning](https://clickhouse.com/docs/en/engines/table-engines/mergetree-family/custom-partitioning-key)
- Schema snapshot taken on 2026-05-08
  ([`lore/1-tasks/backlog/0204_FEATURE_clickhouse-pilot-crate-docker-schema/sources/db-schema-snapshot.md`](../1-tasks/backlog/0204_FEATURE_clickhouse-pilot-crate-docker-schema/sources/db-schema-snapshot.md))
  — canonical reference for the Postgres logical shape task 0204 mirrors
