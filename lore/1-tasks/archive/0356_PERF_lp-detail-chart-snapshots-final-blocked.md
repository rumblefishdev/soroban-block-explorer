---
id: '0356'
title: 'PERF/BUG: lpdetail+lpchart snapshots FINAL (blocked) + non-deterministic snapshot data bug (indexer emits before+after images)'
type: PERF
status: completed
related_adr: []
related_tasks: ['0354', '0338']
tags:
  [
    priority-medium,
    effort-medium,
    layer-clickhouse,
    layer-indexer,
    milestone-3,
    phase-launch,
  ]
milestone: 3
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Deferred from 0354 — lpdetail/lpchart cannot drop FINAL output-identically; surfaced a snapshot data-quality bug in the process.'
  - date: 2026-07-06
    status: backlog
    who: fmazur
    note: >
      Root cause diagnosed on the local DB. NOT a ClickHouse bug — the indexer
      (xdr-parser/src/state.rs) emits one snapshot per LedgerEntryChange,
      including BOTH the read-only `state` (before-image) AND `updated`
      (after-image) of each op, so a pool touched in a ledger gets multiple
      snapshots with DIFFERENT reserves. "One snapshot per (pool, ledger)" is
      delegated to a non-deterministic DB dedup (PG `DO NOTHING` keeps first-
      inserted; CH ReplacingMergeTree-without-version keeps an arbitrary one).
      Fix belongs in the indexer. Details below.
  - date: 2026-07-07
    status: active
    who: stkrolikiewicz
    note: >
      Promoted to active. Raw-meta re-parse of ledger 62075700 (public
      archive) confirmed the diagnosis — all 41 touched pools have
      before≠final, validated 41/41 vs Horizon effects. Implementing the
      indexer fix: snapshot only from mutating changes + ledger-scope
      keep-last dedup (deterministic final per (pool, ledger)).
  - date: 2026-08-13
    status: completed
    who: stkrolikiewicz
    note: >
      Archived — all four PRs merged and running on prod since mid-July;
      the task file simply never recorded it. #318 (07-13) fixed the
      indexer: snapshots only from mutating changes + ledger-scope
      keep-last dedup; re-ingest idempotent in PG and CH; integration
      test on ledger 62075700 validated 41/41 vs Horizon; no version
      column, docs N/A. #335 (07-14) dropped FINAL on the four snapshot
      reads, verified byte-identical against prod (0 mismatches on all
      four sites); lpdetail read_rows 988,348 → 65,652 (~15×). The
      50M/mo load test (07-17) then drove two follow-ups under this
      task's scope: #347 (lpdetail: seek `ledgers` by one sequence +
      equi-join; box total read work 78.3 → 38.35 bn) and #349 (lpchart:
      bound the upper ledger seek both ways; 26.3M → 684k rows/req).
      Endgame record kept in ## Outcome. One AC deferred and defused:
      table-wide cleanup of pre-#318 duplicate rows (scan hits the 6 GiB
      cap) — harmless, every read path dedups via LIMIT 1 BY since #335.
---

# PERF/BUG: lpdetail+lpchart snapshots FINAL + non-deterministic snapshot bug

## Summary

Two coupled problems on `liquidity_pool_snapshots`, deferred from 0354:

1. **Data bug (root cause found):** the snapshot for a `(pool_id, ledger_sequence)`
   is **not a deterministic function of the ledger** — the indexer emits several
   snapshots per pool/ledger (before/after images of each op) and lets a
   non-deterministic DB dedup pick one. **This is an indexer bug, not ClickHouse.**
2. **Perf (blocked on #1):** `lpdetail` (`fetch_pool_by_id`, ~16M) and `lpchart`
   (`fetch_pool_chart`, ~14M) read a pool's whole snapshot slice under `FINAL`.
   `FINAL` can't be dropped output-identically until the snapshots are
   deterministic — so #1 must land first.

## Root Cause (diagnosed 2026-07-06, local DB)

**Where:** `crates/xdr-parser/src/state.rs` (~808–894). The extractor iterates
**every** `LedgerEntryChange` in the ledger and, for each pool change of type
`created | updated | restored | state`, emits a separate
`ExtractedLiquidityPoolSnapshot` from that change's reserves (l. 826–859).

Stellar Core writes, per operation, both a `state` change (read-only **before**
image) and an `updated` change (**after** image). The code takes **both** →
a pool touched by an op gets **≥2 snapshots with different reserves** (before vs
after the op). The intended "one snapshot per (pool, ledger) = end-of-ledger
reserves" is **not enforced in the parser** — it's delegated to the DB unique
constraint:

- **PG:** `uq_lp_snapshots_pool_ledger DO NOTHING` (write.rs) → keeps the
  **first-inserted** row (deterministic, but "first" ≠ "final/correct").
- **ClickHouse:** `ReplacingMergeTree` with **NO version column** → keeps an
  **arbitrary** row (at merge, and at `FINAL` read-time).

So the "canonical" snapshot reserves are effectively an arbitrary intra-ledger
image (before OR after some op), not the ledger's final state. **ClickHouse is
not buggy** — it dedups correctly; the no-version schema just _exposes_ the bug,
whereas PG's `DO NOTHING` _masks_ it.

### Evidence (local, ledger 62075700)

- **Only ledger 62075700 has duplicates** in the whole table (6 pools) — it's the
  one whose parts happen not to be merged (see "why still visible" below), not a
  systemic count.
- **Only `reserve_a`/`reserve_b` differ**; `total_shares` / `tvl` / `volume` /
  `fee_revenue` / `gross_volume_a` identical across the duplicate rows.
- The delta has the **swap signature** (reserve_a ↑, reserve_b ↓, LP shares
  unchanged) — i.e. the two rows are the **before-op vs after-op** reserves.
- **Even pools with a single op** in that ledger have differing duplicates →
  confirms it's `state`(before) vs `updated`(after) of one op, not multi-op.
- The two rows sit in **two separate parts** (`124_1_1_1` block 1, `124_2_2_1`
  block 2, same backfill 10:20) → ledger 62075700 was ingested in **two passes**,
  each contributing one image; the non-deterministic dedup then diverged.

### Why the duplicates are still visible locally (merge mechanics)

- `ReplacingMergeTree` dedups only when the two rows land in the **same merged
  part**; here they're in two un-merged parts.
- `system.part_log` shows **2× `NewPart`, 0× `MergeParts`** — no merge was ever
  attempted. Partition 124 has just **2 tiny parts (~12 MiB)** and the DB is idle
  (backfill done, no inserts since 10:20) → **no merge trigger** (merges are
  driven by insert activity + part-count pressure, neither present). CH merges to
  control part count, not to remove duplicates.
- **Regardless of physical merge, the API's `FINAL` already collapses them at
  read-time** to one arbitrary row. So the physical duplicates are only the
  visible symptom; the real defect (arbitrary snapshot value) is live now.
- Corollary: on prod, once the chain passes ledger 62.5M this partition stops
  receiving inserts; if it settles into few parts it may sit un-merged too — the
  bug does not "heal" by waiting.

## Implementation

**Primary — fix the indexer (makes both PG and CH deterministic + idempotent):**

- In `state.rs`, emit **exactly one** snapshot per `(pool_id, ledger_sequence)`,
  from the **final** reserves — the last _mutating_ change (`created`/`restored`/
  `updated`, highest operation-order in the ledger). Use the read-only `state`
  (before) image **only** for the pool dimension / FK satisfaction (its original
  purpose, lore-0189), **never** as a competing snapshot with stale reserves.
- Result: a re-ingest of the same ledger yields the **same** row (idempotent) and
  a single pass has no competing images → no differing duplicates in either DB.

**Cleanup — existing bad rows:**

- Backfill/rewrite affected `(pool, ledger)` snapshots to the correct final
  reserves (only ledger 62075700 known locally; enumerate on prod via a
  `GROUP BY pool_id, ledger_sequence HAVING count()>1` scan, but note CH `FINAL`
  hides them — scan the raw table or `system.parts`).

**Optional (CH schema, band-aid only):** a version column (`ingested_at` /
monotonic seq) would make CH's dedup _deterministic_ — but it still wouldn't
identify the _correct_ end-of-ledger value, so it does not replace the indexer
fix. (User constraint: schema change limited to indexes — a version column
exceeds that and needs explicit go-ahead.)

**Then perf (unblocked once snapshots are deterministic):**

- Drop `FINAL` in `lpdetail`/`lpchart`; latest snapshot via read-in-order
  `ORDER BY ledger_sequence DESC LIMIT 1`, bucket aggregates over a deduped
  subquery. Verify byte-identical on many pools (local API).

## Acceptance Criteria

- [x] Root cause of differing-duplicate snapshots identified — indexer emits
      before+after images per op; non-deterministic DB dedup (see Root Cause)
- [x] Indexer emits exactly one deterministic (final-reserves) snapshot per
      `(pool_id, ledger_sequence)`; re-ingest is idempotent (same row in PG **and** CH)
      — #318 (3 unit tests + integration test on ledger 62075700, 41/41 vs Horizon)
- [ ] Existing bad snapshot rows corrected (incl. ledger 62075700) — deferred,
      defused: 62075700 itself is clean on prod (parts merged); the table-wide
      dup scan hits the 6 GiB cap, and every read path dedups via `LIMIT 1 BY`
      since #335, so residual dormant duplicates cannot surface
- [x] `lpdetail`/`lpchart` drop `FINAL`; output byte-identical on many pools —
      #335, verified against prod read-only (0 mismatches on all four sites)
- [x] `read_rows` for the hottest pool well under the ~14M FINAL-merge figure —
      988,348 → 65,652 (#335), then #347/#349 cut far deeper (see Outcome)
- [x] Docs (ADR 0032): N/A — no version column added; #318 fixed the writer instead

## Outcome (shipped 2026-07, archived 2026-08-13)

Four PRs, all merged and deployed during the July read-path perf work:

| PR   | Date  | What                                                       | Measured                                                                     |
| ---- | ----- | ---------------------------------------------------------- | ---------------------------------------------------------------------------- |
| #318 | 07-13 | Indexer: deterministic final snapshot per `(pool, ledger)` | 41/41 pools vs Horizon on ledger 62075700; re-ingest idempotent              |
| #335 | 07-14 | Drop `FINAL` on lpdetail / lpchart / lplist snapshot reads | Byte-identical on prod; lpdetail read_rows 988,348 → 65,652 (52.8 → 2.4 MiB) |
| #347 | 07-17 | lpdetail: seek `ledgers` by one sequence, equi-join        | Box total 78.3 → 38.35 bn reads at 50M/mo; lpdetail 27.2M → 1.7M rows/req    |
| #349 | 07-17 | lpchart: bound the upper ledger seek both ways             | 26.3M → 684k rows/req (lpchart had been 37.4% of the box's reads)            |

Load-test verdict worth keeping (50M/mo = 19.45 req/s): the only endpoints that
hold their p95 under 5× load are the two that never touch ClickHouse (txdetail
1.1×, nftdetail 1.0×) — everything CH-bound degrades 2.4–7.3×. The box is the
bottleneck. Remaining candidates were measured and deliberately deferred:
lplist `created_at` (~9.6M rows/req — fixing it re-opens 0208 Path 1, a stored
`created_at_ledger`, rejected for writer/RMT reasons) and nftdetail persist
(0.1% of reads).

Deferred, defused: enumerating residual pre-#318 duplicate rows (table-wide
`GROUP BY … HAVING count()>1` exceeds the 6 GiB query cap; ledger 62075700
itself is clean on prod). Harmless: writes are deterministic since #318 and
every read path dedups via `LIMIT 1 BY` since #335.
