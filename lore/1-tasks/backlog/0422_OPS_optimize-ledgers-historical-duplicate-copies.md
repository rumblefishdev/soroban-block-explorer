---
id: '0422'
title: 'OPS: OPTIMIZE away the historical duplicate copy in `ledgers` (one-time, ~2x rows)'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0420']
tags: ['area-clickhouse', 'ops', 'effort-small', 'priority-medium']
links: []
history:
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Spawned from 0420. Measurement showed the ledgers duplication is a
      one-time historical artifact, not a live problem and not a merge failure -
      so unlike `accounts` it CAN be cleaned up durably with a single OPTIMIZE.
---

# OPS: OPTIMIZE away the historical duplicate copy in `ledgers`

## Summary

`ledgers` holds **2 physical rows per sequence across ~98% of history** (25.96M
rows for 13.07M distinct sequences). This is a **one-time artifact of a
historical backfill that wrote a second copy** — not a live ingest problem and
not a merge failure. A single `OPTIMIZE` removes it durably.

## Evidence that it is historical, not live

```
partition 127 (live, current):     78,089 rows /  77,930 sequences  → ×1.0 clean
partitions 120–126 (history):   1,000,001 rows / 500,000 sequences  → ×2.0
live ingest:                    15,373 rows/day = 15,373 ledgers/day → 1 row per ledger
```

The live writer is correct. Every partition below the boundary (~ledger 63.5M)
carries exactly two copies.

## Why merges will never clean it up on their own

Merges are **healthy and active** — 40,524 `MergeParts` in 7 days. They are not
behind. The point is that ClickHouse **never promises to merge a partition down
to a single part**: old partitions sit at 4–5 parts, the merge selector sees no
benefit, and the two copies live in different parts forever. Waiting does not
fix it.

## Why this is cheap

The whole table is **1.07 GiB / 107 active parts / 28 partitions**. This is not
a big-table operation.

## Contrast with `accounts` — do NOT do the same there

`accounts` looks similar but is the opposite case: ~8.46M rows/day are inserted
for ~136k genuinely-active accounts (every activity rewrites the row), so merges
hold a **steady ~4.3% un-merged surplus**. An `OPTIMIZE` there is pointless — the
surplus returns within hours. `accounts` must stay deduplicated **at read time**
(see 0420). Only append-only tables qualify for this cleanup.

## Implementation Plan

- [ ] Confirm the duplicate-copy boundary and per-partition row counts
- [ ] Audit which other RMT tables are append-only and similarly affected
      (`assets` measured at >2× — verify whether it is append-only or
      update-heavy before including it)
- [ ] `OPTIMIZE TABLE ledgers FINAL` (consider partition-by-partition to bound
      the work), off-peak
- [ ] Verify: `count()` == `uniqExact(sequence)`, and re-check the read-cost
      baseline — the ledgers list currently reads **1,349,927 rows to return
      20** because of part fragmentation; this should improve measurably
- [ ] Confirm no regrowth after 24h (proves the live writer is clean)

## Acceptance Criteria

- [ ] `ledgers` physical rows == distinct sequences
- [ ] Ledgers list read cost re-measured and recorded
- [ ] No regrowth after 24h
- [ ] Read-time dedup in 0420 stays in place regardless — RMT never guarantees
      dedup, and this cleanup does not license removing those guards

**Consent-gated:** this is a write against production ClickHouse. Do not execute
without an explicit per-action go from the operator.
