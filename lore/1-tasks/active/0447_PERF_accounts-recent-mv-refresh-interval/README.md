---
id: '0447'
title: 'PERF: accounts_recent_mv rewrites a 950 MiB table every 2 minutes — 887 GiB/day with testnet'
type: PERF
status: active
related_adr: []
related_tasks: ['0385', '0403', '0627']
tags: [phase-current, effort-small, priority-high, performance, clickhouse, ops]
links: []
history:
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Measured during a cost investigation. 0385 shipped the MV and flagged its
      recompute as an unmeasured risk; 0403 owns that risk but asks about the
      memory cap, not the write volume. The write volume has now been measured
      and is 661 GiB/day.
  - date: '2026-09-25'
    status: backlog
    who: karolkow
    note: >
      Re-measured on production (read-only). Still true and slightly larger:
      690 GiB/day, 85-98 % of all new-part bytes on the server every day since
      2026-09-01. Measured its effect on a co-located backfill: no measurable
      throughput loss, a CPU burst every 2 minutes that triples tail latency.
      Raising the interval was rejected on review; a projection on `accounts`
      is the candidate direction, not decided. Deferred. See "Re-measured
      2026-09-25".
  - date: 2026-09-29
    status: backlog
    who: karolkow
    note: >-
      Absorbed 0395 (accounts_recent projection vs refreshable MV): same refresh cost, now one task. Read 0395 in archive/ for its projection option.
  - date: '2026-10-06'
    status: active
    who: karolkow
    note: >
      Activated. New measurements: 11 list reads a week against 887 GiB/day
      of writes (testnet doubles the MV), `account_id` is 82 % of the copy,
      the disks burn ~24 % of their rated endurance a year. A local spike
      showed the projection is not read in order. Decided: drop `account_id`
      from the copy now (this task); the projection moves to 0627.
---

# accounts_recent_mv rewrites the whole table every 2 minutes

## Summary

`accounts_recent_mv` is a refreshable materialized view that runs
`SELECT ... FROM accounts FINAL` **every 2 minutes** and rewrites its whole
target table. On 2026-10-06 that is **695 GiB/day on mainnet plus 192 GiB/day
on testnet**, ~96 % of all new-part bytes on the server, for a list read about
twice a day. The cost is NVMe endurance: ~24 % of the drives' rating per year.

Measurements: [notes/R-write-volume-2026-07-and-09.md](notes/R-write-volume-2026-07-and-09.md)
(volume, CPU, effect on a backfill),
[notes/R-usage-disk-and-projection-spike-2026-10-06.md](notes/R-usage-disk-and-projection-spike-2026-10-06.md)
(readers, column sizes, disk endurance, projection spike).

## Stan teraz

- Done: measurements and the decision below; projection spike (→ 0627).
- Next: PR — copy without `account_id`, list resolves it by `id`.
- In force: no `REFRESH EVERY` change (rejected 2026-09-25).

## Context

Task 0385 built the copy so the account-list browse (`GET /v1/accounts`,
sorted by `last_seen_ledger`) seeks an ordered table instead of paying
`accounts FINAL` per request. `accounts` is `ORDER BY account_id` because
`last_seen_ledger` mutates and cannot be in an RMT sort key. The header KPI
(`total_accounts`, `network/queries.rs`) also reads `count()` over the copy,
16.5k times a week.

## Decision — 2026-10-06

| Option                                                                         | Writes/day                     | Outcome                                                                                      |
| ------------------------------------------------------------------------------ | ------------------------------ | -------------------------------------------------------------------------------------------- |
| Raise `REFRESH EVERY`                                                          | 1/N of today                   | Rejected on review 2026-09-25: still a full rewrite, only spaced out                         |
| **Drop `account_id` from the copy**; the list resolves it for the page by `id` | ~160 GiB (estimate: 82 % less) | **This task**                                                                                |
| Projection on `accounts`, copy and MV removed                                  | ~0.7 GiB (spike)               | **0627** — not read in order, needs a widening ledger-window reader and its own count source |

## Implementation

- `init.sql`: `accounts_recent` and `accounts_recent_mv` without
  `account_id`.
- `accounts::fetch_list`: page `accounts_recent` without `account_id`, then
  one `SELECT id, account_id FROM accounts WHERE id IN (page ids)` seek on
  `idx_acc_id` — the same id → StrKey lookup the transaction lists use.
- Docs: `database-schema-overview.md` (`accounts_recent`).
- Production, after the API deploy, per database (`default`, `testnet`):
  `ALTER TABLE accounts_recent_mv MODIFY QUERY …`, then
  `ALTER TABLE accounts_recent DROP COLUMN account_id`. API first: the old
  API selects `account_id` from the copy.

## Acceptance Criteria

- [ ] `/v1/accounts` responses identical before and after (same fields, same
      values, same cursor)
- [ ] Rows read and latency of the list measured before and after
- [ ] `part_log` `NewPart` for the MV inner tables measured for a day after
      the ALTERs, on both databases
- [ ] Docs updated — `docs/architecture/database-schema/database-schema-overview.md`
- [ ] 0403's memory question either answered here or explicitly left with 0403
