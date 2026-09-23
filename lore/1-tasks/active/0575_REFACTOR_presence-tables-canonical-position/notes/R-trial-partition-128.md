---
prefix: R
title: Trial partition 128 — B/row per column and codec, measured
status: mature
---

# R — Trial partition 128 (2026-09-23)

**Baseline, partition 128 on production (2026-09-23, merged parts).**

| table                         | column              | B/row |
| ----------------------------- | ------------------- | ----- |
| `transaction_participants`    | `transaction_id`    | 8.03  |
| `transaction_participants`    | `ledger_sequence`   | 2.96  |
| `transaction_participants`    | `account_id`        | 0.21  |
| `operation_asset_appearances` | `transaction_id`    | 8.03  |
| `operation_asset_appearances` | `ledger_sequence`   | 1.01  |
| `contract_transactions` (ref) | `application_order` | 0.95  |
| `contract_transactions` (ref) | `ledger_sequence`   | 1.03  |

The reference position column costs 0.95 B/row on a merged partition — the
1.31 whole-table figure was the upper bound it was said to be.

**Export path measured.** One 10k-ledger slice of partition 128 joined to
`transactions` on production (`LEFT JOIN` on `(id, ledger_sequence)`) streams
~6.7 M rows / 120 MB in 2.4 s — the whole partition in ~2 minutes.

**First local attempt failed — the laptop disk, not the method.** Streaming
the partition into the local docker ClickHouse (10 columns: the target shape
plus six codec variants of `ledger_sequence` / `application_order`) filled the
laptop's 11 GiB free space in ~2 minutes and stopped Docker Desktop. Nothing
on production was touched. The trial needs either a production scratch table
or a much smaller local load.

Salvaged from the failed load before dropping it: 326,826,034 rows over 47
of the 50 slices, **0 rows without an `application_order`** — every
`transaction_participants` row of those slices found its transaction by
`(id, ledger_sequence)`. The coverage gate of step 4 should hold; the size
figures were not usable (unmerged, and the load stopped mid-partition).

## Trial result — partition 128, both tables (2026-09-23)

Rebuilt locally from production reads: each 10k-ledger slice joined to
`transactions` on production, streamed into a docker ClickHouse 26.3, all
variants as extra columns of one table (same rows, same sort). **Row counts
equal to production exactly** — `transaction_participants` 350,505,524,
`operation_asset_appearances` 289,434,129 — and **0 rows without a
position**. Local `ledger_sequence` under the default codec costs what
production pays (2.93 vs 2.96, 1.011 vs 1.013 B/row), so the method is
faithful. Parts: 8 and 13 after the load (production has 10 and 6);
`OPTIMIZE FINAL` to 1 part changed no column by more than 0.1 B/row.

B/row (before the merge, comparable to production's parts):

| column / codec                                  | `transaction_participants` | `operation_asset_appearances` |
| ----------------------------------------------- | -------------------------- | ----------------------------- |
| `transaction_id` today (production)             | 8.03                       | 8.03                          |
| `application_order` Int16, default (LZ4)        | 1.58                       | 1.39                          |
| `application_order` `ZSTD(1)`                   | 1.12                       | 0.98                          |
| `application_order` `T64, ZSTD(1)`              | **0.97**                   | **0.91**                      |
| `ledger_sequence` Int64, default (LZ4)          | 2.93                       | 1.01                          |
| `ledger_sequence` UInt32, default               | 2.79                       | 0.85                          |
| `ledger_sequence` `ZSTD(1)`                     | 1.06                       | 0.28                          |
| `ledger_sequence` `DoubleDelta, ZSTD(1)`        | 0.78                       | 0.14                          |
| `ledger_sequence` `Delta, ZSTD(1)`              | **0.77**                   | 0.17                          |
| leading id (`account_id` / `asset_id`), default | 0.21                       | 0.04                          |

Row totals:

| shape                                | `transaction_participants` | `operation_asset_appearances` |
| ------------------------------------ | -------------------------- | ----------------------------- |
| today (production)                   | 11.19                      | 9.09                          |
| position, default codecs             | 4.72 (−58%)                | 2.44 (−73%)                   |
| position + `Delta,ZSTD` / `T64,ZSTD` | **1.95 (−83%)**            | **1.12 (−88%)**               |

Whole-table projection (_estimate_, one partition × all rows; production
column data today 111.96 and 100.72 GiB): position with default codecs
−64 / −74 GiB; with the two codecs **−92 / −88 GiB, ~180 GiB together**.
`Delta` and `DoubleDelta` are within 0.03 B/row of each other on both tables;
`Delta` taken for both, one convention.

Space used on the laptop: ~14 GiB of host disk for a 4 GiB table (Docker's
disk image grows with inserts and merges); the load script stopped below
15 GiB free. Local trial database, container and volume removed afterwards.

Still open in step 1: the read benchmark needs the table on the production
server (a scratch copy of partition 128, or the real fill in step 4).
