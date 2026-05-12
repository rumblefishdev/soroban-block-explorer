---
id: '0208'
title: 'ClickHouse: liquidity_pools state-semantics correction'
type: FEATURE
status: completed
related_adrs: ['0044']
related_tasks: ['0206']
tags:
  [
    'phase-future',
    'effort-small',
    'priority-medium',
    'clickhouse',
    'schema',
    'superseded',
  ]
links: []
history:
  - date: '2026-05-12'
    status: backlog
    who: fmazur
    note: >
      Spawned from task 0206 (CH writer for the 0204 schema) — observed on
      the 10k-ledger smoke at 62016000 that `liquidity_pools` accumulates
      one row per (pool, ledger-where-pool-changed) instead of one row per
      pool. 217,042 rows for 10,761 unique pools (avg_dup ≈ 20.2×). Root
      cause: schema engine is plain `MergeTree` (no dedup) but parser
      emits `ExtractedLiquidityPool` on every pool change, not only on
      creation. Two clean fix paths laid out below; pick one before
      implementing.
  - date: '2026-05-12'
    status: completed
    who: claude
    note: >
      **Folded into task 0206 production-schema refactor (Path 2
      chosen).** User redirected scope: "schema ma być produkcyjna od
      razu, napraw wszystko inline". `liquidity_pools` got:
      `created_at_ledger` dropped (derive read-time via `MIN(ledger_sequence)`
      on snapshots), `last_updated_ledger` added, engine changed to
      `ReplacingMergeTree(last_updated_ledger)`. Plus FK/identity
      columns now use natural keys (`asset_a_issuer_account` instead of
      `asset_a_issuer_id`), `LowCardinality(String)` for compression.
      Implemented in `crates/db-clickhouse/schema/init.sql` + writer
      `stage.rs` + row struct `rows.rs` + tests. ADR 0044 history
      gained a comprehensive entry covering all schema changes
      including this one. Task closed as superseded — see
      `lore/2-adrs/0044_clickhouse-pilot-parallel-store.md` history
      entry dated 2026-05-12 "Production-grade schema refactor".
---

# ClickHouse: liquidity_pools state-semantics correction

## Summary

The CH `liquidity_pools` table accumulates one row per pool-change-event
instead of one row per pool. Read-time correctness requires either
schema engine change (ReplacingMergeTree variant) or writer-side dedup.
Either path needs an ADR 0044 amendment because it changes the
`crates/db-clickhouse/schema/init.sql` shape.

## Context

Observed during task 0206 (CH writer implementation) on the 10k-ledger
smoke at 62016000–62025999:

- `liquidity_pools: 217,042 rows`, `uniqExact(pool_id) = 10,761`, `avg_dup ≈ 20.2`
- `liquidity_pool_snapshots: 217,042 rows` (intentionally append-per-change)

The two table counts match because the parser emits an
`ExtractedLiquidityPool` value **every time** it emits an
`ExtractedLiquidityPoolSnapshot` (every pool change), not only on the
pool's create event. PG side handles this via
`INSERT … ON CONFLICT (pool_id) DO UPDATE` — net 1 row per pool. CH
schema chose `ENGINE = MergeTree` per task 0204's "immutable
post-create" framing, but that framing turned out to be misaligned
with parser/writer behaviour.

Extrapolation: 11M-ledger backfill would produce ~240M rows in
`liquidity_pools` for ~kilkadziesiąt-kilkaset tysięcy actually-unique
pools. Every read-time query touching `liquidity_pools` must remember
`LIMIT 1 BY pool_id` — a real footgun.

The 0206 writer's quick-fix attempt (`ReplacingMergeTree(created_at_ledger)`

- `unwrap_or(0)` sentinel) was rejected as semantically crooked
  (`created_at_ledger` is not a watermark and `0` is a fragile sentinel
  for unknown). This task picks a proper fix.

## Two paths to choose between

### Path 1 — strict PG parity

- Schema:
  - Add `last_updated_ledger Int64` column (mirrors PG `liquidity_pools.last_updated_ledger`)
  - Keep `created_at_ledger Int64 DEFAULT 0` (sentinel for unknown when mid-stream)
  - Engine: `ReplacingMergeTree(last_updated_ledger) ORDER BY (pool_id)`
- Writer (`crates/db-clickhouse/src/persist/stage.rs`):
  - Cache `pool_id → created_at_ledger` across the partition.
    First emit with `Some(X)` for a pool stores X in cache; subsequent
    updates within the partition reuse cached X so all rows have
    identical `created_at_ledger`.
  - Pass 2 stub-rowing for pools referenced by `pool_snapshots` /
    `lp_positions` but not having a create-emit in this batch
    (mid-stream). Stub with `created_at_ledger = 0`,
    `last_updated_ledger = ledger_sequence`. Mirrors the
    `soroban_contracts` Pass 2 stub-rowing already in 0206.
- Pros: bit-for-bit parity with PG, easy mental model for anyone
  who knows PG schema.
- Cons:
  - Writer keeps state cross-ledger (HashMap per partition)
  - Cross-partition gotcha: pool created in partition A, updated in
    partition B → in B, cache is empty (writer is per-partition-fresh)
    → degraded to `created_at_ledger = 0`. Full 11M backfill resolves
    because both partitions commit and merger folds; mid-stream replay
    of B without A degrades.
  - Pass 2 stub-rowing is a copy of the soroban_contracts pattern.

### Path 2 — CH-idiomatic minimal (preferred by claude)

- Schema:
  - **Drop** `created_at_ledger` from `liquidity_pools` (column removed)
  - Add `last_updated_ledger Int64`
  - Engine: `ReplacingMergeTree(last_updated_ledger) ORDER BY (pool_id)`
- Writer: always emit on every pool change (current behaviour); all
  rows for the same pool have identical content modulo
  `last_updated_ledger`, RMT folds to one row per pool cleanly.
- Read-time recovery of create-ledger via JOIN:
  ```sql
  SELECT pool_id, MIN(ledger_sequence) AS create_ledger
  FROM liquidity_pool_snapshots
  GROUP BY pool_id
  ```
- Pros:
  - Schema is cleanest (pool identity = identity, watermark separate)
  - Writer has no cross-ledger state
  - No Pass 2 stub-rowing complexity
  - No cross-partition gotcha
- Cons:
  - Schema diverges from PG (PG has `created_at_ledger` column)
  - Reads needing create-ledger must JOIN to snapshots (cheap —
    snapshots is partition-indexed)
  - Diverges from "1:1 CH/PG column shape" framing in ADR 0044 §4
    — but ADR 0044 §4a already established `soroban_events` unfold
    as a CH-specific shape, so divergence is precedented.

## Acceptance Criteria

- [ ] Pick Path 1 or Path 2; document the choice + reasoning in an
      ADR 0044 history entry.
- [ ] `crates/db-clickhouse/schema/init.sql` updated:
  - Path 1: column added + engine changed
  - Path 2: column dropped + column added + engine changed
- [ ] `crates/db-clickhouse/src/persist/rows.rs` — `LiquidityPoolRow`
      column order + types match `init.sql` byte-for-byte
      (column-order-pinning test in `persist/tests_cross.rs` updated).
- [ ] `crates/db-clickhouse/src/persist/stage.rs` — staging code
      updated per chosen path:
  - Path 1: cache map + Pass 2 stub-rowing helper
  - Path 2: drop `created_at_ledger` field; ensure
    `last_updated_ledger` is populated from
    `pool.last_updated_ledger` (parser already provides this).
- [ ] Unit tests in `persist/tests_cross.rs`:
  - Re-emit same pool over multiple ledgers in `stage::prepare`,
    assert exactly 1 row in `staged.pool_rows` per pool after the
    background-merge-equivalent (writer dedups within partition).
  - Path-specific: cache hit behaviour (Path 1) OR identity-content
    invariant for same pool across emits (Path 2).
- [ ] Re-run 10k smoke at 62016000–62025999: post-`OPTIMIZE TABLE
  liquidity_pools FINAL`, `count() = uniqExact(pool_id) ≈ 10761`
      (was 217k pre-fix). Read-time query without `LIMIT 1 BY`
      returns one row per pool.
- [ ] ADR 0044 history entry documenting the choice + the schema
      change.
- [ ] `docs/architecture/database-schema/clickhouse-pilot.md` §3
      schema-parity table moves `liquidity_pools` from "immutable
      post-create" to "state (natural version: last_updated_ledger)".
- [ ] `crates/db-clickhouse/README.md` type-translation table and/or
      coercion section updated to match.
- [ ] `lore/1-tasks/active/0206_…/notes/G-coverage-mapping.md` — the
      "ExtractedLiquidityPool → liquidity_pools" section updated to
      reflect new shape.

## Out of Scope

- Performance benchmark of ReplacingMergeTree merge overhead at 11M
  scale — covered by ADR 0044 Q6 ("pilot success criteria") follow-up.
- API read-path updates — task 0207 (CH endpoint queries reference set)
  picks this up.
- PG side — unchanged; PG's
  `ON CONFLICT (pool_id) DO UPDATE` already does the right thing.

## Notes

- `liquidity_pool_snapshots` is **not** affected by this task. It
  stays as append-per-change.
- `lp_positions` shape stays. It already uses
  `ReplacingMergeTree(last_updated_ledger) ORDER BY (pool_id, account_id)`,
  which is the same pattern Path 1 / Path 2 apply here.
- The same-class-of-bug check for other "state" tables in CH:
  `accounts` (RMT last_seen_ledger ✓), `nfts` (RMT current_owner_ledger ✓),
  `soroban_contracts` (RMT wasm_uploaded_at_ledger ✓),
  `lp_positions` (RMT last_updated_ledger ✓), `assets` (plain RMT, no
  natural version; cardinality bounded so no issue),
  `account_balances_current` (RMT last_updated_ledger ✓),
  `liquidity_pools` (**plain MergeTree — this task's target**).
- Effort: small (~half day focused work). Single schema column +
  engine line, single writer field, plus tests + docs. Path 1's
  cross-partition cache is ~30 LOC extra.
