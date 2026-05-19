---
id: '0232'
title: 'FEATURE: ClickHouse assets aggregates live-refresh via REFRESHABLE MATERIALIZED VIEW'
type: FEATURE
status: backlog
related_adr: ['0044']
related_tasks: ['0194', '0228']
blocked_by: ['0228']
tags:
  [
    priority-medium,
    effort-small,
    layer-data,
    clickhouse,
    enrichment,
    materialized-view,
    live-mode,
  ]
links: []
history:
  - date: '2026-05-18'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from the 2026-05-18 CH-enrichment planning session, after
      noting that `asset_aggregates.rs` (task 0228 Stage 1) is a
      post-merge snapshot only — it does not maintain
      `assets.{holder_count, total_supply}` under live ingestion. PG
      keeps these fresh via inline `recompute_asset_aggregates` on every
      per-ledger commit; CH live writer cannot afford the equivalent
      JOIN against `account_balances_current` per partition. After the
      Hetzner go-live, these two columns will drift from the live state
      unless a periodic recompute runs. This task picks the cheapest
      live-mode fix that **does not alter the `assets` schema**:
      a CH refreshable materialized view targeting the existing table.
---

# FEATURE: ClickHouse `assets` aggregates live-refresh via REFRESHABLE MATERIALIZED VIEW

## Summary

Replace the manual `backfill-runner asset-aggregates` invocation (task
0228 Stage 1) with a CH-native `REFRESHABLE MATERIALIZED VIEW` that
re-runs the same aggregate query on a schedule and atomically writes
the new state back into the existing `assets` table. Keeps `assets`
DDL unchanged — no version column, no separate aggregates table, no
API read-path changes.

## Status: backlog

Blocked on task 0228 landing on Hetzner (we need the production CH
operational + the Stage 1 baseline snapshot in place before adding
the live-refresh layer).

## Context

Today on PG, `crates/indexer/src/handler/persist/write.rs:2659`
(`recompute_asset_aggregates`) runs inline per-ledger inside the same
transaction as the balance writes — `assets.{holder_count,
total_supply}` are always fresh.

On CH the live writer batches per-partition; a JOIN against a
~50M-row `account_balances_current` on every commit is not affordable.
The Stage 1 `asset-aggregates` subcommand built today
(`crates/backfill-runner/src/asset_aggregates.rs`) handles the
post-merge snapshot but is a one-shot. After Hetzner go-live, the
columns will drift from the live state unless someone re-runs the
subcommand periodically.

A refreshable materialized view is the smallest moving part that gives
us "scheduled recompute + atomic visibility" without a writer change,
without a schema change, and without an API change.

## Recommended approach — Variant A (target = existing `assets`)

```sql
CREATE MATERIALIZED VIEW assets_aggregates_refresh
REFRESH EVERY 1 HOUR
TO assets
AS
SELECT
    a.asset_type,
    a.asset_code,
    a.issuer_id,
    a.contract_id,
    a.name,
    if(a.asset_type IN (1, 2),
       CAST(ifNull(agg.total_supply, toDecimal128(0, 7)) AS Decimal128(7)),
       a.total_supply) AS total_supply,
    if(a.asset_type IN (1, 2),
       CAST(ifNull(agg.holder_count, 0) AS Int32),
       a.holder_count) AS holder_count,
    a.icon_url
  FROM assets FINAL AS a
  LEFT JOIN (
      SELECT
          asset_code,
          issuer_id,
          countIf(balance > 0) AS holder_count,
          sum(balance) AS total_supply
        FROM account_balances_current FINAL
       WHERE asset_type IN (1, 2)
       GROUP BY asset_code, issuer_id
  ) AS agg ON agg.asset_code = a.asset_code AND agg.issuer_id = a.issuer_id;
```

Mechanics:

- CH 23.12+ feature. Hetzner runs CH 26.3 (per task 0216 hardware
  spec) → supported.
- `REFRESH EVERY 1 HOUR` schedules the SELECT.
- `TO assets` inserts the result rows back into the existing table.
  `assets` is `ReplacingMergeTree` ordered by
  `(asset_type, asset_code, issuer_id, contract_id)` — duplicates
  on the same PK collapse on background merge; last insert wins.
- The SELECT is byte-for-byte the query already implemented in
  `asset_aggregates.rs`; this task is "move it from imperative
  subcommand to declarative MV".

### Why not change `assets` schema

The 0228 task notes already flag the schema-engine-swap follow-up
(`AggregatingMergeTree` with `SimpleAggregateFunction`) as out of
scope and large in cost. This task explicitly stays cheaper than
that — same `assets` shape, same writer path, same reads, just a
scheduled refresh layer.

### Open questions / caveats

1. **RMT-without-version collapse semantics**: `assets` has no
   version column. On collapse CH keeps "an arbitrary" row per PK;
   in practice the last-inserted row wins because background merger
   processes parts in insertion order, but this is not formally
   guaranteed. Mitigations:
   - (a) `OPTIMIZE TABLE assets FINAL` cron after every MV refresh
     (cheapest)
   - (b) Add `last_refreshed_ledger` Int64 version column to `assets`
     → tiny schema change, defeats the "no schema change" property
   - (c) Read-side `FINAL` everywhere — already part of the
     no-`FINAL`-at-query-time invariant exception list (ADR 0044
     allows FINAL on `assets` per §X if needed)
2. **Refresh interval choice**: 1 h vs 15 min vs 5 min.
   `account_balances_current FINAL` full scan at mainnet scale is the
   cost driver — measure on Hetzner post-go-live and pick the
   tightest interval that stays under N% CPU budget.
3. **Atomic visibility**: refreshable MV with `TO existing_table`
   does plain `INSERT`s — between refresh complete and next OPTIMIZE
   the table holds both old and new versions for each PK. Reads via
   `FINAL` see the new version immediately; non-`FINAL` reads see
   whichever part the granule scan hits first. For API endpoints
   that already obey no-`FINAL` (ADR 0044), this is a behavioural
   change worth confirming or routing through a view.

## Implementation Plan

### Step 1 — Schema migration

Add the `CREATE MATERIALIZED VIEW assets_aggregates_refresh ... AS ...`
DDL to `crates/db-clickhouse/schema/init.sql` (idempotent
`CREATE MATERIALIZED VIEW IF NOT EXISTS`). Add the `OPTIMIZE TABLE
assets FINAL` cron entry to the Hetzner runbook (every refresh + 5 min
buffer).

### Step 2 — Retire (or alias) `asset-aggregates` subcommand

Keep the subcommand as an operator escape hatch (manual re-run if MV
falls behind) but document the MV as the production source of truth.

### Step 3 — Sanity probe

Compare `assets.holder_count` and `assets.total_supply` on CH against
PG for the top-100 assets by activity. Tolerance: < 0.1% drift inside
the refresh window. Use the existing `compare-with-stellar-api` skill
extended with PG↔CH parity checks.

### Step 4 — Verify no-FINAL-at-query-time invariant

The API endpoints reading `assets` must either tolerate seeing both
the old and the new row briefly post-refresh, or query through a
`FINAL`-using view. Audit
`crates/api/src/assets/queries.rs` and adjust if needed.

## Acceptance Criteria

- [ ] `CREATE MATERIALIZED VIEW assets_aggregates_refresh ... TO assets ...`
      landed in `init.sql` and the migration applies cleanly to Hetzner.
- [ ] `OPTIMIZE TABLE assets FINAL` cron documented in the Hetzner
      ops runbook.
- [ ] Refresh-interval measurement on Hetzner production CH; the
      committed value is justified in this task's notes (cost vs
      staleness trade-off).
- [ ] PG ↔ CH parity probe shows < 0.1% drift on top-100 assets at
      steady state (after the refresh interval lapses).
- [ ] API endpoints reading `assets.{holder_count, total_supply}`
      either pass through the audit (no FINAL needed) or get an
      explicit view fix.
- [ ] **Docs updated** — `docs/architecture/data-pipeline/` gets a
      section describing the assets-aggregates refresh layer alongside
      the PG inline recompute. The CH schema overview gains a
      "Refreshable MV" subsection.
- [ ] **API types regenerated** — N/A unless the read-path audit
      forces an API shape change; default expectation is no API
      change.

## Alternatives considered (not chosen)

- **Cron `backfill-runner asset-aggregates`** — operationally
  identical but split across CH config (cron) + runner code; harder
  to reason about. MV declares the contract in one place.
- **Incremental MV with `AggregatingMergeTree`** — would keep the
  aggregate state incrementally, no full scan per refresh. Powerful
  but requires `holder_count` to use a state-based aggregate function
  (`uniqExact-State` over `account_id`s with `balance > 0`), which
  needs a custom AggregateFunction column and meaningful Rust-side
  decoder work. Cost > value for this task; revisit if the
  refreshable-MV scan cost ever exceeds budget.
- **Full schema-engine-swap to `AggregatingMergeTree(SimpleAggregateFunction…)`** —
  the long-term follow-up flagged in task 0228 notes. Eliminates
  Tier-1 repair columns at the storage-engine level. Big change;
  out of scope here.

## Notes

- This task is a leaf optimisation, not a blocker for go-live. Stage 1
  (manual one-shot `asset-aggregates`) is the go-live gate; this task
  smooths the live-mode steady state afterward.
- Same approach (refreshable MV) **does not apply** to the other 6
  Stage 1 columns — those are monotone (`MIN(ledger)` columns and
  one-shot deploy info) and don't drift in live mode.
- If we later land the schema-engine-swap proposal, this MV becomes
  redundant and can be dropped.
