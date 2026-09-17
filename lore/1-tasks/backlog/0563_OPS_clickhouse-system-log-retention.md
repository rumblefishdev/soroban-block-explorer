---
id: '0563'
title: 'OPS: ClickHouse system logs keep 30 days — four log tables grow without limit (≥ 89 GiB reclaimable)'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0541', '0538', '0455']
tags: ['clickhouse', 'hetzner', 'disk', 'ops', 'effort-small', 'priority-high']
links:
  - crates/db-clickhouse/config.d
  - docker-compose.prod.yml
  - infra-hetzner/ansible/roles/app/tasks/main.yml
history:
  - date: 2026-09-17
    status: backlog
    who: karolkow
    note: >
      Filed from 0541's disk check before the soroban_events rebuild: free space
      at 20.5%, and the system database holds 174.50 GiB of server logs, four
      tables without any retention. Decided to reclaim this first (0541
      decision 231 A). Answers 0538 open question 5, which saw 32 GiB in
      September's first measurement.
  - date: 2026-09-17
    status: backlog
    who: karolkow
    note: >
      Deferred, not dropped: no server log is deleted now. The measurements,
      the rename trap and the two execution variants (drop the months older
      than 30 days first, or a single MODIFY TTL that rewrites every part) stay
      here for a later decision. 0541 proceeds without the reclaimed space.
---

# OPS: ClickHouse system logs keep 30 days

## Summary

`text_log`, `trace_log`, `query_log` and `part_log` (and three small metric
logs) have no TTL: they hold everything since May. Give every server log a
30-day TTL, on the live tables and in the server config, so at least 89 GiB
comes back now and the logs stop growing. Blocks 0541 phase 3 (the
`soroban_events` rebuild needs a second copy of the table on disk for weeks).

## Measured (production, 2026-09-17, read-only)

Disk: 360.71 GiB free of 1.72 TiB (20.5%); `default` 1.02 TiB, `system`
174.50 GiB, `prices` 63.13 GiB.

| table                     | total     | parts older than 30 days | TTL today |
| ------------------------- | --------- | ------------------------ | --------- |
| `text_log`                | 75.95 GiB | 28.18 GiB                | none      |
| `trace_log`               | 36.95 GiB | 27.94 GiB                | none      |
| `query_log`               | 23.78 GiB | 18.40 GiB                | none      |
| `part_log`                | 17.04 GiB | 9.73 GiB                 | none      |
| `processors_profile_log`  | 12.97 GiB | 0                        | 30 days   |
| `metric_log`              | 3.79 GiB  | 2.54 GiB                 | none      |
| `asynchronous_metric_log` | 2.83 GiB  | 1.49 GiB                 | none      |
| `query_metric_log`        | 1.09 GiB  | 0.99 GiB                 | none      |

Whole monthly parts older than 30 days: 89.27 GiB. The TTL also trims the
rows of August's parts older than 30 days, so the real gain is higher
(estimate). `text_log` alone adds about 1 GiB a day (47.78 GiB since
mid-August).

## The trap — a config TTL alone frees nothing

ClickHouse compares an existing system log table's `CREATE` with the one its
config produces. On any difference it **renames the old table to
`<name>_0` and creates a new, empty one** (`SystemLog::prepareTable`,
`src/Interpreters/SystemLog.cpp`, present in the production build
`v26.3.10.60-lts`: "Existing table … has obsolete or different structure.
Renaming it to …"). The renamed table keeps all its data, so shipping only a
config TTL would leave the 174 GiB on disk, split across `_0` tables.

The same comparison bites the other way: a TTL set only by `ALTER` is undone at
the next restart (the config still says "no TTL", so the table is renamed and a
TTL-less one created).

## Implementation

Order matters: live tables first, config second, so the restart finds a table
that already matches the config.

1. **Operator — TTL on the live tables** (frees space within minutes; parts
   whose every row is expired are dropped whole, August's parts are rewritten
   by a background merge):

   ```sql
   ALTER TABLE system.text_log MODIFY TTL event_date + INTERVAL 30 DAY
   ALTER TABLE system.trace_log MODIFY TTL event_date + INTERVAL 30 DAY
   ALTER TABLE system.query_log MODIFY TTL event_date + INTERVAL 30 DAY
   ALTER TABLE system.part_log MODIFY TTL event_date + INTERVAL 30 DAY
   ALTER TABLE system.metric_log MODIFY TTL event_date + INTERVAL 30 DAY
   ALTER TABLE system.asynchronous_metric_log MODIFY TTL event_date + INTERVAL 30 DAY
   ALTER TABLE system.query_metric_log MODIFY TTL event_date + INTERVAL 30 DAY
   ```

   Check (read-only): `SHOW CREATE TABLE system.text_log` carries
   `TTL event_date + toIntervalDay(30)`, the same form as
   `processors_profile_log` already has; free space rises by ≥ 89 GiB.

2. **Repo — the same TTL in the server config** (PR): new
   `crates/db-clickhouse/config.d/system-logs.xml` with, per table,
   `<text_log><ttl>event_date + INTERVAL 30 DAY</ttl></text_log>` (and the six
   others); bind-mount line in `docker-compose.prod.yml` next to the other
   `config.d` files; `infra-hetzner/README.md` config list. Before writing,
   compare the config-generated `CREATE` with step 1's result on a local
   container (`docker compose` with the same image tag), so no rename happens
   on production.

3. **Operator — deploy**: Ansible sync (`config.d` is already in the synced
   list) and recreate the ClickHouse container (a new bind-mounted file needs
   a recreate, not a restart — task 0314). Brief API/indexer errors during the
   recreate; the indexer queue holds ledgers.

4. **Verify (read-only)**: `SELECT name FROM system.tables WHERE database =
'system' AND name LIKE '%\_0'` is empty. If a rename happened anyway, the
   `_0` table already carries the 30-day TTL from step 1 and empties itself
   within 30 days; it can also be dropped at once.

5. **`text_log` volume** — find which loggers and levels fill it (`SELECT
level, logger_name, count() … GROUP BY …` over one day). If most rows are
   below `information`, propose a level for `text_log` in the same config file
   (verify the element name in the ClickHouse docs first).

## Acceptance Criteria

- [ ] All seven tables show `TTL event_date + toIntervalDay(30)` in production
- [ ] Free disk rose by ≥ 89 GiB (record before/after)
- [ ] `config.d/system-logs.xml` merged and deployed; no `_0` system log table
      exists after the recreate (or it carries the TTL)
- [ ] `text_log` per-day volume measured; level decision recorded
- [ ] **Docs updated** — `infra-hetzner/README.md` (config list);
      `docs/architecture/infrastructure/**` if it lists server config
      (N/A otherwise, reason recorded)
