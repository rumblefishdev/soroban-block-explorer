# Dedup and filtered pages without over-fetch — options (2026-09-22)

Decided: not now; this is the target when the over-fetch items are fixed.

**Measured on production (26.3), first page, native SAC:**

| read                                   | `LIMIT 1 BY`            | `FINAL` + `LIMIT`              |
| -------------------------------------- | ----------------------- | ------------------------------ |
| contract list, `contract_transactions` | 25–27M rows, 108–149 ms | 230M rows, 3.86 GiB, 1.1–1.2 s |
| same, a contract quiet since 128       | 1.8M rows, 24–36 ms     | 1.8M rows, 30–33 ms            |
| contract events page                   | 144–178 ms (keys first) | timeout, over 30 s             |

**ClickHouse, per its source, PRs and docs** (research, 2026-09-22):

- `FINAL` is ClickHouse's own answer for exact reads of a ReplacingMergeTree,
  and 26.3 has its speed-ups on (range splitting, vertical `FINAL`, skip
  indexes under `FINAL`). But on 26.3 a descending `FINAL … ORDER BY key DESC
LIMIT n` does not read in order: it reads every matching range and sorts —
  the 230M above. Fixed in 26.9 (`optimize_read_in_reverse_order_final`,
  default on, ReplacingMergeTree only; not in the 26.8 LTS). Lazy reading of
  heavy columns under `FINAL` arrives in 26.4.
- `LIMIT 1 BY` does not stop the read early on any released version (upstream
  issue #113110 reproduces our pattern: 29.5M rows against 274k; the fix,
  PR #113565, is open).
- A filter on a non-key column with `ORDER BY key LIMIT n` stops as soon as
  `n` rows pass it, and heavy columns are read only for those rows — when
  nothing else blocks the early stop.
- Skip indexes help rare values only (`set` or `bloom_filter`); measured here:
  `RESTORE_FOOTPRINT` sits in 14 of partition 128's 29,782 granules,
  `REVOKE_SPONSORSHIP` in 152.
- Recommended shape for "list by X, filtered, newest first": the filter
  columns in the table the list is ordered by (or a second table / view).
  Projections are never used with `FINAL`.

**Target design:**

1. Upgrade to ClickHouse ≥ 26.9 and re-measure `FINAL` on the descending
   pages. If it reads in order, `FINAL` replaces both `LIMIT 1 BY` and every
   over-fetch (lists, assets, ledgers): exact pages, early stop.
2. Operation-type filter: a `type` skip index on `operations_appearances`, then
   drop the partition pin — to be measured on one partition first.
3. Contract list with a second filter: `source_id` and an operation-type mask
   on `contract_transactions`, so the filter runs inside the seek (schema
   change and refill of ~3 bn rows).
