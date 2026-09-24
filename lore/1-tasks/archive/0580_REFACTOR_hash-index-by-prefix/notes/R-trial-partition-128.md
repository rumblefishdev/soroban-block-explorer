---
prefix: R
title: Trial partition 128 — prefix index B/row, ledger codec, collisions
status: mature
---

# R — Trial, partition 128 (2026-09-23)

Production partition 128 of `transaction_hash_index` (64,000,000–64,499,999,
215,860,779 rows) streamed read-only per 10k-ledger slice into a local
ClickHouse 26.3, one `MergeTree ORDER BY hash_prefix` table with every variant
as a column (same rows, same order). The load stopped at the laptop's disk
guard after **156,896,102 rows (73%)**, 64,000,000 onward; 10 unmerged parts.
Measured on those rows.

| column / codec                         | B/row     |
| -------------------------------------- | --------- |
| `hash` FixedString(32), today          | 32.113    |
| `hash_prefix` UInt64                   | **8.031** |
| `ledger_sequence` Int64, default (LZ4) | 4.053     |
| `ledger_sequence` `ZSTD(1)`            | 2.589     |
| `ledger_sequence` `ZSTD(3)`            | 2.633     |
| `ledger_sequence` `T64, ZSTD(1)`       | **2.186** |
| `ledger_sequence` UInt32, default      | 3.919     |
| `ledger_sequence` UInt32 `ZSTD(1)`     | 2.822     |

Production partition 128 today (`system.parts_columns`): `hash` 32.121,
`ledger_sequence` 4.115 B/row — the local default matches it, so the method
is faithful.

**Row: 36.24 → 10.22 B (−72%)** with `hash_prefix UInt64` +
`ledger_sequence Int64 CODEC(T64, ZSTD(1))`. Whole table (_estimate_, one
partition × 5.14 bn rows): 174.96 → ~49 GiB, **~126 GiB saved**. `T64` wins
because behind a random prefix the ledger has no order to delta, only a
narrow range (19 bits inside a partition).

**Collisions:** 0 prefixes shared by two hashes among the 156.9 M rows
(`GROUP BY hash_prefix HAVING count() > 1`, in-order aggregation). Expected
over the whole table: ~0.7 (5.14 bn² / 2⁶⁵); the reader handles them by
taking every ledger with the prefix.
