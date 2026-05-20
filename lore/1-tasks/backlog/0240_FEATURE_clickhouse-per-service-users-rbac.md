---
id: '0240'
title: 'FEATURE: ClickHouse per-service users + RBAC profiles + quotas (Layer 3 defense-in-depth)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0216', '0227', '0239']
tags:
  [
    priority-high,
    effort-medium,
    layer-infrastructure,
    clickhouse,
    rbac,
    security,
    defense-in-depth,
  ]
links: []
history:
  - date: '2026-05-20'
    status: backlog
    who: fmazur
    note: 'Spawned during 0238 scope review — 0216 decision document specifies a three-layer defense-in-depth (TLS + mTLS + CH-side RBAC) but 0227 delivered only Layers 1–2. Currently every mTLS-authenticated client reaches CH as the single `default` user with full permissions. This task adds Layer 3: per-service CH users, profiles and quotas matching each AWS service / dev consumer.'
---

# FEATURE: ClickHouse per-service users + RBAC profiles + quotas

## Summary

Replace the single shared `default` ClickHouse user with **per-service
users**, each bound to a profile (read-only / write-no-DDL / admin) and
a quota (queries-per-hour, memory, execution time). Closes the
defense-in-depth Layer 3 gap left by 0227 — currently a compromised
client (Lambda, Galexie, dev laptop) authenticated by mTLS still has
unrestricted DDL access to the entire database.

## Context

The three-layer security architecture decided in [[task-0216]] is:

1. **Layer 1** — TLS encryption (Caddy + Let's Encrypt) ✅ done in
   [[task-0227]]
2. **Layer 2** — mTLS client cert verify + CN allowlist (Caddy
   `client_auth.mode require_and_verify` + Ansible-rendered snippet)
   ✅ done in [[task-0227]]
3. **Layer 3** — ClickHouse user + RBAC + quotas — **not yet
   implemented**

Without Layer 3, the three "independent layers" collapse to two
effective layers, and a compromise of any single mTLS client cert
(Lambda IAM principal stolen, dev laptop unrevoked, Galexie task
hijacked) yields the same blast radius as compromising the
shared CH password: full read/write/DDL on every table.

This task is a **strict pre-requirement of [[task-0239]] Phase 6
step 2** (drop RDS). Before AWS-side compute starts depending on
ClickHouse as the sole datastore, RBAC must constrain blast radius
on a per-service basis.

## Scope

### Per-service user matrix

Final layout — one CH user per AWS service + per developer:

| CH user                  | Profile          | Quota          | Permitted operations                               | Consumer                       |
| ------------------------ | ---------------- | -------------- | -------------------------------------------------- | ------------------------------ |
| `default`                | `admin`          | `unlimited`    | Everything (kept for sidecar init.sql + emergency) | `db-clickhouse-init` sidecar   |
| `galexie`                | `write_no_ddl`   | `high_write`   | INSERT only on ingestion tables                    | Galexie ECS task               |
| `api_reader`             | `read_only`      | `api_throttle` | SELECT on `default.*`                              | Lambda API (read-heavy)        |
| `ingestion_writer`       | `write_no_ddl`   | `high_write`   | INSERT on tables Galexie does not touch            | Lambda Ingestion               |
| `partition_admin`        | `partition_only` | `low_volume`   | `ALTER TABLE ... DROP PARTITION` + SELECT          | Lambda Partition mgmt          |
| `migration_admin`        | `migration_full` | `low_volume`   | DDL + read + write (one-shot migrations)           | Lambda Migration               |
| `<firstname>_dev`        | `dev_limited`    | `dev_throttle` | SELECT + limited INSERT on `default.*`             | Dev laptops (one per operator) |
| `dict_reader` (existing) | `read_only_lan`  | n/a (loopback) | SELECT inside container (loopback only)            | Dictionary SOURCE clause       |

### Profiles to define

Add to `crates/db-clickhouse/users.d/profiles.xml`:

- `admin` — `readonly=0`, `allow_ddl=1`, large memory + execution
  limits
- `read_only` — `readonly=1`, `max_memory_usage` capped,
  `max_execution_time` 30s
- `write_no_ddl` — `readonly=0`, `allow_ddl=0`, optimised for INSERT
  throughput (`max_insert_block_size`, `min_insert_block_size_rows`)
- `partition_only` — `readonly=0`, `allow_ddl=0`, plus a SQL-level
  grant for `ALTER TABLE ... DROP PARTITION` only
- `migration_full` — `readonly=0`, `allow_ddl=1`, time-limited
  execution (no hung migrations)
- `dev_limited` — `readonly=0`, `allow_ddl=0`,
  `max_insert_block_size` small (devs don't ingest at scale)
- `read_only_lan` — same as `read_only` but `networks` field limits
  to loopback (already in place for `dict_reader`)

### Quotas to define

Add to `crates/db-clickhouse/users.d/quotas.xml`:

- `unlimited` — keepalive for emergency / sidecar
- `api_throttle` — `10000 queries / hour`, `1B read_rows / hour`,
  `100 GB read_bytes / hour`, `1000s execution_time / hour`
- `high_write` — `unbounded queries`, `unbounded read`, `1 PB
written_bytes / hour` ceiling (sanity, not a real cap)
- `low_volume` — `100 queries / hour` (migration / partition jobs
  are infrequent)
- `dev_throttle` — `1000 queries / hour`, `100 M read_rows / hour`,
  per-dev so one dev's bad query doesn't starve others

### Files

```
crates/db-clickhouse/users.d/
├── default.xml          (existing — kept, retains admin role)
├── dict.xml             (existing — dict_reader, no change)
├── profiles.xml         (NEW — all profile definitions)
├── quotas.xml           (NEW — all quota definitions)
├── services.xml         (NEW — galexie + lambda-* users)
└── devs.xml             (NEW — per-developer users, Ansible-rendered)
```

`devs.xml` is Ansible-rendered from `OPERATOR_SSH_PUBKEYS`-adjacent
env (one user per dev whose CN is on the mTLS allowlist). Same
mechanism as the Caddy CN allowlist (template re-renders on
`--tags app`).

### Password management

CH user passwords delivered in the **same Secrets Manager bundle**
as the mTLS cert (from [[task-0239]] Phase 1). Bundle shape grows
from `{cert, key, ca}` to:

```json
{
  "cert": "-----BEGIN CERTIFICATE-----...",
  "key": "-----BEGIN PRIVATE KEY-----...",
  "ca": "-----BEGIN CERTIFICATE-----...",
  "ch_user": "galexie",
  "ch_password": "<random-32-byte>"
}
```

Lambda / Galexie code reads both blocks from the secret at startup,
configures the CH client with both mTLS material AND HTTP Basic
Auth. Per-service password rotation is independent from cert
rotation.

Hetzner-side: passwords land in `~/.config/soroban-prod.env` as
`CLICKHOUSE_PASSWORD_<USER>` env vars; Ansible renders them into
the rendered users.d XML.

### Ansible changes

- New `app` role task: render `services.xml` and `devs.xml`
  templates from env-sourced passwords + dev-CN list.
- Update preflight assertion in `site.yml` to require the new env
  vars (`CLICKHOUSE_PASSWORD_GALEXIE`, `CLICKHOUSE_PASSWORD_API_READER`,
  etc.).
- Restart CH container on change (handler).

### init.sql changes

- Sidecar continues to run as `default` user (admin) so DDL is
  permitted.
- After table creation, sidecar issues `GRANT SELECT ON default.*
TO api_reader;`, `GRANT INSERT ON default.ledgers TO galexie;`,
  etc. — SQL-level grants overlay XML-level profiles for tighter
  control.
- Idempotent: every GRANT is `IF NOT EXISTS`-equivalent in CH.

### Caddy mTLS → CH user mapping

CN allowlist (Caddy snippet) and CH user are **independent identities** —
Caddy verifies the cert, then the client presents the matching CH
user/password in the HTTP Authorization header. Naming convention
ties them together for audit clarity:

| Caddy CN (mTLS allowlist)        | Expected CH user   |
| -------------------------------- | ------------------ |
| `galexie-<environment>`          | `galexie`          |
| `lambda-api-<environment>`       | `api_reader`       |
| `lambda-ingestion-<environment>` | `ingestion_writer` |
| `lambda-partition-<environment>` | `partition_admin`  |
| `lambda-migration-<environment>` | `migration_admin`  |
| `<firstname>-laptop`             | `<firstname>_dev`  |

Caddy access logs already forward `X-Client-Subject` (full DN) +
`X-Client-Cert-Fingerprint`. ClickHouse access logs record the CH
user — joining the two streams gives full provenance (which cert
holder performed which query).

### Stacks / code affected (Lambda + Galexie side, in [[task-0239]])

This task scoped to **CH-side config only**. The Lambda code
changes that consume the new bundle shape are part of
[[task-0239]] Phase 2 — that task gains a sub-bullet to read the
extra `ch_user` + `ch_password` fields. Coordinate the two tasks
so the Secrets Manager bundle is upgraded in lockstep with the
Lambda runtime expectations.

## Acceptance Criteria

- [ ] All seven new profiles defined in
      `crates/db-clickhouse/users.d/profiles.xml` and applied by
      CH (verify via `SELECT * FROM system.settings_profiles`).
- [ ] All five new quotas defined in
      `crates/db-clickhouse/users.d/quotas.xml` and applied (verify
      via `SELECT * FROM system.quotas`).
- [ ] Per-service users created with the right profile + quota
      binding (verify via `SELECT * FROM system.users`).
- [ ] Dev users Ansible-rendered into `devs.xml` from env;
      re-running with no env change is a no-op.
- [ ] Negative test: connect as `api_reader`, attempt `DROP TABLE`
      — expect `Cannot execute query in readonly mode` (or
      `Not enough privileges`).
- [ ] Negative test: connect as `galexie`, attempt `ALTER TABLE
... ADD COLUMN` — expect rejection.
- [ ] Negative test: connect as `partition_admin`, attempt
      `INSERT` — expect rejection.
- [ ] Quota smoke: connect as `api_reader`, fire 10001 trivial
      queries within an hour — expect 10001st rejected with
      `quota exceeded`.
- [ ] Sidecar `db-clickhouse-init` still runs `init.sql` as
      `default` and the GRANT statements succeed idempotently
      across multiple boots.
- [ ] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.
- [ ] **Docs updated** —
      `docs/architecture/security/clickhouse-rbac.md` (NEW)
      documents the per-service user matrix + rotation procedure;
      `infra-hetzner/README.md` operating-model section adds a
      pointer to it.

## Dependencies

- [[task-0227]] — `users.d/` mount mechanism and Ansible
  template-rendering pattern (✅ delivered).
- [[task-0239]] Phase 1 (Secrets Manager bundles) is a soft
  dependency: this task can define CH-side config independently,
  but the Lambda / Galexie code can only consume the new users
  after the bundle shape is upgraded — coordinate ordering.

## Risks / Considerations

- **Sidecar GRANT idempotency**: ClickHouse's `GRANT` is
  technically idempotent (re-grant is a no-op if the grant exists)
  but error messages differ across CH versions. Verify on 26.3
  before committing the sidecar SQL.
- **Password rotation overhead**: each service has a separate
  password now. Document rotation procedure (update Secrets
  Manager → Ansible re-render → CH container restart) — should
  this be its own follow-up task or a section in this task's
  README? Default: section here.
- **`partition_only` profile is granular**: CH does not have a
  native "DROP PARTITION but not ALTER" privilege bit; have to
  combine `allow_ddl=0` (which blocks ALTER) with a SQL-level
  `GRANT ALTER TABLE` (which re-enables specific subset). Verify
  this combination actually works in CH 26.3 — if not, fall back
  to a broader `write_with_alter` profile and document the
  permission surface.
- **Quota false-positives on burst traffic**: `api_throttle` =
  10k queries/hour. If Lambda API hits an unexpected burst (e.g.
  cache invalidation), it gets throttled. Pre-cutover, observe
  current RDS qps as a baseline and adjust the cap if needed.
- **`default` user kept**: deliberate. Sidecar needs admin to
  create tables + grants. The `default` password remains in
  `CLICKHOUSE_PASSWORD` (one secret), only accessible from inside
  the box (sidecar + emergency operator SSH). External services
  cannot reach `default` because the Caddy CN allowlist routes
  them to per-service certs which map to per-service users.

## Out of Scope

- SQL-RBAC migration away from XML `users.d/`. CH supports
  `CREATE ROLE` + `GRANT TO ROLE` since 20.10 — cleaner for
  team-of-N management, but the static XML approach is fine for
  ≤10 services. Spawn a separate refactor task if/when the
  user matrix grows beyond manageable XML.
- Per-table column-level grants (CH supports `GRANT SELECT
(column1, column2) ON table`). Not needed for the current
  service set; add if business requirements emerge.
- ClickHouse-side audit log shipping (currently `system.query_log`
  retains 30 days, queryable directly). Long-term log archival
  is its own observability task.
- Multi-tenant isolation (separate CH databases per tenant). Not
  applicable to our indexer use case.
