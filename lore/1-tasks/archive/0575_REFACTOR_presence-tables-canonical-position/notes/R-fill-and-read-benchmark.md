---
prefix: R
title: Staging fill of partitions 100–128 and the read benchmark, measured
status: mature
---

# Staging fill of partitions 100–128 and the read benchmark

Production, 2026-09-23. Fill by the operator (`fill_presence.zsh`, `chw`);
every measurement below read-only through `chq`.

## Fill

Both staging tables hold partitions 100–128, every 50,000-ledger slice gated
(distinct keys of the old table equal to the staging copy). Partition 129,
the head, is left for the window.

Active parts after the fill (`system.parts`):

| table                         | rows 100–128, old | rows 100–128, staging | old size   | staging size         |
| ----------------------------- | ----------------- | --------------------- | ---------- | -------------------- |
| `transaction_participants`    | 10,923,214,300    | 10,902,524,793        | 112.03 GiB | 19.97 GiB, 230 parts |
| `operation_asset_appearances` | 11,740,691,398    | 11,787,887,582        | 100.75 GiB | 12.35 GiB, 233 parts |

The row counts differ by 0.19% and 0.40%: unmerged duplicates on either side
(the old tables carry them, and slices re-run after a stop fill twice). The
distinct keys are what the gate compares, and they matched in every slice.
Saving on these partitions: −92.06 and −88.40 GiB, −180 GiB together,
before the staging parts merge.

### What the gate needed

The per-slice gate was rewritten three times during the fill:

1. `uniqExact` over the key tuple of a whole 50k slice: over the per-query
   memory cap (~3.73 GiB) on 54M keys.
2. `uniqExact` over a hash of the tuple: fits, but a hash collision
   (`operation_asset_appearances`, slice 59,350,000: 42,772,224 against
   42,772,225; the exact tuple count was equal) made it report a mismatch
   that was not there.
3. Exact tuple count in two halves in one query: over the cap again on
   `transaction_participants` 58,600,000, which had been filled twice.

What held: one exact query per quarter of a slice, each a separate call
(the largest used 1.89 GiB), summed in the shell.

Running both tables in parallel spent the `dev_read` hourly read quota
(2 TiB then) partway through partition 126 of `transaction_participants`; it
resumed after the reset. Two runs side by side read ~2 TiB per hour through
their gates; the quota has been 4 TiB/h since then.

## Read benchmark

First page (26 rows), best of 3, driver seek plus the `transactions` page
fetch, production path (old table, `(ledger_sequence, id) IN`) against this
task's path (staging table, `(ledger_sequence, application_order) IN`). A
`—` marks a time not recorded.

### Asset list

| asset            | path | read_rows | MiB  | s    |
| ---------------- | ---- | --------- | ---- | ---- |
| native XLM       | old  | 63.6M     | 1451 | 0.27 |
|                  | new  | 30.4M     | 516  | 0.12 |
| abUSDC (mid)     | old  | 1.6M      | 36   | —    |
|                  | new  | 4.0M      | 65   | 0.04 |
| sparse (31 rows) | old  | ~1.75M    | 22   | —    |
|                  | new  | ~1.75M    | 22   | —    |

The old native XLM path includes the second arm (the SAC's
`soroban_invocations_appearances`), which this task removes; it is the only
list over the gate's 1 GiB today.

### Account list

| account                            | path | read_rows  | MiB   | s     |
| ---------------------------------- | ---- | ---------- | ----- | ----- |
| hottest (6.0M rows in 50k ledgers) | old  | 25,899,265 | 591.1 | 0.117 |
|                                    | new  | 37,682,441 | 639.4 | 0.138 |
| mid (318 rows in 50k ledgers)      | old  | 993,793    | 19.2  | 0.032 |
|                                    | new  | 1,196,365  | 21.9  | 0.045 |
| sparse (3 rows at 55.0M)           | old  | 1,501,985  | 22.0  | 0.039 |
|                                    | new  | 2,368,397  | 30.0  | 0.050 |

The account list reads 20–58% more rows (8–36% more bytes) on the staging
table: it has 230 unmerged
parts against the old table's 93, and a seek reads a granule per part. Every
list stays under the gate (< 1 s, < 1 GiB); re-measure after the swap, once
merges have caught up.
