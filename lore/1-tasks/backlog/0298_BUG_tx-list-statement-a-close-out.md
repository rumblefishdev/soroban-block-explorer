---
id: '0298'
title: 'Statement A fix close-out: lower read_rows quota, canonical SQL, regression test'
type: BUG
status: backlog
related_adr: ['0032']
related_tasks: ['0290']
tags:
  [
    'clickhouse',
    'api',
    'quota',
    'docs',
    'test',
    'phase-launch',
    'priority-medium',
  ]
links: []
history:
  - date: 2026-06-17
    status: backlog
    who: fmazur
    note: 'Spawned from 0290 future work — the core Statement A two-step seek fix is live + verified on prod (35.7M→~1.0M/poll), but three follow-up items remained open at completion.'
---

# Statement A fix close-out: lower read_rows quota, canonical SQL, regression test

## Summary

The 0290 fix (two-step `accounts`/`ledgers` key-seek + `accounts.id` bloom
index) is **deployed and verified on prod** (polled `/transactions` reads
~1.0M rows/poll vs the old 35.7M). Three loose ends were deferred from 0290 and
are tracked here so they are not lost.

## Context

Parent: [[0290]]. The stopgap quota bump (`api_throttle.read_rows` 10B→**50B**)
was applied during the incident to keep the site up while the real fix landed.
Now that the per-poll read is ~35× lower, the 50B cap hides a problem that no
longer exists and must be brought back down to a real guard. The canonical SQL
doc and a regression guard were the remaining acceptance items.

## Implementation

- **Lower `api_throttle.read_rows` 50B → ~1–2B** in
  `crates/db-clickhouse/users.d/quotas.xml` (keep `errors` at 0 — justified
  independently). Deploy: ansible Hetzner `--tags app` + restart CH (single-file
  bind-mount inode trap — see 0290 Stopgap). Dry-run first.
- **Update canonical SQL**
  `docs/architecture/database-schema/endpoint-queries-clickhouse/02_get_transactions_list.sql`
  to reflect the two-step seek (no `accounts`/`ledgers` JOIN in Statement A) and
  the final quota caps. Per [ADR 0032](../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- **Regression test** bounding Statement A `read_rows` for a first-page request
  (assert ≪ partition size) — guards against a join/scan regression reappearing.

## Acceptance Criteria

- [ ] `api_throttle.read_rows` lowered to ~1–2B and live on prod (verified via
      `SELECT … FROM system.quotas`)
- [ ] `02_get_transactions_list.sql` matches the deployed two-step Statement A
- [ ] `quotas.xml` comment reflects the final caps + the fix that justified them
- [ ] Regression test asserting Statement A first-page `read_rows` is bounded
