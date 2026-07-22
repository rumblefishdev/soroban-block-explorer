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
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Scope corrected: **`assets.icon_url` is the third dead column** and was missing
      from this task. 0293 moved supply/holders to `balance_aggregates`; `icon_url`
      moved to `asset_enrichment` on the same principle, and `AssetRow::staged`
      writes `None` into all three on every build. Found while auditing state-table
      writers for 0425 — the three NULLs looked like a clobber until the DEAD
      annotations in `init.sql` explained them. Drop all three together; splitting
      them means a second prod ALTER for no reason.
  - date: 2026-06-22
    status: backlog
    who: karolkow
    note: 'Spawned from 0293. Destructive prod cleanup deferred by 0293 (option A backward-compat): drop the now-dead assets.total_supply/holder_count and rebuild ledgers/wasm_interface_metadata as RMT. Neither is applied by CREATE TABLE IF NOT EXISTS, so each needs explicit migration. The additive 0293 rollout runbook lives in 0293 itself; this task is cleanup only, gated on that rollout being verified in prod.'
  - date: 2026-06-24
    status: backlog
    who: stkrolikiewicz
    note: >
      Engine-swap half DONE in prod (out of band, during SAC-redrain backup
      prep). ledgers + wasm_interface_metadata MergeTree -> ReplacingMergeTree
      via create-copy-EXCHANGE-OPTIMIZE FINAL-drop, guarded by a uniqExact
      distinct-key gate. wasm 3760->3720 (40 byte-identical dups collapsed);
      ledgers 12,582,889 unchanged (no dup sequences). snapshot_d backup taken
      first. Remaining: dead-columns drop + asset_aggregates refresh monitoring
      + docs. Task stays open (backlog).
  - date: 2026-07-07
    status: backlog
    who: karolkow
    note: >
      Reality-sync (chq re-verify + PG-retirement fallout). (1) Engine swap
      RE-CONFIRMED in prod 2026-07-07: ledgers + wasm_interface_metadata both
      ReplacingMergeTree (system.tables). (2) TABLE RENAMED: the `asset_aggregates`
      table + `asset_aggregates_mv` this spec references were RETIRED by task
      0331/0339 (ADR 0051 — SACs folded into the wrapped classic asset, unified
      supply per asset). Supply/holders now serve from `balance_aggregates`
      (+ `balance_aggregates_mv`), keyed on the unified `asset_id`, NOT the old
      `(asset_code, issuer_id)`. All `asset_aggregates` refs below updated. (3) §4
      PG aggregate decommission is now MOOT — task 0244 (PG removal) merged
      (PR #319), so `recompute_asset_aggregates` (PG) + the `DataSource::Pg` arm
      are gone. (4) Still-outstanding (chq-confirmed): assets.total_supply +
      holder_count STILL present in prod → the code-strip + ALTER DROP + docs are
      the real remaining work. Monitoring AC may now belong to 0331 (owner of
      balance_aggregates_mv) — flagged below.
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Half of this task is already done — verified on prod 2026-07-22.**
      The engine swap it asks for is in place: `wasm_interface_metadata` is
      already `ReplacingMergeTree`, so that item can be struck.
      The other half stands and is stronger than the task claims: `assets` holds
      **361,015 rows of which exactly 25 carry a non-zero `total_supply` and 25 a
      non-zero `holder_count`**. The live aggregates moved to `balance_aggregates`
      (fed by a refreshable MV off `balances`), so these two columns are dead
      weight on every read of the table. Dropping them is the whole remaining
      scope. Note this is an `ALTER TABLE` on prod, so it needs an ops window.
---

# CH prod cleanup — drop dead assets columns + engine swap (spawned from 0293)

## Summary

The destructive half of the 0293 ClickHouse changes, deferred so the rollout
stays reversible (option A backward-compat). Two independent migrations, both
**after** the additive 0293 rollout is live and verified in prod (rollout
runbook: `0293/README.md` → "Deploy / Migration Runbook"):

1. Drop the now-dead `assets.total_supply` / `assets.holder_count` / `icon_url` (served from
   the `balance_aggregates` table — renamed from `asset_aggregates` by 0331/0339,
   ADR 0051; written `None` by the indexer, read by nothing).
2. Rebuild `ledgers` and `wasm_interface_metadata` as `ReplacingMergeTree`.

Neither reaches prod via `init.sql` — every statement there is `CREATE TABLE IF
NOT EXISTS`, a no-op on an existing table; and `ALTER ... MODIFY ENGINE` does not
exist in ClickHouse, so an engine swap is a create-copy-`EXCHANGE TABLES`-drop.

## Context

- **Dead columns.** 0293 stopped serving `total_supply` / `holder_count` from
  `assets` (clobbered to NULL by the per-ledger indexer — see
  `0293/notes/G-assets-aggregate-clobber-proof.md`) and serves them from a
  pre-computed aggregate table (refreshable MV) instead. That table was
  `asset_aggregates` at 0293; **0331/0339 (ADR 0051) retired it** and now serves
  from `balance_aggregates` (keyed on the unified `asset_id`, folding SAC balances
  into the wrapped classic asset). The dead columns are kept until this task so the
  rollout is reversible.
- **Engine swaps.** `ledgers` (commit marker, live-tail) and
  `wasm_interface_metadata` are `MergeTree` today; 0293's `init.sql` declares them
  `ReplacingMergeTree` so a crash / backfill re-run re-inserting the same key is
  idempotent (last-write-wins on merge, not a duplicate row). A fresh DB gets RMT;
  an existing prod table keeps MergeTree until rebuilt.

## Implementation Plan

> **Prerequisite:** the additive 0293 rollout (aggregate table + refreshable MV
> created + first refresh run, API deployed, read-rows smoke captured, assets flag
> flipped to CH) is live and verified — see `0293/README.md`. **Superseded by
> 0331/0339:** that table is now `balance_aggregates` (+ `balance_aggregates_mv`),
> already live in prod. Take a prod backup (0298 quiesce-backup item) before any
> step here. None of this is reversible.

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

### 3. Monitor the `balance_aggregates` refresh (lore-0293 follow-up)

> **Ownership note (2026-07-07):** the MV is now `balance_aggregates_mv`, created
> by 0331/0339. If those tasks already carry the refresh-monitoring item, drop this
> AC as a duplicate rather than double-wiring the alert. Kept here until confirmed.

The refreshable MV degrades safely on a failed refresh — a failed run leaves the
previous good table intact (stale, never empty) — but there is **no signal** if a
refresh silently stalls (OOM/timeout/lock), so the figures would just age. Wire an
alert on `system.view_refreshes`:

```sql
SELECT view, status, last_success_time, exception
FROM system.view_refreshes
WHERE view = 'balance_aggregates_mv';
```

Alert if `exception != ''` OR `now() - last_success_time > ~10 min` (a few missed
2-minute cycles). Cheap; pairs with the existing CH monitoring.

### 4. (Optional, separate) PG aggregate decommission — DONE (0244 merged)

**No longer applicable.** Task 0244 (full PG removal) merged 2026-07-07 (PR #319):
`recompute_asset_aggregates` (PG), the `DataSource::Pg` arm, and the
`--target postgres` backfill path are all gone. Nothing to decommission here.

## Acceptance Criteria

- [x] 0293 rollout confirmed live + verified in prod (prerequisite) — prod serves
      supply/holders from `balance_aggregates` (0331/0339 successor of the 0293
      `asset_aggregates`); assets read path is all-CH.
- [ ] `balance_aggregates` refresh monitored (`system.view_refreshes` alert on
      `exception` / stale `last_success_time`) — **may be owned by 0331/0339**; drop
      if already wired there.
- [x] `ledgers` and `wasm_interface_metadata` are `ReplacingMergeTree`
      (`SELECT engine FROM system.tables WHERE name IN (...)`), row counts match
      pre-swap (modulo RMT dedup), no data gap at the swap boundary.
      **DONE 2026-06-24; RE-CONFIRMED in prod 2026-07-07 (chq):** both RMT; wasm
      3760->3720 (40 byte-identical dups collapsed), ledgers 12,582,889 unchanged
      (no dup sequences); create-copy-`EXCHANGE`-`OPTIMIZE FINAL`-drop with a
      `uniqExact` distinct-key gate; snapshot_d backup taken first.
- [ ] Indexer no longer references `assets.total_supply` / `holder_count`;
      `ALTER ... DROP COLUMN` applied; `init.sql` + `lib.rs` updated.
- [ ] **Docs updated** — `docs/architecture/database-schema/**` reflects the three
      RMT engines and the dropped `assets` columns (matches prod state).
- [ ] **API types regenerated** — N/A (no API DTO/handler change; the `AssetRow`
      DTO keeps `total_supply`/`holder_count`, now sourced from `balance_aggregates`).
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
