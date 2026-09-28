---
id: '0591'
title: 'OPS: api_reader and ingestion_writer have no <grants> — ClickHouse gives them ALL ON *.*'
type: OPS
status: active
related_adr: ['0032']
related_tasks: ['0240', '0314', '0567', '0568', '0569']
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

- [ ] `api_reader` and `ingestion_writer` carry `<grants>` limited to what
      Step 1 found; neither holds anything `ON *.*`
- [ ] `URL`, `REMOTE`, `S3` and the other external-source privileges, and
      `displaySecretsInShowAndSelect`, are gone from both unless a named need
      is written here
- [ ] Deployed to production in place, verified with `SHOW GRANTS` and the
      negative probes; API and ingestion healthy afterwards
- [ ] `galexie`, `dev_read`, `dict_reader`: measured, and scoped here or moved
      to a follow-up task
- [ ] **Docs updated** — `docs/architecture/security/clickhouse-rbac.md`: the
      matrix matches the box, and the tenancy paragraph says what BE users can
      reach in `prices.*` after this change (ADR 0032)
