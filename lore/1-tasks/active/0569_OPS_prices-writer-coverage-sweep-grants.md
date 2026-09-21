---
id: '0569'
title: 'prices_writer: SELECT on default.soroban_events + default.soroban_contracts for the weekly pool-coverage sweep'
type: OPS
status: active
related_adr: []
related_tasks: ['0477', '0314', '0567']
tags: ['clickhouse', 'prices-api', 'rbac', 'effort-small']
links: []
history:
  - date: '2026-09-21'
    status: active
    who: okarcz
    note: >
      Created from the prices-api ask (their task 0100). Same shape as 0477:
      two read grants on prices_writer, applied in place on the box. Scope is
      deliberately the two tables, not default.*.
---

# prices_writer: SELECT on default.soroban_events + default.soroban_contracts

## Summary

Add two read-only grants to the `prices_writer` XML user, under the existing
`system.view_refreshes` line:

```
GRANT SELECT ON default.soroban_events
GRANT SELECT ON default.soroban_contracts
```

Only SELECT, only these two tables — not `default.*`. No new user, cert, CN
map entry, profile or quota.

## Status: Active

**Current state:** branch `ops/0569_prices-writer-coverage-sweep-grants`,
chained on `ops/0568_prices-admin-system-reads` (same doc table); not applied.

## Context

Once a week (Monday 05:17 UTC) a prices-api Lambda, connecting as
`prices_writer` over mTLS (CN `prices-ingestion-production`), looks for
contracts emitting `swap` / `trade` events that its `prices.pool_registry`
does not know — the venue-discovery sweep that would have caught SushiSwap V3
months earlier. Today those reads return `Code: 497 ACCESS_DENIED` (checked
2026-09-21).

Measured load on production: one pass reads ~218.5 M rows / 46.6 GB in
9.6 s with 141 MB memory; weekly, one query at a time, no retries, each
query capped at 50 s client-side.

## Implementation

1. Repo (this branch): two `<query>` grants + comment in `services.xml`;
   `docs/architecture/security/clickhouse-rbac.md` row updated.
2. Box: the 0477 / 0567 path — the mounted file overwritten IN PLACE so the
   inode is kept and ClickHouse hot-reloads `users.d`. ⚠️ The 0477 trap is
   half-fixed: the ansible `users.d` sync is now `--inplace` (inode kept), but
   the deploy operator's laptop has no ansible set up, so the box-side
   overwrite remains the applied path here. Verified on 0567 the same day:
   inode 16777410 kept, hot-reload, no container restart.
3. Verify: CH log shows the config reload; `SHOW GRANTS FOR prices_writer`
   lists the two new lines plus the existing `prices.*`, `system.parts`,
   `system.mutations`, `system.view_refreshes`.
4. Rollback: overwrite in place from the backup, then revert the commit.
5. Merge and release so the repo matches the box byte for byte.

Verification on the prices side, with their `prices_writer` cert:
`SELECT count() FROM default.soroban_events WHERE 0` and the same for
`soroban_contracts` return 0; `default.transactions` still returns 497.

## Acceptance Criteria

- [ ] `SHOW GRANTS FOR prices_writer` on prod lists the two new SELECTs and
      the four existing grants, nothing else.
- [ ] prices-side check passes: both tables readable, `default.transactions`
      still denied.
- [ ] Repo `services.xml` matches the box file byte-for-byte after merge.
- [ ] **Docs updated** — `clickhouse-rbac.md` row; other architecture docs
      N/A.
- [ ] **API types regenerated** — N/A (no `crates/api` change).
