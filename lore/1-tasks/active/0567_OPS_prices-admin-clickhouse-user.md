---
id: '0567'
title: 'prices_admin: a prices.*-scoped ClickHouse user for partition operations, so the prices re-ingest does not need dev_shared'
type: OPS
status: active
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

## Status: Active

**Current state:** PR #468 open (`ops/0567_prices-admin-clickhouse-user` → develop);
not yet applied on the box.

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

- [ ] `SHOW GRANTS FOR prices_admin` on prod lists exactly the four grants
      above and nothing else.
- [ ] A request with the `prices-admin-production` cert returns
      `currentUser() = 'prices_admin'`, can CREATE/TRUNCATE/DROP a probe table
      in `prices`, and is refused DDL on `default.*`.
- [ ] Repo `services.xml` matches the box file byte-for-byte after merge.
- [ ] **Docs updated** — `docs/architecture/security/clickhouse-rbac.md` row
      added; other architecture docs N/A (grant list only).
- [ ] **API types regenerated** — N/A (no `crates/api` change).

## Notes

- Revocation is one line: remove the CN from `CLICKHOUSE_CN_USER_MAP` and
  reload Caddy. No CA rotation, no CH restart.
- The user is intended for operator-driven campaigns, not for a Lambda. Its
  cert bundle stays with the operator, never in Secrets Manager next to the
  `prices_writer` bundle.
