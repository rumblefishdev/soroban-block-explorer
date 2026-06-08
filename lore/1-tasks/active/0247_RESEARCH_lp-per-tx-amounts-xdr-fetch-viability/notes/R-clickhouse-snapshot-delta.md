---
prefix: R
title: Path E — per-tx LP amounts from ClickHouse snapshot reserve deltas
status: finding
spawned_from: '0247'
---

# Finding: CH already holds the reserve time-series → a pure-SQL path exists

## What was checked

- `crates/db-clickhouse/schema/init.sql` — `liquidity_pool_snapshots`
  (lines 378-390) and `operations_appearances` (287-302).
- `crates/xdr-parser/src/state.rs:555-649` — how the indexer emits
  `ExtractedLiquidityPoolSnapshot` from LP `LedgerEntryChanges`.

## The data is already there

`liquidity_pool_snapshots` stores, per `(pool_id, ledger_sequence)`, the
**post-state** `reserve_a` / `reserve_b` (Decimal128(7)) of the pool at
every ledger where it changed. The indexer writes one row per pool per
changed-ledger (`state.rs:639`), deduped by `uq_lp_snapshots_pool_ledger
DO NOTHING` (write.rs). Its original purpose is the analytics **chart**
endpoint (TVL/volume/fee time-series), NOT per-tx amounts.

But the reserves are raw. The **delta between consecutive snapshots = the
net amount moved** in that ledger:

```sql
reserve_a - lagInFrame(reserve_a) OVER (
  PARTITION BY pool_id ORDER BY ledger_sequence
) AS delta_a   -- net A moved at this ledger
```

`operations_appearances` carries `pool_id, transaction_id,
application_order` per LP op → tells WHICH tx/op touched the pool in that
ledger. Join the two and you get per-tx amounts **with no XDR archive fetch
and no new ingest extractor** — for the clean case.

Direction from delta signs:

- both `+` → deposit
- both `-` → withdraw
- opposite signs → trade (direction = which side went up vs down)

## This defines Path E (pure CH SQL)

| Path  | Source                  | XDR fetch       | New table | New extractor   |
| ----- | ----------------------- | --------------- | --------- | --------------- |
| A     | archive read-time       | yes (hot path)  | no        | yes (op-result) |
| C     | indexer → side table    | parse at ingest | yes       | yes             |
| **E** | snapshots LAG + oa join | **no**          | **no**    | **no**          |

Path E reuses data already ingested for the chart endpoint. Zero new infra.

## The hard limit — granularity (see R-... collision + diagram)

Snapshot is per **(pool, ledger)**, not per-tx/op. `DO NOTHING` keeps only
the FINAL post-state of the ledger. If a ledger has **>1 LP op on the same
pool** (independent txs, path-payment trades routing through the pool, or
multi-op txs), the LAG delta is the **net sum** across all of them and
**cannot** be split per-op. The XDR op-meta retains per-op before/after;
the CH snapshot collapses it.

So Path E is exact **only** for ledgers with exactly one LP op per pool.
Coverage = the fraction of LP ops that are the sole LP op on their pool in
their ledger. On HOT pools (the ones users view) multiple trades per ledger
are plausibly common → coverage may be materially below 100%. **This is the
gate, not a formality.**

## Measured 2026-06-03 (prod CH) — gate CLOSED

| Query                            | Result    | Meaning                                               |
| -------------------------------- | --------- | ----------------------------------------------------- |
| per-group collision (all pools)  | **5.75%** | misleading — groups counted equally                   |
| **per-op collision (all pools)** | **25.0%** | 1-in-4 displayed LP-op rows sit in a colliding ledger |
| per-group collision (hot top-50) | 9.5%      | per-group; per-op-hot would be higher                 |

Collisions are **dense**: 25% of ops live in the 5.75% of groups that
collide (a 10-op group = 10 unattributable ops but 1 group). The per-op
number is the decision-relevant denominator for a row list.

**Conclusion:** Path E is exact for ~75% of rows, **25% collide**. With the
product requirement = **100% per-tx amounts (must-have)**, Path E (and
Path E + degrade) is insufficient, and a 25% Path-A XDR fallback is too
much hot-path S3. → **Path C** (ingest-side per-op extraction) is the
selected path. See `S-recommendation.md`; decision feeds existing **task 0279** (from 0274).

## Required measurements (need CH mTLS access)

1. **Collision rate** — `% of (pool_id, ledger_sequence) groups in
operations_appearances WHERE pool_id IS NOT NULL having >1 LP op`,
   weighted by traffic (hot pools matter more). Defines Path E coverage.
2. **Delta validation** — for a sample of single-op ledgers, compare the
   snapshot LAG delta against the true amount parsed from XDR
   (`compare-with-stellar-api` skill) → confirm the delta = actual moved
   amount (watch: fee accrual, rounding, Decimal128 scale).
3. **Edge cases** — `state` change_type emits a Δ=0 snapshot (pool
   referenced, not mutated) → filter `delta != 0`; pool creation (no prior
   snapshot → LAG null = first deposit); withdraw-to-zero.

## Recommended posture

Path E as **primary** for single-op ledgers; **fall back to Path A**
(read-time XDR, per-op meta) only for the colliding minority — but only if
the collision rate is low enough that the fallback fraction stays off the
hot path. If collisions dominate on hot pools, reconsider Path C (ingest
extractor writes true per-op amounts, no collision problem at all).
