---
id: '0422'
title: 'OPS: clean up the historical duplicate copy left by a backfill (ledgers + invocations; classify the rest)'
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

The two that _are_ clean are the small ones — e.g. partition 100 holds only
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

## Scope is wider than `ledgers` — measured

`ledgers` was simply where the bug surfaced. Duplication is present across most
RMT tables, and **the ratios split into two clearly different signatures** which
need opposite treatment:

| table                                                                                          | rows    | excess  | share              | signature                                     |
| ---------------------------------------------------------------------------------------------- | ------- | ------- | ------------------ | --------------------------------------------- |
| `asset_enrichment`                                                                             | 608,313 | 276,585 | ~45%               | to classify                                   |
| `liquidity_pools`                                                                              | 73,242  | 20,959  | ~29%               | to classify                                   |
| `assets`                                                                                       | 769k    | ~435k   | >2×                | second pass?                                  |
| `ledgers`                                                                                      | 25.9M   | ~12.9M  | 2×                 | **second pass** (history ×2, live ×1.0)       |
| `soroban_invocations_appearances`                                                              | —       | —       | 2× on older ranges | **second pass** (clean from ~63.50M)          |
| `soroban_contracts`                                                                            | 138k    | ~8.5k   | 6.6%               | to classify                                   |
| `accounts`                                                                                     | 14.97M  | ~620k   | 4.3%               | **engine working as designed** — do not touch |
| `lp_positions`                                                                                 | 108,553 | 712     | 0.7%               | to classify                                   |
| `transaction_participants`                                                                     | —       | —       | 0.36%              | to classify                                   |
| `soroban_contract_metadata`                                                                    | 3,831   | 3       | 0.1%               | to classify                                   |
| `transactions`, `soroban_events`, `operations_appearances`, `liquidity_pool_snapshots`, `nfts` | —       | 0       | clean              | nothing to do                                 |

Two signatures, two conclusions:

- **Old ranges ×2, recent ranges clean** (`ledgers`, `soroban_invocations_appearances`)
  — a one-time second write pass. Cleanable, and the boundary is visible:
  `soroban_invocations_appearances` is duplicated at 63.40M / 63.45M and clean at
  63.50M / 63.55M.
- **A steady low percentage** (`accounts` at 4.3%) — the engine absorbing a
  continuous rewrite stream. NOT cleanable; an OPTIMIZE there is wasted work.

Everything in the middle (`asset_enrichment`, `liquidity_pools`, …) must be
classified **before** any cleanup: measure whether the excess is concentrated in
old ranges (second pass → clean it) or spread evenly and regenerating (engine →
leave it).

> Read-time deduplication in 0420 stays regardless, and this table is the reason
> why: the problem was never one table. Every read listed above is already
> deduplicated in code (`FINAL`, `LIMIT 1 BY`, `GROUP BY`+`argMax`), which is
> what makes the site correct today irrespective of this cleanup.

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

- Operator's recollection is that **a backfill was run** — this is the leading
  hypothesis and it fits every piece of evidence: a backfill re-walks history,
  which is exactly the "old ranges doubled, live range clean" boundary seen in
  both `ledgers` and `soroban_invocations_appearances`. Confirm which backfill
  run, over which range, and whether it is re-runnable.
- `system.part_log` shows `MutatePart` activity on 2026-07-17, and the oldest
  active part dates from the same day; something touched the whole table then.
  Note `part_log` retention is short, so this window may be all that survives —
  check it early.
- Cross-check backfill / re-ingest runbooks and any Fargate backfill task runs
  around that date.

- [ ] Identify the backfill run (which job, which range, when) and confirm
      whether re-running it would duplicate again
- [ ] If it can: make it idempotent (or document the required cleanup) **before**
      the OPTIMIZE, otherwise this task will recur

## Implementation Plan

- [ ] Confirm the duplicate-copy boundary and per-partition row counts
- [ ] Classify EVERY table in the scope table above by signature (old-ranges-×2
      vs steady-percentage) before touching any of them — the two need opposite
      treatment, and `accounts` must be excluded
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
