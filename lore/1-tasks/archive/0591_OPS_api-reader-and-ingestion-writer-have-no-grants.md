---
id: '0591'
title: 'OPS: api_reader and ingestion_writer have no <grants> — ClickHouse gives them ALL ON *.*'
type: OPS
status: completed
related_adr: ['0032']
related_tasks: ['0240', '0314', '0396', '0567', '0568', '0569']
tags: [clickhouse, security, infra-hetzner, priority-high, effort-small]
links:
  - crates/db-clickhouse/users.d/services.xml
  - docs/architecture/security/clickhouse-rbac.md
  - https://github.com/rumblefishdev/stellar-prices-api/blob/develop/lore/1-tasks/backlog/0258_CHORE_clickhouse-roles-hold-drop-and-system-on-every-database.md
history:
  - date: '2026-09-28'
    status: backlog
    who: stkrolikiewicz
    note: >
      Filed from stellar-prices-api task 0258, which measured the grants on
      ch-prod-01 on 2026-09-25 and found that both wide users are this
      repo's, not prices'. The fix lives in this repo's services.xml, so the
      task lives here; 0258 points at it.
  - date: '2026-09-28'
    status: active
    who: stkrolikiewicz
    note: Activated.
  - date: '2026-09-29'
    status: completed
    who: stkrolikiewicz
    note: >
      PR #531 merged 2026-09-28. services.xml deployed in place 2026-09-29
      06:33 UTC (inode kept, hot reload, no restart). SHOW GRANTS matches the
      file for all four users; 13/13 probes on prod as expected. Over the
      first 7 minutes: api_reader 20 queries and ingestion_writer 1,832, all
      without error; ledgers kept advancing.
---

# OPS: api_reader and ingestion_writer have no `<grants>` — ClickHouse gives them ALL ON \*.\*

## Summary

`api_reader` (Lambda API, CN `lambda-api-<env>`) and `ingestion_writer`
(Lambda Ingestion, CN `lambda-ingestion-<env>`) have no `<grants>` block in
`crates/db-clickhouse/users.d/services.xml`. A user-XML entry without one gets
every privilege on every database. The per-service matrix in
`docs/architecture/security/clickhouse-rbac.md` says `SELECT on default.*` and
`INSERT on tables Galexie does not touch`. The box does not enforce either.

## Context

`SHOW GRANTS` on `ch-prod-01`, 2026-09-25 (run by the operator through
`docker exec app-clickhouse-1 clickhouse-client` for prices-api task 0258),
identical for both users:

```
GRANT CHECK, SHOW, SELECT, INSERT, ALTER, CREATE, DROP, UNDROP TABLE,
      TRUNCATE, OPTIMIZE, BACKUP, KILL QUERY, KILL TRANSACTION,
      MOVE PARTITION BETWEEN SHARDS, SYSTEM, dictGet,
      displaySecretsInShowAndSelect, INTROSPECTION, CLUSTER, FILE, URL,
      REMOTE, MONGO, REDIS, MYSQL, POSTGRES, SQLITE, ODBC, JDBC, HDFS, S3,
      HIVE, AZURE, KAFKA, NATS, RABBITMQ, SOURCES ON *.*
```

What the profiles still stop: `api_reader` runs `read_only` (`readonly=1`,
`allow_ddl=0`), `ingestion_writer` runs `write_no_ddl` (`allow_ddl=0`), so
`DROP`, `TRUNCATE`, `CREATE` and `SYSTEM` are refused at the settings layer.

What nothing stops:

- `ingestion_writer` can `INSERT` into any table in any database, `prices.*`
  included.
- `INSERT INTO FUNCTION url()/s3()/remote()` and `SELECT FROM url()/remote()`
  reach the network from inside the box: a path out for data.
- `displaySecretsInShowAndSelect` on both.

The tenancy paragraph of the RBAC doc calls the reverse direction (BE users
reaching `prices.*`) "inside BE's trust boundary and expected". That covers
reach into `prices.*`. It does not cover the matrix claiming a scoping the box
lacks, or the external-source privileges. `galexie` and `dev_read` have no
`<grants>` block either; their grants were not measured.

## Implementation Plan

### Step 1: Derive what each user needs

- `api_reader`: every database and `system.*` table the Lambda API reads,
  from `crates/api` and
  `docs/architecture/database-schema/endpoint-queries-clickhouse/`.
- `ingestion_writer`: every table the Lambda Ingestion inserts into, and any
  `ALTER`/`OPTIMIZE` it issues.
- `SHOW GRANTS FOR galexie, dev_read, dict_reader` on the box; decide in this
  task or file a follow-up.

### Step 2: Inline `<grants>` in `services.xml`

Same form as `prices_reader` / `prices_writer` / `prices_admin` (0314, 0567,
0569). SQL `GRANT` is refused for XML-defined users
(`ACCESS_STORAGE_READONLY`), so the grant has to live in the file. A
`<grants>` block also flips the user into explicit-grant mode: anything not
listed is denied, so a missing table breaks the API or ingestion loudly.

### Step 3: Deploy in place, as 0567–0569

`users.d` sync with `--inplace` (inode kept), ClickHouse hot-reloads. Run the
Ansible play with `--check --diff` first and confirm "Restart compose stack"
is not notified.

### Step 4: Verify

- `SHOW GRANTS FOR api_reader, ingestion_writer` shows the scoped set.
- API endpoints and ingestion lag stay healthy through one full ingestion
  cycle.
- Negative probes: `ingestion_writer` `INSERT` into a `prices.*` table is
  refused; `SELECT FROM url(...)` is refused for both.

## Acceptance Criteria

- [x] `api_reader` and `ingestion_writer` carry `<grants>` limited to what
      Step 1 found; neither holds anything `ON *.*`
- [x] `URL`, `REMOTE`, `S3` and the other external-source privileges, and
      `displaySecretsInShowAndSelect`, are gone from both unless a named need
      is written here
- [x] Deployed to production in place, verified with `SHOW GRANTS` and the
      negative probes; API and ingestion healthy afterwards (2026-09-29, see
      "Deploy and verification")
- [x] `galexie`, `dev_read`, `dict_reader`: measured, and scoped here or moved
      to a follow-up task (`galexie` and `dev_read` scoped here; `dict_reader`
      is removed by 0396's pending rollout)
- [x] **Docs updated** — `docs/architecture/security/clickhouse-rbac.md`: the
      matrix matches the box, and the tenancy paragraph says what BE users can
      reach in `prices.*` after this change (ADR 0032)
      — updated; also the CN map row for `lambda-enrichment-<environment>`,
      and `docs/runbooks/live-tail-cutover.md`, which still said that CN
      stays unmapped

## Implementation Notes

### Step 1: what each user touches

Derived from the code, then checked against `system.query_log` on
`ch-prod-01`, 2026-09-28. The log covered 2026-09-01 → 2026-09-28 and was
filtered to `type != 'QueryStart'`.

- `api_reader`: 166,579 queries, all `Select`.
  - Outside `default.*` it reads only `prices.price_usd_series(_1h)`, plus
    the tables under those views: `prices.assets`, `price_ohlcv_1h`,
    `price_ohlcv_1d` and `usd_rate`.
  - It also reads `system.one`, which needs no grant.
  - No table functions.
- `ingestion_writer`: 9.26 M `Insert`, 1.25 M `Select` and 11,665 `Describe`
  (the `clickhouse` crate's insert validation).
  - It touches only `default.*` and `system.one`.
  - The log includes tables the code no longer names
    (`operations_appearances`, `contract_transactions`, …).
  - The Caddy map has `CN=lambda-enrichment-production → ingestion_writer`,
    so the enrichment worker is covered by this grant.
- `galexie`: 0 queries. It writes to S3 only.
- `dev_read`: reads `default`, `prices` and 40+ `system.*` tables.
  - Table functions: `clusterAllReplicas` (39 queries, all 2026-09-25
    `query_log` analyses), `format` (2), `numbers` (2) and `values` (2).
- `dict_reader`: 0 queries, and the box has 0 dictionaries.
- Materialized views: every MV in `default` and `prices` is refreshable, and
  none fires on INSERT. So `ingestion_writer` needs no grant on any MV
  target.

### Local verification

CH 26.3 with the new `users.d/` mounted:

- `SHOW GRANTS` matches the XML. The empty `<grants/>` leaves `galexie` with
  no grants.
- Allowed as intended:
  - SELECT on `default` and on a `prices` view over a `prices` table
  - INSERT, DESCRIBE and the anti-join shape
  - `SELECT 1`
  - for `dev_read`: `numbers`, `values`, SHOW, DESCRIBE, EXPLAIN and
    `system.*`
- Refused with 497:
  - `url`, `remote`, `s3` and `file`
  - `INSERT INTO FUNCTION url()`
  - `prices.*` for `ingestion_writer`
  - `ALTER DELETE`, `OPTIMIZE`, `KILL` and `SYSTEM`
  - `system.parts` for the Lambda users
  - `clusterAllReplicas` and `format()` for `dev_read`

## Issues Encountered

- **Pushing hit `ENOSPC` on the laptop.** The pre-push clippy build filled
  the disk. After space was freed the push ran with hooks on, and
  `CARGO_TARGET_DIR` pointed at an external disk.
- **The deploy was run by hand.** The agent's permission policy refuses
  production deploys, so the operator ran the one `cat >` command from their
  terminal. The agent did the read-only checks before and after.
- **Unfiltered probes on the `price_usd_series*` views fail with 241**
  (`MEMORY_LIMIT_EXCEEDED`), not 497. `SELECT * … LIMIT 1` over a whole
  series goes over the profile's memory cap, and the access check passes
  before execution starts. `EXPLAIN` is the probe to use for these views.

## Design Decisions

### From Plan

1. **Inline `<grants>` in `services.xml`**, the same shape as the prices
   users.

### Emerged

2. **Grants per database, not per table.** Per-table grants would break
   ingestion loudly whenever a new table landed before the matching
   `services.xml` deploy. The real risks are `prices.*`, external sources,
   `ALTER`, `SYSTEM` and secrets, and `default.*` excludes all of them.
   `default.*` also matches the RBAC matrix's own wording.
3. **`api_reader` gets `SELECT ON prices.*`, not per view.** The two views
   run as INVOKER and read four `prices` tables underneath, per the query
   log.
4. **`galexie` gets an empty `<grants/>`** and keeps its user and CN, so
   nothing changes in Caddy. Dropping the user can come later.
5. **`dev_read` is scoped here** to `SELECT` on `default.*`, `prices.*` and
   `system.*`.
   - No `REMOTE`, so `clusterAllReplicas` is refused; `system.x` reads the
     same rows on a single node.
   - `format()` is refused as well; it was used twice in 28 days.
6. **`dict_reader` is left alone in this task.** It is box drift: `dict.xml`
   is not in the repo, but the box's compose files still mount it. It is
   loopback-only and unused, yet it holds `ALL` on `default` plus `URL`,
   `REMOTE`, `FILE` and `S3` globally. Removing it changes the compose
   mounts and recreates the container, so it cannot be an in-place users.d
   sync. Task 0396 already removes it, so no follow-up was filed.
   - 0396's code is merged (`8c130eb4`), and its rollout step 1 has run: the
     box has 0 dictionaries.
   - Its step 2 (`--tags app`) is still pending. As of 2026-09-28 `dict.xml`
     is still on the box and still mounted.

## Deploy and verification

### How

Deploy `services.xml` alone, in place, the way 0561 did: an
`ssh … 'cat > /srv/app/crates/db-clickhouse/users.d/services.xml'`. The
inode is kept and ClickHouse hot-reloads the file. The `cat >` must be the
only command reading stdin.

Do not use `--tags app`. Its users.d rsync runs with `delete: true`, so
once 0396 is on the checkout it deletes `dict.xml`. The compose sync then
drops the mount, and the container is recreated: not in place, and it
couples this deploy to 0396's rollout.

### Record, 2026-09-29

- Before: the box's `services.xml` was byte-identical to the repo before this
  change, and a copy was kept on the box as `/tmp/services.xml.pre-0591`.
- Deployed 06:33:11 UTC with
  `git show origin/develop:crates/db-clickhouse/users.d/services.xml | ssh sorban-prod 'cat > /srv/app/crates/db-clickhouse/users.d/services.xml'`.
- The inode was the same before and after (`16777410`) on the host and in the
  container, and the container saw the new file (9,793 bytes, 21 `GRANT`
  lines).
- `SHOW GRANTS` matched the file for `api_reader`, `ingestion_writer`,
  `galexie` (none) and `dev_read`.
- Probes, run as each user through `clickhouse-client --user` inside the
  container, 13/13 as expected:
  - allowed: `SELECT` on `default.ledgers` (both Lambda users), on
    `prices.asset_metadata` (`api_reader`, `dev_read`), and on `system.parts`
    (`dev_read`)
  - refused with 497: `url()` (`api_reader`, `ingestion_writer`, `dev_read`),
    `remote()` (`api_reader`), `INSERT INTO prices.asset_metadata … WHERE 0`
    (`api_reader`, `ingestion_writer`), any read as `galexie`, and
    `clusterAllReplicas` (`dev_read`)
- `api_reader` can read every view in `prices`. `current_price_usd`,
  `identity_by_contract`, `usd_reference` and `usd_reference_1h` answered.
  The four `price_usd_series*` views passed `EXPLAIN`.
- Live traffic over HTTP from 06:33:11 to 06:40:08:
  - `ingestion_writer`: 1,832 queries, 0 errors
  - `api_reader`: 20 queries, 0 errors, none of them on `prices.*`, so that
    path is proven by the probes only
  - `max(sequence)` in `ledgers` advanced over the check

## Future Work

No new task. What is left:

- `dict_reader` and `dict.xml` go with step 2 of 0396's rollout
  (`--tags app`), which is still pending.
- Remove `/tmp/services.xml.pre-0591` on the box once `api_reader` has served
  a `prices.*` read without error.
- Tell the `dev_read` cert holders that `clusterAllReplicas` and `format()`
  are refused now; `system.*` read on the one node returns the same rows.
