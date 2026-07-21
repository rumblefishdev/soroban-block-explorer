---
id: '0426'
title: 'BUG: 12 version-less RMT tables make re-parsing history unsafe — `--reindex` can keep the stale row'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0425', '0232', '0421', '0266', '0356']
tags: [priority-high, effort-large, clickhouse, data-integrity, backfill]
links:
  - crates/db-clickhouse/schema/init.sql
  - docs/backfills.md
history:
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Spawned from 0425. The rule that task wrote down — "if the signal is only in
      XDR, re-parse the range with `run --reindex`; do not write a bespoke
      targeted-write binary" — is the right rule and **cannot be followed safely
      today**. Twelve tables carry no RMT version column, so re-parsing a range that
      was ingested by an older parser build produces rows at equal version with
      different values, and ClickHouse keeps an arbitrary one. Wrong data, not
      duplicates. Both binaries 0425 deleted (`metadata-backfill`,
      `pool-ids-backfill`) wrote to a single table partly to dodge exactly this.
      Rejected framing: carving an exception into the 0425 rule that blesses
      targeted-write binaries whenever the build differs. That is a plaster — it
      licenses the workaround and leaves the defect. The table should express
      "newest write wins"; operator discipline should not have to.
---

# BUG: version-less RMT tables make re-parsing history unsafe

## Summary

`ReplacingMergeTree` with no version column picks an **arbitrary** row among
duplicates. That is harmless when re-parsing produces byte-identical rows (same
parser build) and silently wrong when it does not. Twelve tables are in that
state, so `run --reindex` — the sanctioned way to rebuild history — is unsafe on
exactly the ranges that most need rebuilding: the old ones, parsed by an old
build.

## Context

Recorded already as rule 4 in [`docs/backfills.md`](../../docs/backfills.md), as
a hazard to plan around:

> **Different build → unsafe** on version-less tables: `liquidity_pool_snapshots`,
> `assets`, `transactions`, and the 9 event-log tables. At equal version with a
> changed value, RMT may keep the **stale** row from the earlier attempt.

It has already produced wrong data once, by a different route: 0356 found the
parser emitting both the before- and after-image of a pool per op, so one
`(pool, ledger)` key got two rows with different reserves and version-less RMT
picked one **at random**. The parser fix landed; the engine-level exposure did
not change.

The cost compounds: because `--reindex` is unsafe, every historical fix so far
was shipped as a bespoke single-table binary with its own partition loop,
watermark and resume logic — `metadata-backfill` (0304), `pool-ids-backfill`
(0266). 0425 deleted both and wrote the rule against that shape. This task is
what has to land for the rule to be followable.

## The shape of the fix (to decide, not yet decided)

Same family as [0232](0232_FEATURE_clickhouse-tier1-live-mode-mitigation.md) /
[0421](0421_BUG_first-seen-ledger-clobbered-on-every-account-write.md): make the
**table** enforce the invariant instead of the operator.

Options, to be weighed with measurements, not preference:

1. **Add a version column** carrying a write clock (ingest ms) so the newest
   write always wins. In-house precedent: `nft_enrichment` and `asset_enrichment`
   already use the writer's own clock as version. Cost: breaks byte-identical
   replay determinism, which the parallel-writer design leans on (`ids.rs`
   "Parallel-writer safety") — needs thought, not a shrug.
2. **Version by parser build** (a monotonic build/schema counter) — replay stays
   deterministic within a build, and a newer build always wins. Needs a build
   ordinal the ingest path can read.
3. **Partition-level replace** for re-parses (drop + re-ingest the CH partition)
   instead of relying on merge semantics. Recorded as rejected in 0228 because
   backfill commits per 64k ledgers while CH partitions are 500k — re-examine,
   the constraint may not bind for a deliberate re-parse.

Enumerate the 12 tables from `init.sql` first — the rule-4 list names 3 plus
"the 9 event-log tables" and should not be trusted without a re-count.

## Acceptance Criteria

- [ ] The 12 tables are enumerated from `init.sql`, not from the doc prose.
- [ ] A re-parse of an old range with a newer parser build is **proven** to keep
      the newer row — fixture on a real ClickHouse, both orders of arrival.
- [ ] `docs/backfills.md` rule 4 either disappears or shrinks to "safe, because
      the tables are versioned".
- [ ] The `crates/backfill-runner/README.md` clause-2 blocker note is removed —
      `run --reindex` becomes the unconditional answer for XDR-only signals.
- [ ] Docs updated — `docs/architecture/database-schema/**` (schema shape change)
      and `docs/backfills.md`.
- [ ] API types regenerated — `N/A` unless `crates/api/**` or `Cargo.*` change.
