---
id: '0310'
title: 'CH prod cleanup — drop dead assets aggregate columns + MergeTree→ReplacingMergeTree engine swap (wasm_interface_metadata, ledgers)'
type: OPS
status: backlog
related_adr: ['0044']
related_tasks: ['0293', '0232', '0298', '0244']
tags:
  [
    'phase-future',
    'effort-medium',
    'priority-medium',
    'clickhouse',
    'migration',
  ]
history:
  - date: 2026-06-22
    status: backlog
    who: karolkow
    note: 'Spawned from 0293. Destructive prod cleanup deferred by 0293 (option A backward-compat): drop the now-dead assets.total_supply/holder_count and rebuild ledgers/wasm_interface_metadata as RMT. Neither is applied by CREATE TABLE IF NOT EXISTS, so each needs explicit migration. The additive 0293 rollout runbook lives in 0293 itself; this task is cleanup only, gated on that rollout being verified in prod.'
---

# CH prod cleanup — drop dead assets columns + engine swap (spawned from 0293)

## Summary

The destructive half of the 0293 ClickHouse changes, deferred so the rollout
stays reversible (option A backward-compat). Two independent migrations, both
**after** the additive 0293 rollout is live and verified in prod (rollout
runbook: `0293/README.md` → "Deploy / Migration Runbook"):

1. Drop the now-dead `assets.total_supply` / `assets.holder_count` (served from
   the `asset_aggregates` table since 0293; written `None` by the indexer, read by nothing).
2. Rebuild `ledgers` and `wasm_interface_metadata` as `ReplacingMergeTree`.

Neither reaches prod via `init.sql` — every statement there is `CREATE TABLE IF
NOT EXISTS`, a no-op on an existing table; and `ALTER ... MODIFY ENGINE` does not
exist in ClickHouse, so an engine swap is a create-copy-`EXCHANGE TABLES`-drop.

## Context

- **Dead columns.** 0293 stopped serving `total_supply` / `holder_count` from
  `assets` (clobbered to NULL by the per-ledger indexer — see
  `0293/notes/G-assets-aggregate-clobber-proof.md`) and serves them from the
  pre-computed `asset_aggregates` table (refreshable MV) instead. The columns are kept until this task so the rollout
  is reversible.
- **Engine swaps.** `ledgers` (commit marker, live-tail) and
  `wasm_interface_metadata` are `MergeTree` today; 0293's `init.sql` declares them
  `ReplacingMergeTree` so a crash / backfill re-run re-inserting the same key is
  idempotent (last-write-wins on merge, not a duplicate row). A fresh DB gets RMT;
  an existing prod table keeps MergeTree until rebuilt.

## Implementation Plan

> **Prerequisite:** the additive 0293 rollout (`asset_aggregates` table + refreshable MV created + first refresh run, API
> deployed, read-rows smoke captured, assets flag flipped to CH) is live and
> verified — see `0293/README.md`. Take a prod backup (0298 quiesce-backup item)
> before any step here. None of this is reversible.

### 1. Engine swap `ledgers` and `wasm_interface_metadata` → ReplacingMergeTree

Per table: create-copy-`EXCHANGE`-catch-up-drop. `EXCHANGE TABLES` is an atomic
metadata swap (Atomic db engine), so the live-tail gap is only the rows inserted
between the snapshot and the swap — re-inserting them into the RMT is idempotent
(dedup by key on merge). Example for `ledgers` (mirror for
`wasm_interface_metadata`):

```sql
-- a. new table, identical columns, RMT engine + same ORDER BY/PARTITION as init.sql
CREATE TABLE ledgers_rmt AS ledgers ENGINE = ReplacingMergeTree
  PARTITION BY intDiv(sequence, 500000) ORDER BY sequence;   -- match init.sql exactly

-- b. snapshot copy
INSERT INTO ledgers_rmt SELECT * FROM ledgers;

-- c. atomic swap (sub-ms; new indexer writes land in the RMT after this)
EXCHANGE TABLES ledgers AND ledgers_rmt;

-- d. catch-up: rows that hit the old table between (b) and (c) are now in
--    ledgers_rmt; re-insert the recent tail, RMT dedups on merge
INSERT INTO ledgers
SELECT * FROM ledgers_rmt
WHERE sequence >= (SELECT max(sequence) FROM ledgers) - 100000;

-- e. drop the old table (now named ledgers_rmt)
DROP TABLE ledgers_rmt;
```

- Re-declare the engine/ORDER BY/PARTITION to **exactly** match `init.sql` —
  `CREATE TABLE ... AS ledgers` copies columns, **not** the engine.
- `wasm_interface_metadata` is low-write (no live-tail pressure) — the catch-up
  can be a full `INSERT ... SELECT` or skipped behind a brief write pause.
- Optional `OPTIMIZE TABLE ... FINAL` after the swap forces the dedup merge so
  non-`FINAL` reads are clean sooner.

### 2. Stop the indexer writing the dead columns, then drop them

Order matters — the ClickHouse client names every struct field in the INSERT
column list, so dropping a column the indexer still INSERTs → `INSERT` fails
(`No such column`).

1. Code: remove `total_supply` / `holder_count` from `AssetRow`
   (`crates/db-clickhouse/src/persist/rows.rs`) and the three construction sites
   in `crates/db-clickhouse/src/persist/stage.rs` (they currently write `None`).
2. Deploy the indexer; let old Lambda versions drain.
3. Drop the columns (cheap metadata op):

```sql
ALTER TABLE assets DROP COLUMN total_supply;
ALTER TABLE assets DROP COLUMN holder_count;
```

4. Remove `total_supply` / `holder_count` from `init.sql`'s `assets` DDL and from
   the `crates/db-clickhouse/src/lib.rs` statement-count comment (count unchanged
   — still 22 — but the dead-column note goes).

### 3. Monitor the `asset_aggregates` refresh (lore-0293 follow-up)

The refreshable MV degrades safely on a failed refresh — a failed run leaves the
previous good table intact (stale, never empty) — but there is **no signal** if a
refresh silently stalls (OOM/timeout/lock), so the figures would just age. Wire an
alert on `system.view_refreshes`:

```sql
SELECT view, status, last_success_time, exception
FROM system.view_refreshes
WHERE view = 'asset_aggregates_mv';
```

Alert if `exception != ''` OR `now() - last_success_time > ~10 min` (a few missed
2-minute cycles). Cheap; pairs with the existing CH monitoring.

### 4. (Optional, separate) PG aggregate decommission

Only once the PG assets path is formally retired (task 0244).
`recompute_asset_aggregates` (PG) is **alive** today (API `DataSource::Pg` +
backfill `--target postgres`); do NOT remove it here. Listed only for trail
completeness.

## Acceptance Criteria

- [ ] 0293 rollout confirmed live + verified in prod (prerequisite).
- [ ] `asset_aggregates` refresh monitored (`system.view_refreshes` alert on
      `exception` / stale `last_success_time`).
- [ ] `ledgers` and `wasm_interface_metadata` are `ReplacingMergeTree`
      (`SELECT engine FROM system.tables WHERE name IN (...)`), row counts match
      pre-swap (modulo RMT dedup), no data gap at the swap boundary.
- [ ] Indexer no longer references `assets.total_supply` / `holder_count`;
      `ALTER ... DROP COLUMN` applied; `init.sql` + `lib.rs` updated.
- [ ] **Docs updated** — `docs/architecture/database-schema/**` reflects the three
      RMT engines and the dropped `assets` columns (matches prod state).
- [ ] **API types regenerated** — N/A (no API DTO/handler change; the `AssetRow`
      DTO keeps `total_supply`/`holder_count`, now sourced from `asset_aggregates`).
      Re-confirm at execution time if the diff touches `crates/api/**`.

## Notes

- **Why separate from 0298?** 0298 (`ch-atomicity-hardening`) is broad hardening
  research (orphan guards, insert-dedup tokens, restore drills). This is a
  concrete ordered migration checklist with a hard prerequisite — kept separate.
- **Why separate from 0293?** 0293 carries the additive rollout (its own change);
  this is the deferred destructive cleanup, gated on that rollout. Splitting keeps
  the reversible part and the irreversible part on distinct changes.
- Evidence + aggregate design: `0293/notes/G-assets-aggregate-clobber-proof.md`.
  Per-table engine/PK/version inventory: `0293/README.md`.
