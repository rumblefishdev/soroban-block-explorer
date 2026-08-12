---
id: '0477'
title: 'prices_writer monitoring grants: SELECT on system.mutations + system.view_refreshes'
type: OPS
status: active
related_adr: []
related_tasks: ['0314', '0199']
tags: ['clickhouse', 'prices-api', 'rbac', 'effort-small']
links: []
history:
  - date: '2026-08-12'
    status: active
    who: stkrolikiewicz
    note: >
      Created from the prices owner's ask after their 07-21..08-03
      coarse-rollup freeze post-mortem (recorded in 0199's
      R-prices-freeze note). Their new freshness alarm measures data,
      not MV exit status; these two read grants let it also catch the
      precursor signal — pending mutations sat undone for 13 days.
---

# prices_writer monitoring grants: system.mutations + system.view_refreshes

## Summary

Add two read-only grants to the `prices_writer` XML user so the prices-api
freshness alarm can watch ClickHouse's own progress signals:

```
GRANT SELECT ON system.mutations
GRANT SELECT ON system.view_refreshes
```

Requested by the prices owner after the 07-21 → 08-03 coarse-rollup freeze:
their rollup MVs reported success while rolling up nothing for 13 days, and
pending mutations sitting undone was the one signal that would have flagged
it days earlier. Same shape as the `system.parts` grant from task 0314.

## Context

Runtime CH users are XML-managed on our side
(`crates/db-clickhouse/users.d/services.xml`), synced to
`/srv/app/crates/db-clickhouse/users.d/` by the ansible `app` role and
bind-mounted **per file** into the container. Task 0314 established the
gotcha: ansible's write-and-rename gives the host file a new inode, the
single-file mount keeps the old one, so a deploy alone never applies a
grants change and a plain restart doesn't help — only container recreate
does (SQL `GRANT` is impossible: `users_xml` storage is read-only).

## Implementation

Applied WITHOUT a container recreate, by keeping the inode:

1. Repo change (this branch): the two `<query>` grants in `services.xml`
   + `docs/architecture/security/clickhouse-rbac.md` row updated.
2. Box (`sorban-prod`): overwrite the mounted file IN PLACE
   (`cat new > services.xml` — truncate+write preserves the inode; NOT
   `sed -i` / `scp` / editors, which write-and-rename). CH hot-reloads
   `users.d` on content change; the container sees it because the inode
   never changed.
3. Verification: config-reload line in the CH log + `SHOW GRANTS FOR
   prices_writer` + prices owner confirms a live `SELECT count() FROM
   system.mutations` under their credentials.
4. The merged repo state then matches the box byte-for-byte, so the next
   `--tags app` deploy rsyncs nothing and the inode stays put.

## Acceptance Criteria

- [ ] Both grants live on prod, verified by the prices owner from their side.
- [ ] Repo (`services.xml`) matches the box file byte-for-byte after merge.
- [ ] **Docs updated** — `docs/architecture/security/clickhouse-rbac.md`
      (prices_writer row); other architecture docs N/A (no shape change
      beyond the grant list).
- [ ] **API types regenerated** — N/A (no `crates/api` change).

## Future Work

- 0314's open follow-up stands: the ansible `users.d` sync still does not
  notify a container recreate, so any grants change applied the NORMAL way
  (deploy only) silently no-ops. This task dodges it via the in-place edit;
  the playbook fix remains unowned.
