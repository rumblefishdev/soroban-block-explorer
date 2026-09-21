---
id: '0568'
title: 'prices_admin: SELECT on system.columns + system.disks, so the re-ingest script needs no dev_read cert on the campaign machine'
type: OPS
status: active
related_adr: []
related_tasks: ['0567', '0477']
tags: ['clickhouse', 'prices-api', 'rbac', 'effort-small']
links: []
history:
  - date: '2026-09-21'
    status: active
    who: okarcz
    note: >
      Spawned from 0567 the same afternoon. The re-ingest script's reader
      role needs two system tables (schema gate on system.columns, disk gate
      on system.disks) that prices_admin cannot read, so its default reader
      is dev_read — a personal laptop cert the operator does not want copied
      to the campaign machine, and a profile capped at 30 s per query that
      the month-wide FINAL sums may not fit. Two read-only grants let the
      admin cert be the reader too.
---

# prices_admin: SELECT on system.columns + system.disks

## Summary

Add two read-only grants to the `prices_admin` XML user from task 0567:

```
GRANT SELECT ON system.columns
GRANT SELECT ON system.disks
```

With them the prices re-ingest script can run every check under the admin
identity (profile `prices_write_ddl`, no execution cap) instead of a
developer's `dev_read` cert (profile `read_only`, 30 s / 4 GB per query,
2 TiB/h quota). No write scope changes: `prices_admin` still writes to
`prices.*` only.

## Status: Active

**Current state:** branch `ops/0568_prices-admin-system-reads`; not applied.

## Context

The script's reader role reads `system.columns` (the "21 pf columns present"
gate) and `system.disks` (free-space gate, per month). `prices_writer` and
`prices_admin` hold SELECT on `system.parts` / `system.mutations` only —
`system.disks` is a known ACCESS_DENIED for the writer. Both tables are
metadata: column names/types of tables the user can already see, and volume
free/total space. Same shape as 0477's `system.mutations` grant.

## Implementation

1. Repo: two `<query>` lines in the `prices_admin` block; doc row updated.
2. Box: in-place overwrite of the mounted `services.xml` (inode kept),
   the 0567 / 0477 path. No Caddy change — the CN map is untouched.
3. Verify: `SHOW GRANTS FOR prices_admin` → six lines; from the campaign
   machine `SELECT min(free_space) FROM system.disks` under the admin cert
   returns a number.

## Acceptance Criteria

- [ ] `SHOW GRANTS FOR prices_admin` on prod lists the two new SELECTs and
      nothing else new.
- [ ] Under the `prices-admin-production` cert, `system.disks` and
      `system.columns` are readable from the campaign machine.
- [ ] Repo `services.xml` matches the box file byte-for-byte after merge.
- [ ] **Docs updated** — `docs/architecture/security/clickhouse-rbac.md`
      row; other architecture docs N/A.
- [ ] **API types regenerated** — N/A (no `crates/api` change).
