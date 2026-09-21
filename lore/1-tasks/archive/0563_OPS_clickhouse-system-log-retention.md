---
id: '0563'
title: 'OPS: ClickHouse system logs keep 30 days — four log tables grow without limit (≥ 89 GiB reclaimable)'
type: OPS
status: completed
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
  - date: 2026-09-21
    status: backlog
    who: claude
    note: >
      Measured again for 0541's review: net growth per table, the text_log
      level (answers implementation step 5), where the log lines come from, and
      a consideration on query_log's TTL. No status change.
  - date: 2026-09-21
    status: backlog
    who: karolkow
    note: >
      Decided: 30 days on six log tables, text_log keeps information and
      above, the server file log goes from trace to debug, query_metric_log is
      left out. Tested on a local container of the production image; runbook
      recorded. Still backlog until promoted for the config PR.
  - date: 2026-09-21
    status: active
    who: karolkow
    note: >
      Promoted to active for runbook step 1: the config file, the compose bind
      mount and the README config list, in one PR. The production steps (2–5)
      follow its merge and stay with the operator.
  - date: 2026-09-21
    status: completed
    who: karolkow
    note: >
      Deployed. Dropped 22 expired monthly partitions, set the 30-day TTL on
      the six live tables (25 minutes of rewrites), then ran the Ansible app
      deploy from the merged commit 96f1a053; ClickHouse recreated at 10:47
      UTC. No _N table, six TTLs, text_log without Debug or Trace,
      logger.level debug. Free disk 345.46 → 478.57 GiB (+133.11). Three
      alarms paged once during the recreate and cleared by themselves; 0
      ledger gaps, DLQ empty. macOS openrsync failed the first dry run (fixed
      with Homebrew rsync). Archived.
---

# OPS: ClickHouse system logs keep 30 days

## Summary

> Decided 2026-09-21 — see "Decision" and "Runbook" below: six tables at 30
> days, `text_log` at `information`, the file log at `debug`; about 132 GiB back
> at once. Deployed the same day — see "Deployed": +133.11 GiB free.

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

## Measured again (production, 2026-09-20/21, read-only)

> Answers implementation step 5. Adds net growth per table, where the log
> lines come from, and a consideration on `query_log`'s TTL.

### Written is not kept

Bytes written (`system.part_log`, `NewPart`) overstate what a log table keeps:
merges compress wide rows hard. Kept per day = size on disk ÷ days of data held
(`system.parts`, active parts):

| table                     | size      | days held            | kept / day | TTL     |
| ------------------------- | --------- | -------------------- | ---------- | ------- |
| `text_log`                | 80.74 GiB | 70 (from 2026-07-14) | 1.15 GiB   | none    |
| `trace_log`               | 38.00 GiB | 126                  | 308.79 MiB | none    |
| `query_log`               | 24.62 GiB | 126                  | 200.05 MiB | none    |
| `part_log`                | 17.65 GiB | 126                  | 143.44 MiB | none    |
| `processors_profile_log`  | 14.18 GiB | 30                   | steady     | 30 days |
| `metric_log`              | 3.91 GiB  | 126                  | 31.78 MiB  | none    |
| `asynchronous_metric_log` | 2.94 GiB  | 126                  | 23.91 MiB  | none    |
| `query_metric_log`        | 1.10 GiB  | 124                  | 9.05 MiB   | none    |

Server logs keep ≈ 1.86 GiB a day. `metric_log` writes about 2.5 GiB a day and
keeps 32 MiB of it.

Disk, from `asynchronous_metric_log` (`DiskAvailable_default`, daily maximum):
380.49 GiB free on 2026-09-13, 351.58 GiB on 2026-09-20 — a net loss of
4.13 GiB a day, so the logs are about 45% of it. Free space swings about 8 GiB
within a day (37 GiB on backfill days), so a free-space gate should read the
daily minimum.

### `text_log` — the server logs at `trace`

`system.server_settings`: `logger.level = trace`, the value in ClickHouse's
shipped default config. `text_log` by level, 2026-09-18 to 09-20: Trace 58.0%,
Debug 41.9%, Information 0.1%, Warning 5,052 rows, Error 2,727 rows.

A `text_log` level of `information` keeps the useful 0.1% and removes nearly
all of the 1.15 GiB a day. Verify the config element name in the ClickHouse docs
before writing it (step 5).

### Where the log lines come from

`text_log` on 2026-09-20, attributed through `query_id` → `query_log.user`:

| source                                            | share |
| ------------------------------------------------- | ----- |
| indexer (`ingestion_writer`)                      | 29.6% |
| internal queries (empty user — refreshable views) | 28.5% |
| background, no query id (merges, view refreshes)  | 22.1% |
| co-located `prices` database (`prices_writer`)    | 19.6% |
| API (`api_reader`)                                | 0.2%  |
| operator reads (`dev_read`)                       | 0.1%  |

Queries 2026-09-14 to 09-20: indexer 72.7%, `prices_*` 20.9%, internal 5.7%,
API 0.4%, operator 0.3%. Manual reads are not a factor.

About half the lines come from refreshable views and merges.
`accounts_recent_mv` (task 0385, `REFRESH EVERY 2 MINUTE`, a full recompute
from `accounts FINAL`) writes 687.73 GiB of temporary parts a day (7-day
average), against 1.61 GiB for `balance_aggregates_mv`, which uses the same
pattern on an aggregate. Its storage is small (`accounts_recent` 972.55 MiB);
its write volume was not measured in 0385. Out of this task's scope — recorded
because it drives log volume.

Its refreshes in `query_log` on 2026-09-20 — internal
`` INSERT INTO default.`.tmp.inner_id.5b2fb432-…` ``; the text does not contain
the view's name, so a filter on `accounts_recent` finds nothing: 720 refreshes, 8.5 s
average, 18.8 s maximum, 4.43 CPU-hours, 18.30 billion rows read, 895.58 GiB
written, 1.23 GiB peak memory — in one day.

### `trace_log` composition

2026-09-20: Memory 40.3%, MemoryPeak 36.8%, Real 20.8%, CPU 2.0%. All defaults:
`memory_profiler_step` 4 MiB, both query profiler periods 1 s.

### Consideration for the TTL decision — `query_log`

> Not taken — `query_log` stays at 30 days (see "Decision").

The plan gives every table 30 days. `query_log` is the evidence base for
questions like 0541's consumer sweep: a sweep of its full retention (from
2026-05-19) found one client outside this repository reading `soroban_events`,
active in July and on two days in September. With 30 days the July activity
would not have been visible. A longer TTL for `query_log` alone (90–180 days)
costs about 200 MiB a day. `part_log` served the per-table write measurement
above; 30–60 days covers that use.

## Decision (karolkow, 2026-09-21)

| setting                          | value                                                                                                                       |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| TTL                              | `event_date + INTERVAL 30 DAY` on `text_log`, `trace_log`, `query_log`, `part_log`, `metric_log`, `asynchronous_metric_log` |
| `processors_profile_log`         | unchanged — already 30 days from the shipped config                                                                         |
| `query_metric_log`               | left without a TTL (below)                                                                                                  |
| `text_log` level                 | `information` — keeps Fatal, Critical, Error, Warning, Notice, Information                                                  |
| server file log (`logger.level`) | `trace` → `debug`                                                                                                           |

Steady state ≈ 34 GiB of server logs (row-proportional estimate from each
table's last 30 days), against 183 GiB today and ~770 GiB a year from now if
nothing changes (+1.6 GiB a day, measured over the last 30 days).

Considered and not taken:

- **Per-table retention** (`query_log` 90–180 days, `trace_log` 14, …): more
  rules for a larger result (~55 GiB). The risk that argued for a long
  `query_log` — the outside client found in 0541 ran on 2026-07-17, 09-14 and
  09-18, a 59-day gap — is covered by the pre-change sweep plus asking the
  co-located project's owner, not by retention.
- **A conditional TTL on `text_log`** (7 days for Trace/Debug, 90 for the rest):
  accepted by the config and correct on a local test, but unnecessary once the
  table keeps `information` and above.
- **Dropping `query_log`'s start rows** — see "Redundancy inside tables".

### Log levels

Cumulative: a level keeps itself and everything more severe, so `information`
keeps 1–6.

| #   | level       | `text_log` rows, 70 days | example on production                                                                 |
| --- | ----------- | ------------------------ | ------------------------------------------------------------------------------------- |
| 1   | Fatal       | 0                        | —                                                                                     |
| 2   | Critical    | 0                        | —                                                                                     |
| 3   | Error       | 178,630                  | `Error loading config from users.xml`; `Cancelled merging parts` (~690 a day, benign) |
| 4   | Warning     | 8,102                    | `Cannot read remaining request body during exception handling`                        |
| 5   | Notice      | 0                        | —                                                                                     |
| 6   | Information | 1,641,306                | `Ready for connections`; `Have 16 tables in drop queue`                               |
| 7   | Debug       | 814,683,022              | `Selected 1/50 parts … 379/55210 marks by primary key`; `Loading config users.xml`    |
| 8   | Trace       | 863,831,911              | `Renaming temporary part tmp_insert_… to …`                                           |
| 9   | Test        | 0                        | —                                                                                     |

### Why `text_log` can drop Debug and Trace

What those lines say is kept, structured, elsewhere — each checked on
production on 2026-09-21:

| information             | structured source                                                                                                                                                                                                      |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| index pruning per query | `query_log` `ProfileEvents` `SelectedParts`, `SelectedPartsTotal`, `SelectedMarks`, `SelectedMarksTotal`, `SelectedRanges` — for one query, the same numbers as its Debug line (1/50 parts, 379/55,210 marks, 1 range) |
| duration, rows, memory  | `query_log`                                                                                                                                                                                                            |
| query errors            | `query_log` exception with stack trace; `error_log` (2.1 M entries since May, 66 codes)                                                                                                                                |
| merge backlog           | `asynchronous_metric_log` `MaxPartCountForPartition`, as a time series                                                                                                                                                 |
| merges                  | `part_log`                                                                                                                                                                                                             |
| view refreshes          | `query_log` — internal `` INSERT INTO default.`.tmp.inner_id.<uuid>` ``, 720 a day per view                                                                                                                            |

`text_log` was read 16 times in 70 days, `trace_log` once (`query_log`, every
user).

The one real cost found: **a successful config reload logs only at Debug.** On
2026-09-17 `ConfigReloader` wrote 50 Error lines, 53 Debug lines and no
Information line. After the change, confirm a reload from the file log or from
the loaded state (`system.users`, `system.quotas`, `system.server_settings`).

### Why the file log goes to `debug`

The file (`/srv/clickhouse-logs/clickhouse-server.log`) is bounded by rotation —
`logger.size` 1000M × `logger.count` 10 ≈ 10 GB — whatever the level; the level
only sets how many days fit. At `trace` that is about 1.5 days (estimate:
`text_log` holds 445.86 GiB uncompressed over 70 days at 5.5× compression, so
~6.4 GiB of text a day). `debug` drops Trace, about half the lines, so about
three days fit (estimate). After the change this file is the only place Debug
lines exist; lowering it further would free at most the ~10 GB it already
occupies, once.

### Why `query_metric_log` is left out

It is the only log table that re-checks its definition against the config
**while running**, not only at startup. On the local test its `ALTER … MODIFY
TTL` was followed within half a second by a rename to `query_metric_log_0` —
before the config was deployed — and the restart renamed it again. The other six
did neither. Including it would need a config-only change plus a `DROP` of its
`_0` copy, and would turn the verification rule into "any `_N` table is a
failure, except this one". It holds 1.10 GiB, adds about 9 MiB a day, and nothing
has read it in four months.

### Where the logs live on the host (from this repository)

| log                             | location                                         | bound                        | duplicate?                    |
| ------------------------------- | ------------------------------------------------ | ---------------------------- | ----------------------------- |
| `system.text_log`               | table                                            | none today; 30 days after    | yes — of the log file         |
| server log file                 | `/srv/clickhouse-logs/clickhouse-server.log`     | 10 × 1000M                   | yes — of `text_log`           |
| server error file               | `/srv/clickhouse-logs/clickhouse-server.err.log` | 10 × 1000M                   | a subset of the log file      |
| other log tables                | `system` database                                | none today; 30 days after    | no file copy                  |
| ClickHouse container stdout     | Docker `json-file`                               | 5 × 100m                     | no — 5 startup lines (tested) |
| Caddy access log                | Docker `json-file` (`output stdout`)             | 5 × 100m                     | partly overlaps `query_log`   |
| backup job                      | `/var/log/ch-backup.log`                         | logrotate, 26 weeks          | no                            |
| system journal                  | `/var/log/journal`                               | systemd default (unverified) | —                             |
| co-located project's containers | not in this repository                           | unverified                   | —                             |

The weekly backup freezes only the `default` database (`ch-backup.sh.j2`:
`readonly DB="default"`), so no server log is in the Borg archive. No log
shipper runs — Loki is only mentioned as a future option in the `Caddyfile`.

Host-side sizes are not measured yet (operator, read-only, on the host):
`sudo du -sh /srv/clickhouse-logs /var/log /var/lib/docker/containers`,
`journalctl --disk-usage`,
`sudo find /var/lib/docker/containers -name '*-json.log*' -size +10M -exec ls -lh {} +`,
`docker ps`.

### Who reads the log tables

`query_log` over its whole retention (from 2026-05-19): besides manual reads,
three query shapes by `default` over the native client — `query_log` by
`log_comment`, `part_log` over the last hours, `text_log` with `level <= 3` over
the last 60 minutes. None needs more than 30 days or a Debug line. `text_log` was
also truncated by hand on 2026-07-14, which is why it starts on that date.

### Redundancy inside tables — noted, not acted on

- `query_log` stores every query twice: `QueryStart` and `QueryFinish`,
  3,840,442 and 3,840,370 in seven days. The 72-row difference equals the 72
  `ExceptionWhileProcessing` rows, so that week the start rows added nothing.
  `log_queries_min_type = 'QUERY_FINISH'` would save ~1.5–2 GiB at 30 days
  (estimate). Not taken: it is a user-profile setting, in the config area that
  produced 50 reload errors on 2026-09-17.
- `part_log` writes a start and an end row per merge (1.43 M each a week) and a
  `RemovePart` row per removed part (43% of its rows). No setting trims it; it is
  ~4.6 GiB at 30 days.
- If space runs short later, the next levers are `processors_profile_log`
  (~14 GiB, read on one day in four months) and the `query_log` start rows.

### Tested on a local container (2026-09-21, image 26.3.10.60 = production)

| check                                                                 | result                                                                                                       |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| stock `text_log` definition                                           | identical to production, character for character                                                             |
| TTL in the config only                                                | old table renamed to `text_log_0` with all its data — the trap below is real                                 |
| ALTER first, then the same TTL in the config, then restart            | six tables kept, rows preserved, no rename                                                                   |
| `query_metric_log`                                                    | renamed twice — right after its ALTER, and again at restart                                                  |
| `ALTER … MODIFY TTL`                                                  | rewrites every part, including parts with nothing expired (`materialize_ttl_after_modify = 1` on production) |
| conditional TTL (`… DELETE WHERE level IN ('Trace', 'Debug'), …`)     | accepted by the config; deleted exactly the intended rows                                                    |
| final config (`logger.level` debug, `text_log` information, six TTLs) | file: 0 Trace, 304 Debug; table: 0 Trace, 0 Debug, 60 Information, 9 Warning                                 |
| rename message                                                        | logged at `Debug` — invisible in `text_log` after the change; verify by the absence of `_N` tables           |
| production `ttl_only_drop_parts`                                      | 0 — row-level TTL deletes work                                                                               |
| production `max_partition_size_to_drop`                               | 46.57 GiB; the largest partition to drop is 30.97 GiB                                                        |

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

## Runbook (2026-09-21)

Every step that changes production is the operator's (`chw`, Ansible);
read-only checks are `chq`.

**0. Before (read-only).** Record free space; confirm no renamed log table
exists yet (the second query must return nothing).

```bash
chq "SELECT formatReadableSize(free_space) FROM system.disks WHERE name='default'"
chq "SELECT name FROM system.tables WHERE database='system' AND match(name, '_[0-9]+\$')"
```

**1. Merge the config PR — do not deploy it yet.** New
`crates/db-clickhouse/config.d/system-logs.xml`, the bind-mount line in
`docker-compose.prod.yml`, and the config list in `infra-hetzner/README.md`.
Merged first so that steps 3 and 4 can run back to back.

```xml
<clickhouse>
    <logger>
        <level>debug</level>
    </logger>
    <text_log>
        <level>information</level>
        <ttl>event_date + INTERVAL 30 DAY</ttl>
    </text_log>
    <trace_log><ttl>event_date + INTERVAL 30 DAY</ttl></trace_log>
    <query_log><ttl>event_date + INTERVAL 30 DAY</ttl></query_log>
    <part_log><ttl>event_date + INTERVAL 30 DAY</ttl></part_log>
    <metric_log><ttl>event_date + INTERVAL 30 DAY</ttl></metric_log>
    <asynchronous_metric_log><ttl>event_date + INTERVAL 30 DAY</ttl></asynchronous_metric_log>
</clickhouse>
```

**2. Drop whole months older than 30 days** — instant, rewrites nothing. With
the cutoff at 2026-08-22, August goes too: its last ten days would expire within
ten days anyway, and dropping it spares step 3 about 49 GiB of rewriting. If
the runbook is executed later, recompute which months are fully expired.

```bash
chw "ALTER TABLE system.text_log DROP PARTITION 202607"
chw "ALTER TABLE system.text_log DROP PARTITION 202608"
chw "ALTER TABLE system.trace_log DROP PARTITION 202605"
chw "ALTER TABLE system.trace_log DROP PARTITION 202606"
chw "ALTER TABLE system.trace_log DROP PARTITION 202607"
chw "ALTER TABLE system.trace_log DROP PARTITION 202608"
chw "ALTER TABLE system.query_log DROP PARTITION 202605"
chw "ALTER TABLE system.query_log DROP PARTITION 202606"
chw "ALTER TABLE system.query_log DROP PARTITION 202607"
chw "ALTER TABLE system.query_log DROP PARTITION 202608"
chw "ALTER TABLE system.part_log DROP PARTITION 202605"
chw "ALTER TABLE system.part_log DROP PARTITION 202606"
chw "ALTER TABLE system.part_log DROP PARTITION 202607"
chw "ALTER TABLE system.part_log DROP PARTITION 202608"
chw "ALTER TABLE system.metric_log DROP PARTITION 202605"
chw "ALTER TABLE system.metric_log DROP PARTITION 202606"
chw "ALTER TABLE system.metric_log DROP PARTITION 202607"
chw "ALTER TABLE system.metric_log DROP PARTITION 202608"
chw "ALTER TABLE system.asynchronous_metric_log DROP PARTITION 202605"
chw "ALTER TABLE system.asynchronous_metric_log DROP PARTITION 202606"
chw "ALTER TABLE system.asynchronous_metric_log DROP PARTITION 202607"
chw "ALTER TABLE system.asynchronous_metric_log DROP PARTITION 202608"
```

Frees about 132 GiB (measured 2026-09-21: `text_log` 59.15, `trace_log` 32.50,
`query_log` 20.87, `part_log` 14.34, `metric_log` 3.23,
`asynchronous_metric_log` 2.33).

**3. TTL on the six live tables — outside peak hours.** Each statement launches a
mutation that rewrites every remaining part — after step 2, September only,
about 35 GiB compressed.

```bash
chw "ALTER TABLE system.text_log MODIFY TTL event_date + INTERVAL 30 DAY"
chw "ALTER TABLE system.trace_log MODIFY TTL event_date + INTERVAL 30 DAY"
chw "ALTER TABLE system.query_log MODIFY TTL event_date + INTERVAL 30 DAY"
chw "ALTER TABLE system.part_log MODIFY TTL event_date + INTERVAL 30 DAY"
chw "ALTER TABLE system.metric_log MODIFY TTL event_date + INTERVAL 30 DAY"
chw "ALTER TABLE system.asynchronous_metric_log MODIFY TTL event_date + INTERVAL 30 DAY"
```

Wait until this returns nothing:

```bash
chq "SELECT table, parts_to_do FROM system.mutations WHERE database='system' AND NOT is_done"
```

Each `ALTER` returns only after its own table's rewrite (85–510 s each, 25
minutes for all six on 2026-09-21), so keep the terminal open: interrupting the
loop leaves the remaining tables without the TTL.

**4. Deploy the config immediately after step 3.** Ansible sync, then **recreate**
the ClickHouse container (a new bind-mounted file needs a recreate, not a
restart — task 0314). Keep the gap after step 3 short. Brief API and indexer
errors during the recreate; the indexer queue holds the ledgers. The operator's
laptop needs rsync 3.x (on macOS, `brew install rsync` — the system's openrsync
rejects the playbook's `--chmod`). Expect one page each from
`indexer-ch-write-failures`, `ingestion-backlog-age` and the co-located
project's error alarm; on 2026-09-21 all three cleared by themselves within 5–7
minutes.

**5. Verify (read-only).**

```bash
chq "SELECT name FROM system.tables WHERE database='system' AND match(name, '_[0-9]+\$')"
chq "SELECT name, extract(engine_full, 'TTL [^S]*') FROM system.tables WHERE database='system' AND name IN ('text_log','trace_log','query_log','part_log','metric_log','asynchronous_metric_log')"
chq "SELECT level, count() FROM system.text_log WHERE event_time > now() - INTERVAL 1 HOUR GROUP BY level"
chq "SELECT value FROM system.server_settings WHERE name='logger.level'"
chq "SELECT formatReadableSize(free_space) FROM system.disks WHERE name='default'"
```

Expected: no `_N` table (do not rely on the rename message — it is logged at
`Debug` and will not reach `text_log`); six tables with `TTL event_date +
toIntervalDay(30)`; no Trace or Debug rows in `text_log`; `logger.level` is
`debug`; about 132 GiB more free space than step 0, more once step 3's mutations
finish.

## Deployed — 2026-09-21

Operator `karolkow`, from the task worktree at `96f1a053` (the merged
`develop`), clean tree. Every check below is read-only.

| step     | UTC                 | result                                                                                                                                                                                                                                               |
| -------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0 before | 09:40               | free 345.46 GiB; no `_N` table; no running system mutation; the 22 partitions present, 132.38 GiB                                                                                                                                                    |
| 2 drop   | 09:43:35 – 09:43:53 | 22 × `DROP PARTITION`, each `QueryFinish` without exception; no part older than September left                                                                                                                                                       |
| 3 TTL    | 09:44:07 – 10:09:29 | six `MODIFY TTL` without exception, each returning after its rewrite: `text_log` 266 s, `trace_log` 510 s, `query_log` 398 s, `part_log` 98 s, `metric_log` 164 s, `asynchronous_metric_log` 85 s; six mutations done, no fail reason                |
| 4 deploy | 10:47:31 (up)       | `--tags app`; content changed: `system-logs.xml` (new), `docker-compose.prod.yml`, `infra-hetzner/README.md`, `ansible/roles/app/tasks/main.yml`, and `schema/init.sql` (see Issues); `.env`, client credentials and the Caddy map `ok`; all healthy |
| 5 verify | 10:50               | below                                                                                                                                                                                                                                                |

After the recreate:

- No `_N` table. The six tables show `TTL event_date + toIntervalDay(30)`;
  `processors_profile_log` keeps its own 30 days, `query_metric_log` none.
- `text_log` since the recreate (first 2.5 minutes): Information 209, Warning 9
  (startup: `[::]` listen, delay accounting, schedule pool), Error 2
  (`Cancelled merging parts`); no Debug or Trace.
- `logger.level` = `debug` in `system.server_settings`.
- The six tables: 167.86 GiB (2026-09-20) → 35.84 GiB.
- Free disk: 345.46 GiB → 469.14 GiB after step 3 → 478.57 GiB after the
  recreate, **+133.11 GiB**.
- Schema sidecar: 40 statements as `default`, 0 errors.
- Ingestion: 0 gaps in `ledgers` over three hours (the query in
  `docs/runbooks/health.md`), lag 2–3 s by 10:50, ingest DLQ empty. API reads
  (`api_reader`) resumed at 10:50:53 without errors.
- `DEPLOYED_INFO` now records `96f1a053`.

Alarms — each paged once and cleared without action:

| alarm                                  | ALARM    | OK       |
| -------------------------------------- | -------- | -------- |
| co-located project's error alarm       | 10:48:05 | 10:53:05 |
| `production-indexer-ch-write-failures` | 10:48:49 | 10:53:49 |
| `production-ingestion-backlog-age`     | 10:54:08 | 11:01:08 |

Cause: two indexer reconciles, at 10:47:24 and 10:47:30, got `502 Bad Gateway`
from Caddy while ClickHouse restarted (`reconcile failed — will redeliver
doorbell`). SQS hides a failed doorbell for the visibility timeout — 660 s, the
indexer's 600 s timeout plus 60 — so the oldest message's age climbed to 604 s
and fell to 0 at 10:59 on redelivery. Later doorbells had already written every
ledger.

## Issues Encountered

- **macOS openrsync fails the playbook.** `/usr/bin/rsync` on macOS 26 is
  openrsync ("rsync version 2.6.9 compatible"). The first dry run stopped at the
  first sync with `rsync: --chmod=D755,F644: invalid argument`, before touching
  the box (`changed=0`). Fix: `brew install rsync` (3.5.0), which precedes
  `/usr/bin` on `PATH`.
- **The box was not at its `DEPLOYED_INFO` commit.** It recorded `b63d2782`, the
  last Ansible deploy (2026-08-21), but `users.d/quotas.xml` already matched
  `develop`: task 0561 overwrote that one file in place, without Ansible. The dry
  run's rsync list showed the real difference: `schema/init.sql` changed in size
  (the box held the 2026-08-21 version), every other file only in mtime and
  owner. The sidecar runs `init.sql` on every `up`; all 40 of its
  `CREATE … IF NOT EXISTS` objects already existed on production (checked before
  the deploy), so it created nothing.
- **The dry run's health wait always fails.** Under `--check` the handler's
  `docker inspect` is skipped (`Command would have run if not in check mode`),
  so it retries 24 times and reports `failed`. Not a problem signal.
- **`MODIFY TTL` blocks the client until its rewrite ends.** The runbook assumed
  background mutations; in fact the six-statement loop held the operator's
  terminal for 25 minutes. Recorded in step 3.
- **38 minutes between step 3 and the recreate** (10:09 → 10:47), spent on the
  rsync fix. Nothing restarted ClickHouse in between, so nothing was renamed.

## Implementation

> Superseded by the Runbook above (2026-09-21): six tables instead of seven,
> whole old months dropped first, the `text_log` level and the file log level
> added.

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

- [x] Six tables (`text_log`, `trace_log`, `query_log`, `part_log`,
      `metric_log`, `asynchronous_metric_log`) show
      `TTL event_date + toIntervalDay(30)` in production (2026-09-21, before
      and after the recreate)
- [x] Old months dropped; free disk rose by about 132 GiB — 345.46 → 478.57
      GiB, +133.11
- [x] `config.d/system-logs.xml` merged (PR #466) and deployed — six TTLs,
      `text_log` level `information`, `logger.level` `debug`; no `_N` system
      log table exists after the recreate
- [x] `text_log` holds no Trace or Debug rows written after the recreate; the
      file log holds no Trace lines — `logger.level` = `debug` as loaded by the
      server (`system.server_settings`); the file itself was not read, there
      is no host access from the checking session
- [x] `text_log` per-day volume measured; level decision recorded (2026-09-21)
- [x] **Docs updated** — `infra-hetzner/README.md` (directory map, and the
      single-file mount count 10 → 11); `docs/architecture/infrastructure/infrastructure-overview.md`
      §8.1 (server log retention, and that older server history exists nowhere)

## Implementation notes (config PR, 2026-09-21)

Runbook step 1. Nothing here touches production; steps 2–5 follow the merge.

| file                                                          | change                                                                                                              |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `crates/db-clickhouse/config.d/system-logs.xml`               | new — the six TTLs, `text_log` at `information`, `logger.level` `debug`; the header states the order rule           |
| `docker-compose.prod.yml`                                     | production-only bind mount, and the file in the overlay's header list                                               |
| `infra-hetzner/README.md`                                     | directory map; single-file mounts into `app-clickhouse-1` 10 → 11 (13 with Caddy's two)                             |
| `infra-hetzner/ansible/roles/app/tasks/main.yml`              | comment only (the same count). The sync already copies all of `config.d/`, so the file ships with no Ansible change |
| `docs/architecture/infrastructure/infrastructure-overview.md` | §8.1 — retention of the server log tables                                                                           |

Verified:

- The committed file on a local `26.3.10.60` container (the production image):
  `Merging configuration file '/etc/clickhouse-server/config.d/system-logs.xml'`;
  `logger.level` = `debug`; the six tables carry `TTL event_date +
toIntervalDay(30)`, `processors_profile_log` its own 30 days,
  `query_metric_log` none; `text_log` holds only Information and Warning rows.
- `docker compose -f docker-compose.yml -f docker-compose.prod.yml config`
  renders the mount, read-only, into the ClickHouse service.
- Mount count checked against the compose file itself: 10 on `develop`, 11 here.

### Design decisions — emerged

1. **Production-only, not in the dev compose.** The existing system tables of a
   dev container carry no TTL, so mounting the file there would rename them to
   `_0` on the next start — harmless, but noise, and dev log volume is not the
   problem. Same placement as `memory.xml` and `prometheus.xml`.
2. **The order rule lives in the file's header**, not only in this task. The
   file is where the next editor of a system log setting will look, and the
   trap (a config TTL alone renames the table and frees nothing) is invisible
   from the XML itself.
3. **Deployed from the task worktree**, not from the operator's main checkout.
   The playbook syncs the checkout it runs from; the main checkout was on an
   unrelated feature branch without `system-logs.xml`, and deploying that after
   step 3 would have triggered the rename trap.
