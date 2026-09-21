---
id: '0567'
title: 'prices_admin: a prices.*-scoped ClickHouse user for partition operations, so the prices re-ingest does not need dev_shared'
type: OPS
status: completed
related_adr: []
related_tasks: ['0314', '0477', '0561']
tags: ['clickhouse', 'prices-api', 'rbac', 'effort-small']
links: []
history:
  - date: '2026-09-21'
    status: active
    who: okarcz
    note: >
      Created from the prices side. Their task 0286 phase 3 rebuilds every
      monthly candle partition (snapshot to a backup table, DROP PARTITION,
      re-ingest, DROP + pre-roll the coarse tiers). prices_writer holds only
      SELECT/INSERT/ALTER DELETE/OPTIMIZE and cannot be granted more
      (users_xml storage), so the only mTLS identity able to do it today is
      dev_shared — which holds DROP on every database on the shared box.
      This adds a user scoped to prices.* instead. Same shape as 0314's
      prices users; same in-place-inode application path as 0477.
  - date: '2026-09-21'
    status: completed
    who: okarcz
    note: >
      PR #468 merged (b046e1e2) and APPLIED on the box the same afternoon,
      box-side like 0477: services.xml overwritten in place (inode 16777410
      kept, 6329 -> 7742 bytes), ClickHouse hot-reloaded — SHOW GRANTS FOR
      prices_admin lists exactly the four grants. CN map: operator secret
      updated (version 6b8c55c9), /srv/caddy/cn_user_map.snippet edited in
      place + `caddy reload` (graceful, no restart). Cert
      prices-admin-production issued (365 d, sha256 C8:27:3F:58…). From the
      campaign machine: currentUser() = prices_admin; CREATE / TRUNCATE /
      DROP of a probe table in prices succeeded. No container restart, no
      BE-visible event. Backups on the box: /tmp/services.xml.bak-0567,
      /tmp/cn_user_map.snippet.bak-0567. Archived.
---

# prices_admin: a prices.\*-scoped ClickHouse user for partition operations

## Summary

Add a third prices-tenant XML user, `prices_admin`, next to `prices_writer`
and `prices_reader`, with DDL and partition rights on `prices.*` only, plus
read-only access to `default.*`. It exists so the prices-api history
re-ingest (their task 0286 phase 3) can run from a campaign machine without
a `dev_shared` certificate, which is admin on the whole shared cluster.

```
GRANT SELECT, INSERT, ALTER, CREATE TABLE, DROP TABLE, TRUNCATE ON prices.*
GRANT SELECT ON default.*
GRANT SELECT ON system.parts
GRANT SELECT ON system.mutations
```

Profile `prices_write_ddl` and quota `prices_write`, the same as
`prices_writer`. Cert CN `prices-admin-production`, mapped in
`CLICKHOUSE_CN_USER_MAP` (operator env, not this repo).

## Status: Completed

**Current state:** live on prod since 2026-09-21 ~14:30 UTC; PR #468 merged.

## Context

The prices re-ingest rebuilds each month by copying its partition into a
backup table (`ATTACH PARTITION … FROM`), dropping it, re-ingesting, then
dropping and re-rolling the coarse tiers; `REPLACE PARTITION` from the backup
is the rollback. Those need CREATE TABLE, ALTER and TRUNCATE. `prices_writer`
cannot hold them: it is defined in `users.d/services.xml`, which ClickHouse
treats as read-only access storage, so `GRANT` fails with
`ACCESS_STORAGE_READONLY` even as the superuser (learned on their 2026-07-23
repair run). Every prices repair runbook since July therefore says "the CH
admin takes the snapshots by hand". A dedicated, scoped user closes that gap
without handing a `dev_shared` cert to a campaign machine.

`SELECT ON default.*` is for the AMM half of the re-ingest, which reads
`default.soroban_events` / `default.transactions` and writes `prices.*`. It
is read-only on our data.

## Implementation

1. Repo (this branch): the `<prices_admin>` block in `services.xml`; a row
   in `docs/architecture/security/clickhouse-rbac.md`.
2. Operator env: append `prices-admin-production:prices_admin` to
   `CLICKHOUSE_CN_USER_MAP` in the operator secret; every operator re-fetches.
3. Box: `ansible-playbook -i inventory.ini site.yml --tags app`. The
   `users.d` sync is `--inplace` (inode preserved, per the comment in
   `roles/app/tasks/main.yml`), so ClickHouse hot-reloads the file — the
   path 0477 verified. The CN map only feeds the Caddy snippet, so the only
   handler is "Reload caddy". Check with `--check --diff` first that
   "Restart compose stack" is NOT notified.
4. Verify: `SHOW GRANTS FOR prices_admin` on the box lists the four grants.
   Only if the user is absent does the container need a recreate — a
   BE-visible event to schedule, not to do ad hoc.
5. Issue the cert with `infra-hetzner/ca/issue-client-cert.sh
prices-admin-production`; hand-over per README.

## Acceptance Criteria

- [x] `SHOW GRANTS FOR prices_admin` on prod lists exactly the four grants
      above and nothing else — verified on the box after the hot-reload.
- [x] A request with the `prices-admin-production` cert returns
      `currentUser() = 'prices_admin'`, can CREATE/TRUNCATE/DROP a probe table
      in `prices` — all verified from the campaign machine. "Refused DDL on
      `default.*`" was not exercised live; it follows from the grant list
      (SELECT only on `default.*`, explicit-grant mode).
- [x] Repo `services.xml` matches the box file byte-for-byte after merge —
      the box copy IS `git show origin/develop:…services.xml`.
- [x] **Docs updated** — `docs/architecture/security/clickhouse-rbac.md` row
      added; other architecture docs N/A (grant list only).
- [x] **API types regenerated** — N/A (no `crates/api` change).

## Issues Encountered

- The deploy laptop has no `ansible-core`, no `inventory.ini` and no
  `~/.config/soroban-prod.env`, so the playbook path was not available.
  Applied box-side instead (runbook below). ⚠️ Consequence for BE: the next
  `--tags app` run rsyncs an identical `services.xml` (no-op) and re-renders
  the Caddy snippet from the updated secret to the same content — nothing
  should change, but the snippet was hand-edited, so a diff there is the
  first thing to look at if a run ever reports a change.
- The first attempt at editing the secret assumed a one-line value; the map
  is multi-line with `\` continuations. Edited with a small script instead.

## Notes

- Revocation is one line: remove the CN from `CLICKHOUSE_CN_USER_MAP` and
  reload Caddy. No CA rotation, no CH restart.
- The user is intended for operator-driven campaigns, not for a Lambda. Its
  cert bundle stays with the operator, never in Secrets Manager next to the
  `prices_writer` bundle.

## 📕 DEPLOY RUNBOOK (box-side, the 0477 path)

The deploy operator's laptop has no `ansible-core`, no `inventory.ini` and no
`~/.config/soroban-prod.env`, so this applies the merged file the way 0477
did: in place on the box (inode preserved → ClickHouse hot-reloads), plus the
Caddy map line by hand and a graceful `caddy reload`. The operator secret is
updated first so the next ansible run renders the same map.

0. **Pre-check (laptop, BE repo)** — the merged file carries the user:
   `git show origin/develop:crates/db-clickhouse/users.d/services.xml | grep -c prices_admin` → ≥ 2.
1. **CN map in the operator secret (laptop → Secrets Manager)** — fetch
   `soroban/production/operator/env` to `/dev/shm`, append
   `,prices-admin-production:prices_admin` inside the quotes of the
   `CLICKHOUSE_CN_USER_MAP=` line, `put-secret-value` from the file, shred.
   Checkpoint: re-fetch and grep the map line for the new pair → 1.
2. **services.xml in place (laptop → box as `deploy`)** — first
   `diff` the box file against the pre-merge repo version (`2deabd12`) →
   empty, else STOP (undeployed drift). Record `stat -c '%i'` of
   `/srv/app/crates/db-clickhouse/users.d/services.xml`, back it up to
   `/tmp`, then `git show origin/develop:… | ssh … "cat > FILE"`.
   Checkpoint: inode unchanged, `grep -c prices_admin` ≥ 2.
3. **ClickHouse reloaded (box)** — `docker exec -i app-clickhouse-1
clickhouse-client -q 'SHOW GRANTS FOR prices_admin'` → the four grants.
   If "no user": ⚠️ container recreate needed — schedule with BE, do not
   do ad hoc.
4. **Caddy map (box, sudo)** — back up `/srv/caddy/cn_user_map.snippet`,
   insert `    "CN=prices-admin-production"    prices_admin` before the
   `default "__unmapped__"` line, then
   `docker exec app-caddy-1 caddy validate --config /etc/caddy/Caddyfile`
   and `docker exec app-caddy-1 caddy reload --config /etc/caddy/Caddyfile`
   (graceful; the ansible handler would restart the container instead).
5. **Cert (laptop, BE repo `infra-hetzner/ca`)** — CA key from Secrets
   Manager `soroban/production/ca/key` into `/dev/shm`,
   `./issue-client-cert.sh prices-admin-production`, shred the CA key.
6. **Hand-over** — scp the bundle to the campaign machine; password-manager
   backup per README; shred the laptop `out/` key.
7. **Final test (campaign machine)** — `SELECT currentUser()` → `prices_admin`;
   `CREATE TABLE prices.prices_admin_probe (x UInt8) ENGINE = Memory`,
   `TRUNCATE TABLE prices.prices_admin_probe`, `DROP TABLE prices.prices_admin_probe`
   all succeed. `Code: 497` on any → a grant is missing.
