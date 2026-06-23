---
id: '0314'
title: 'Add prices tenant ClickHouse RBAC (prices_writer / prices_reader)'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0240']
tags:
  [
    'phase-infra',
    'effort-small',
    'priority-medium',
    'clickhouse',
    'rbac',
    'cross-repo',
  ]
links:
  - '.temp/G-be-rbac-pr-description.md'
history:
  - date: 2026-06-23
    status: active
    who: fmazur
    note: 'Task created — cross-repo request from prices-api (their task 0063, ADR 0007).'
---

# Add prices tenant ClickHouse RBAC (prices_writer / prices_reader)

## Summary

Add a second ClickHouse tenant — **`prices`** — to the shared Hetzner cluster as
checked-in RBAC config. The prices-api service (a separate repo, their task 0063,
per their ADR 0007) lands per-source OHLCV candles into a dedicated `prices`
database in **our** cluster so BE in-cluster analytics (e.g. LP-analytics
`price_usd_series` JOIN) can read prices data without a network hop.

This change is **access-control only**: two scoped users, two dedicated quotas,
and (deploy-time, not in repo) two Caddy CN→user mappings. The `prices` database
and its schema are created/owned by prices-api over loopback admin — not here.

**Also bundled in this task** (same `quotas.xml`, separate requester): raise the
`dev_read` quota — `read_rows` 50B→100B and `read_bytes` 1TiB→2TiB — on a dev
request, since devs hit the 50B hourly aggregate on heavier multi-scan sessions.

## Context

On this deployment, ClickHouse tenant users/quotas are defined as checked-in XML
under `crates/db-clickhouse/users.d/` and delivered to the box by Ansible — they
are **not** created with `CREATE USER` SQL. An ad-hoc `CREATE USER` on the box
would drift/be wiped on the next deploy. So the only durable, deploy-reproducible
way to create the prices service identities is to add them to this XML — the one
piece prices-api cannot self-serve. Everything else (box-admin access via their
task 0227, DB creation, schema, mTLS client certs) is on the prices-api side.

Source runbook: `.temp/G-be-rbac-pr-description.md`. Mirrors the existing
per-service tenant pattern from [[task-0240]].

## Implementation Plan

### Step 1: services.xml — add scoped users

Add `prices_writer` (profile `write_no_ddl`, quota `prices_write`, granted only
`SELECT, INSERT, OPTIMIZE ON prices.*`) and `prices_reader` (profile `read_only`,
quota `prices_read`, granted only `SELECT ON prices.*`). Both `<no_password/>`
with the standard loopback + `172.30.0.0/16` networks ACL. These are the **first**
users to carry an inline `<grants>` block — which both scopes them to `prices.*`
and flips them into explicit-grant mode (denies `default.*`).

### Step 2: quotas.xml — add dedicated quotas

Add `prices_write` (caps copied verbatim from `high_write`) and `prices_read`
(caps copied verbatim from `api_throttle`) so prices traffic can never draw down
a BE service's per-user budget.

### Step 3: profiles.xml — no change

`prices_writer` reuses `write_no_ddl`; `prices_reader` reuses `read_only`.

### Step 4: Evergreen docs (ADR 0032)

Update `docs/architecture/security/clickhouse-rbac.md`: two new rows in the
per-service user matrix, two new quotas in the Profiles and quotas section, and
a CN→user convention note for the prices CNs.

### Step 5: quotas.xml — bump `dev_read` (bundled, separate requester)

Raise the `dev_read` quota: `read_rows` 50B→100B (`50000000000`→`100000000000`)
and `read_bytes` 1TiB→2TiB (`1099511627776`→`2199023255552`). Both fields raised
together so `read_bytes` (the real IO ceiling) does not become the binding cap
and negate the row bump. `dev_read` is per-user and isolated from `api_throttle`,
so this cannot reintroduce the 0290 failure mode; the `read_only` 30s/4GB
per-query cap remains the real guard.

### Step 6: Deploy (operator, post-merge — not a repo change)

Append the prices CNs to `CLICKHOUSE_CN_USER_MAP`:
`prices-ingestion:prices_writer,prices-api:prices_reader`, then
`ansible-playbook … --tags app`. Coordinated with prices-api for cert issuance.
(The `dev_read` bump takes effect on the same `--tags app` CH config reload.)

## Acceptance Criteria

- [ ] `prices_writer` + `prices_reader` added to `services.xml` with inline `<grants>`
- [ ] `prices_write` + `prices_read` added to `quotas.xml`
- [ ] `dev_read` quota raised: `read_rows` 50B→100B, `read_bytes` 1TiB→2TiB
- [ ] `profiles.xml` unchanged
- [ ] **Docs updated** — `docs/architecture/security/clickhouse-rbac.md` reflects the
      new users + quotas + CN convention (ADR 0032).
- [ ] **API types regenerated** — N/A — no `crates/api/**`, `Cargo.*`, or
      `libs/api-types/**` change.
- [ ] Reviewer-confirmed: box's CH version applies user-XML `<grants>` at startup
      (supported since ~21.4). Fallback agreed if not: SQL `GRANT … ON prices.*`
      via prices-api loopback admin init.

## Notes

- Decision (with user): use inline `<grants>` in XML (deploy-reproducible,
  single source file) rather than the loopback-admin SQL fallback.
- Granting `ON prices.*` before the `prices` DB exists is fine — CH grants are
  name-based and do not require the object to exist.
- Known/accepted: our own unscoped service users (`ingestion_writer`, etc.) can
  still reach `prices.*`. That is inside our trust boundary. The isolation that
  matters is that prices certs are confined to `prices.*` and cannot touch
  `default.*`.
