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

## Why merges will not finish the job on their own

Merges are **healthy and active** — 40,524 `MergeParts` in 7 days, none stuck.
They are not behind. But ClickHouse **never promises to merge a partition down to
a single part**, and dedup only happens inside a merge, so a partition that
settles at 4–5 parts keeps its duplicate copies indefinitely.

Measured, and it cuts both ways:

```
partitions already deduplicated (×~1.0):   2 of 28
partitions still ×2:                      24 of 28
total rows, start of session → later:  25,965,607 → 25,965,956  (not shrinking)
```

The two that *are* clean are the small ones — e.g. partition 100 holds only
42,576 sequences (history starts mid-partition), so a full merge was cheap
enough to happen. Full 500k-sequence partitions (~1M rows) stop short. So
waiting works only for the partitions that do not matter.

## Why this is cheap

The whole table is **1.07 GiB / 107 active parts / 28 partitions**. This is not
a big-table operation.

## Contrast with `accounts` — do NOT do the same there

`accounts` looks similar but is the opposite case: ~8.46M rows/day are inserted
for ~136k genuinely-active accounts (every activity rewrites the row), so merges
hold a **steady ~4.3% un-merged surplus**. An `OPTIMIZE` there is pointless — the
surplus returns within hours. `accounts` must stay deduplicated **at read time**
(see 0420). Only append-only tables qualify for this cleanup.

## FIRST: find out what wrote the second copy

**Do not clean up before answering this.** An `OPTIMIZE` removes the effect; if
the process that produced it can run again, we simply pay for the same cleanup
twice and learn nothing.

What is established:

- There were **two distinct ingest generations**. A merged part is named
  `100_6_422655_4_476814` — block number **6** and block number **422655** in one
  part, i.e. rows inserted in two widely separated write sessions.
- **The live writer is not the culprit**: 15,373 rows/day for 15,373 ledgers/day,
  and the current partition measures ×1.0.

What is NOT established:

- **Which process performed the second pass, and when.** Part
  `modification_time` cannot date it — merges rewrite parts, so those timestamps
  reflect the last merge, not the original insert.

Leads to check:

- Operator's recollection is that a **re-ingest was run recently** — plausible
  and consistent with the two-generation evidence, but unconfirmed.
- `system.part_log` shows `MutatePart` activity on 2026-07-17, and the oldest
  active part dates from the same day; something touched the whole table then.
  Note `part_log` retention is short, so this window may be all that survives —
  check it early.
- Cross-check backfill / re-ingest runbooks and any Fargate backfill task runs
  around that date.

- [ ] Identify the writer and confirm whether it can run again
- [ ] If it can: make it idempotent (or document the required cleanup) **before**
      the OPTIMIZE, otherwise this task will recur

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
