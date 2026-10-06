---
title: 'Who reads accounts_recent, what the writes cost the disks, and a local projection spike'
type: research
status: mature
tags: [clickhouse, performance]
links:
  - 'https://www.scan.co.uk/products/192tb-samsung-pm983-25-u2-ssd-pcie-30-x4-nvme-mlc-3d-v-nand-3000mb-s-read-1900mb-s-write-540k-50k-io'
history:
  - date: '2026-10-06'
    status: mature
    who: karolkow
    note: 'Production read-only measurements plus a local spike on ClickHouse 26.3.'
---

# Usage, disk cost and a projection spike — 2026-10-06

Production figures are read-only (`system.query_log`, `part_log`,
`asynchronous_metric_log`, `columns`, sysfs on the host). The spike ran on a
local `clickhouse-server:26.3` container; production runs 26.3.10.60.

## Who reads it

`system.query_log`, 7 days to 2026-10-06:

| Reader                                                        | Queries                                |
| ------------------------------------------------------------- | -------------------------------------- |
| account list (`FROM accounts_recent a`), mainnet `api_reader` | 11 (all newest-first, 2 with a cursor) |
| account list, `testnet_reader`                                | 3                                      |
| `count() FROM accounts_recent` (header KPI), `api_reader`     | 16,558                                 |

The full-table rewrite serves about two list reads a day. The count is the
heavy reader; any redesign must keep a cheap source for it.

## What a rewrite consists of

`system.columns` for `default.accounts_recent`:

| Column              | Compressed |
| ------------------- | ---------- |
| `account_id`        | 803.96 MiB |
| `id`                | 114.83 MiB |
| `first_seen_ledger` | 37.77 MiB  |
| `last_seen_ledger`  | 22.88 MiB  |
| `home_domain`       | 2.29 MiB   |

`account_id` is 82 % of the table. The list needs it for one page (≤ 50 rows)
only, and `accounts` resolves `id → account_id` by its `idx_acc_id` bloom index.

## Writes, last 24 h

`part_log` `NewPart` by table: `default` MV inner table 695.33 GiB, `testnet`
MV inner table 192.01 GiB, everything else ~33 GiB. The testnet copy of the
MV (live since the testnet environment started) is new since the 2026-09-25
measurement.

Block-layer writes per NVMe device: 1.64 TiB/day (`BlockWriteBytes_nvme*`,
24 h average).

## What it costs the disks

The host has 2 × Samsung PM983 1.92 TB U.2 (`MZQLB1T9HAJR-00007`, sysfs),
RAID 1. Rated endurance 2,733 TBW (1.3 DWPD, vendor listings). sysfs counters
since the last boot (2026-05-15): 196.7 TB and 198.6 TB written, 7.2 % of the
rating. At the current 1.8 TB/day that is ~24 % of the rating per year;
~95 % of it is this MV (the database wrote 5-13 GiB/day before it landed).
Wear before 2026-05-15 is unknown: no SMART tool is installed on the host,
and the server is leased second-hand hardware. RAID 1 mirrors the writes, so
both drives wear at the same rate.

## Projection spike (local, synthetic)

Two `accounts`-shaped RMTs, identical data: 15M accounts plus 40 update
batches of 250k rows (a quarter of them on 20k hot accounts), 21.5M physical
rows. One table carries
`PROJECTION p_recent (SELECT last_seen_ledger, id, home_domain ORDER BY (last_seen_ledger, id))`
with `deduplicate_merge_projection_mode = 'rebuild'`.

| Metric             | Plain    | With projection               |
| ------------------ | -------- | ----------------------------- |
| `NewPart` bytes    | 1.66 GiB | 1.88 GiB                      |
| `MergeParts` bytes | 1.79 GiB | 2.02 GiB                      |
| Size on disk       | 1.58 GiB | 1.78 GiB (projection 212 MiB) |

Write amplification is ~13 % of `accounts`' own writes, ~0.7 GiB/day on
production (estimate from its 691 MiB inserts + 4.71 GiB merges per day).

Reads, 150 rows newest-first:

| Query                                   | Projection used | Rows read  |
| --------------------------------------- | --------------- | ---------- |
| no filter                               | no              | 21,539,766 |
| cursor only (`(ls, id) < …`)            | yes             | 21,539,766 |
| `last_seen_ledger >= max - 1000`        | yes             | 580,042    |
| `last_seen_ledger BETWEEN` a 100k range | yes             | 114,688    |

The projection prunes by range but is **not read in order**: a page without
a lower ledger bound reads the whole table. A reader would have to search a
ledger window and widen it until a page fills. Production density for sizing
that window: 6,913 row versions in the last 10 ledgers, 23,853 in the last
100, ~1.3 per ledger on average across history.
