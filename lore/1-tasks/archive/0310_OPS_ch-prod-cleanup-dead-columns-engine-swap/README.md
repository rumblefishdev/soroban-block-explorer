---

id: '0310'
title: 'CH prod cleanup — drop dead assets aggregate columns + MergeTree→ReplacingMergeTree engine swap (wasm_interface_metadata, ledgers)'
type: OPS
status: completed
related_adr: ['0044']
related_tasks: ['0293', '0232', '0298', '0244', '0474']
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
  - docs. Task stays open (backlog).
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
- date: '2026-07-22'
  status: backlog
  who: karolkow
  note: >
  **Two more dead objects found while re-verifying this task. Both belong
  here rather than in new tasks — same operation, same ops window.**
  **1. `assets.icon_url` is emptier than the other two.** Re-measured
  2026-07-22: of **361,232** rows, `total_supply` and `holder_count` carry a
  value on **42**, and `icon_url` on **0**. Not a single row in the table has
  ever had one. It is the same dead-column class already scoped here, so add
  it to the same `ALTER`.
  **2. `account_balances_current` is a dead TABLE, not just dead columns.**
  **0 rows** in prod; no writer — `persist/stage.rs:1648` and
  `persist/writer.rs:110` both record the insert being removed for the
  single-write design; and no live reader, the only surviving mention in
  `crates/api` being a comment about a join that was deleted. The live
  equivalent is `balances`, at **89,634,237 rows**.
  Do not drop it blind, though: task **0214** still lists "`account_balances_current`
  row count > 0" as an acceptance criterion. That criterion is unsatisfiable
  as written and is flagged there for rewriting onto `balances` — but the two
  changes should land in a known order, criterion first, so nobody is left
  chasing a table this task removed.
- date: 2026-07-29
  status: backlog
  who: karolkow
  note: >
  Body corrected to match the history. The engine swap has been recorded as
  done since 2026-06-24 and re-verified three times, but the Summary, the
  Context bullet and Implementation §1 still read as outstanding work — a
  reader who stopped at the body would have scheduled an ops window for a
  migration that ran a month ago. Re-verified on prod today: `ledgers` and
  `wasm_interface_metadata` are both `ReplacingMergeTree`; the three dead
  `assets` columns are still present, so the remaining scope is unchanged.
- date: '2026-08-11'
  status: active
  who: karolkow
  note: >
  Activated for the code-strip half; the prod `ALTER`s stay with the operator.
  Pre-flight re-verified on prod (chq) — full evidence in
  `notes/G-prod-preflight-drop-evidence.md`. Four things the body did not say:
  (1) **`name` is already gone from prod `assets`** (8 columns: identity
  4-tuple + the 3 dead + `id`), so the "DROP COLUMN name batches here" note
  carried in `init.sql` / `rows.rs` was stale — struck.
  (2) **None of the three is in the sorting/primary key**
  (`asset_type, asset_code, issuer_id, contract_id`), so each DROP is a cheap
  metadata op with no re-sort.
  (3) **Two live API readers of `assets.icon_url` existed** — the LP detail and
  LP list queries (`max(a.icon_url)` feeding `asset_{a,b}_icon_url`). They read
  a column that is 0/411,654 populated in prod, i.e. every LP leg icon has
  always been NULL. Re-pointed to `asset_enrichment` (ADR 0050 — the source the
  assets endpoints already use, 11,977 icons), which both unblocks the DROP and
  fixes the silently-empty field.
  (4) **The 40 surviving `total_supply`/`holder_count` rows are pre-0293
  leftovers in the OLD scale** — `assets` held display units,
  `balance_aggregates` holds raw (e.g. APFC 6,500,450,000 vs
  65,004,500,000,000,000). Nothing reads them; not a second source of truth.
- date: '2026-08-12'
  status: completed
  who: karolkow
  note: >
  **DONE — deployed and dropped in prod 2026-08-12.** Code half merged via
  PR #390 (develop) + #391 (master); `deploy-production-compute` at
  08:55 UTC; the three `ALTER TABLE assets DROP COLUMN` ran right after.
  `assets` is now identity-only (5 columns). **One incident, ~9 min ingest
  stall, no data loss:** the planned order (deploy → drain → ALTER later)
  was WRONG for the `clickhouse` 0.15 driver — it validates the row struct
  against `DESCRIBE TABLE` in BOTH directions, so the slimmed `AssetRow`
  failed client-side (`SchemaMismatch`: table columns without DEFAULT not
  covered by the struct) until the columns were actually dropped, and warm
  Lambda containers then kept a cached 8-column DESCRIBE until a
  config-touch recycled them. Recovery: DROPs + container recycle; reconcile
  re-drained the S3 backlog, 157/157 ledgers verified, no gap. Full
  post-mortem under "Issues Encountered". Refresh-monitoring AC confirmed
  NOT owned by 0331/0339 (manual runbook check only) — spawned to 0474.

# CH prod cleanup — drop dead assets columns + engine swap (spawned from 0293)

## Summary

The destructive half of the 0293 ClickHouse changes, deferred so the rollout
stays reversible (option A backward-compat). Two independent migrations, both
**after** the additive 0293 rollout is live and verified in prod (rollout
runbook: `0293/README.md` → "Deploy / Migration Runbook"):

1. ~~Drop the now-dead `assets.total_supply` / `assets.holder_count` /
   `icon_url`~~ **DONE 2026-08-12** (served from the `balance_aggregates` table
   — renamed from `asset_aggregates` by 0331/0339, ADR 0051). Code stripped
   2026-08-11 (PR #390/#391), columns dropped in prod 2026-08-12; `assets` is
   identity-only. See §2 and "Issues Encountered" for the ordering lesson.
2. ~~Rebuild `ledgers` and `wasm_interface_metadata` as `ReplacingMergeTree`.~~
   **DONE in prod 2026-06-24** (out of band, during SAC-redrain backup prep) and
   re-verified 2026-07-07, 2026-07-22 and 2026-07-29 — both tables report
   `ReplacingMergeTree` in `system.tables`. The runbook below is kept as the
   record of how it was done, not as outstanding work.

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
- **Engine swaps — settled.** `ledgers` (commit marker, live-tail) and
  `wasm_interface_metadata` were `MergeTree` when this task was written; 0293's
  `init.sql` declares them `ReplacingMergeTree` so a crash / backfill re-run
  re-inserting the same key is idempotent (last-write-wins on merge, not a
  duplicate row). Both were rebuilt in prod on 2026-06-24 and now match
  `init.sql`. Nothing outstanding here.

## Implementation Plan

> **Prerequisite:** the additive 0293 rollout (aggregate table + refreshable MV
> created + first refresh run, API deployed, read-rows smoke captured, assets flag
> flipped to CH) is live and verified — see `0293/README.md`. **Superseded by
> 0331/0339:** that table is now `balance_aggregates` (+ `balance_aggregates_mv`),
> already live in prod. Take a prod backup (0298 quiesce-backup item) before any
> step here. None of this is reversible.

### 1. Engine swap `ledgers` and `wasm_interface_metadata` → ReplacingMergeTree — DONE 2026-06-24

> Kept as the record of the executed migration. Both tables are
> `ReplacingMergeTree` in prod; do not re-run any of it.

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

### 2. Stop the code writing/reading the dead columns, then drop them — DONE 2026-08-12

> **The ordering assumption below was WRONG and caused a ~9 min ingest stall —
> see "Issues Encountered".** The `clickhouse` 0.15 driver validates the row
> struct against `DESCRIBE TABLE` in both directions: a table column without a
> DEFAULT that the struct does not cover fails the insert client-side
> (`SchemaMismatch`), so "deploy first, ALTER whenever" does not exist as a
> safe state. Deploy + `ALTER` + Lambda-container recycle are ONE window.

1. **Code — DONE (2026-08-11).** See "Implementation Notes" below: `AssetRow` +
   the parser's `ExtractedAsset` stripped, both `INSERT INTO assets` column
   lists shortened, the two LP queries re-pointed off `assets.icon_url`,
   `init.sql` / `lib.rs` / docs updated.
2. **Deploy + drop + recycle — DONE (2026-08-12, operator).** Deploy at
   08:55 UTC, the three `ALTER TABLE assets DROP COLUMN` right after the stall
   was diagnosed, then a config-touch
   (`aws lambda update-function-configuration --description …`) to flush the
   driver's cached DESCRIBE from warm containers.

```sql
ALTER TABLE assets DROP COLUMN total_supply;
ALTER TABLE assets DROP COLUMN holder_count;
ALTER TABLE assets DROP COLUMN icon_url;
```

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
- [ ] `balance_aggregates` refresh monitored — **spawned to task 0474**
      (confirmed 2026-08-12: 0331/0339 carry only a manual runbook check,
      nothing wired in `infra/`; an alert is CDK/CloudWatch work, out of this
      task's scope).
- [x] `ledgers` and `wasm_interface_metadata` are `ReplacingMergeTree`
      (`SELECT engine FROM system.tables WHERE name IN (...)`), row counts match
      pre-swap (modulo RMT dedup), no data gap at the swap boundary.
      **DONE 2026-06-24; RE-CONFIRMED in prod 2026-07-07 (chq):** both RMT; wasm
      3760->3720 (40 byte-identical dups collapsed), ledgers 12,582,889 unchanged
      (no dup sequences); create-copy-`EXCHANGE`-`OPTIMIZE FINAL`-drop with a
      `uniqExact` distinct-key gate; snapshot_d backup taken first.
- [x] No code references `assets.total_supply` / `holder_count` / `icon_url`
      any more — writers, readers and `init.sql` all stripped (2026-08-11);
      `lib.rs` note updated. The only surviving mentions are Postgres-era
      (`audit-harness`), dead since 0244.
- [x] `ALTER … DROP COLUMN` applied in prod — ran 2026-08-12 ~09:00 UTC via
      the operator's write CLI, verified: `system.columns` shows 5 columns
      (identity 4-tuple + `id`), matching `AssetRow` exactly.
- [x] **Docs updated** — `database-schema-overview.md` (assets DDL + icon
      paragraph), `endpoint-queries-clickhouse/README.md` (engine table),
      `indexing-pipeline-overview.md`, `enrichment.md`,
      `xdr-parsing-overview.md`. `technical-design-general-overview.md` left
      alone: it is the original PG-dialect design spec (still lists `sac`,
      `description`, `home_page`), already historical rather than evergreen.
- [x] **API types regenerated** — ran
      `npx nx run @rumblefish/api-types:generate`; the only diff is the
      `PoolAssetLeg.icon_url` doc comment. No DTO shape change: the leg still
      exposes `icon_url`, and `AssetRow` keeps `total_supply`/`holder_count`
      sourced from `balance_aggregates`.

## Implementation Notes (code half, 2026-08-11)

Pre-flight evidence: `notes/G-prod-preflight-drop-evidence.md`.

**Writers** — `AssetRow` (`persist/rows.rs`) lost the three fields, so the
generated INSERT column list shortens; every build site already went through
`AssetRow::staged()`, so no call site changed. Two hand-written
`INSERT INTO assets (…)` lists shortened as well:
`backfill-runner/src/contract_type_rebuild.rs` (type-3 rebuild) and a
`backfill-enrichment-runner` test seed, plus the `db-clickhouse` smoke /
metadata E2E seeds and the `column_order_assets` drift test.

**Parser** — `ExtractedAsset.total_supply` / `.holder_count` were `None` at
every one of their construction sites (they only ever fed the dead columns), so
they went too, along with the doc comments promising a future
`recompute_asset_aggregates` write-back that 0244 deleted.

**Readers** — the LP detail and LP list queries were the only live consumers of
`assets.icon_url`. Re-pointed to `asset_enrichment`
(`argMax(icon_url, version)`, same key 4-tuple, same predicate as the
surrounding `asset_sac` join) — the source the assets endpoints already use.

**Verification** — `cargo check --workspace --all-targets`, `cargo clippy`
(clean), `cargo test -p db-clickhouse -p xdr-parser -p api` (all green), and the
re-written LP icon CTE executed against **prod** for pool
`41c3e3d9…` : leg `XRPBANK` now returns `https://xrpb.global/xrpbank.png` where
the old `max(a.icon_url)` returned NULL. Confirms both that ClickHouse accepts
the CTE-inside-join-subquery shape and that the new source has data.

## Issues Encountered

- **~9 min prod ingest stall (2026-08-12 08:55–09:04 UTC), no data loss.**
  The runbook assumed the driver sends a shortened INSERT column list and the
  dropped-from-struct columns simply take their NULL default, making
  "deploy → drain → ALTER later" safe. False for `clickhouse` 0.15: before
  inserting it runs `DESCRIBE TABLE` and validates both directions
  (`row_metadata.rs` — every table column the struct does not cover must have
  a DEFAULT; plain `Nullable(…)` does not qualify). The moment the slimmed
  indexer went live, every reconcile failed client-side with
  `SchemaMismatch` (logged as sanitised `"ClickHouse error"`;
  `system.query_log` stayed clean, which is what pointed at the client).
- **Warm-container metadata cache prolonged it past the DROPs.** The driver
  caches DESCRIBE per client; the crate docs require
  `clear_cached_metadata()` after a schema change. Warm Lambda environments
  kept failing on the cached 8-column shape until a no-op config change
  (`--description`) forced new execution environments.
- **Recovery was clean by design:** the doorbell/reconcile architecture
  (task 0241) redelivered via SQS, ledgers waited on S3, and the cursor
  resumed in order — verified 157/157 ledgers in the stall window, lag back
  to single-digit seconds. The API Lambda was unaffected throughout (its
  reads never touched the dropped columns after the re-point).
- **`origin/develop` auto-deleted by the release merge.** GitHub's
  delete-head-branch automation removed `develop` when PR #391
  (develop→master) merged. Nothing lost (fully contained in master + local
  clones); restored by pushing local `develop` back.

## Design Decisions

### From Plan

1. **Code first, `ALTER` second.** The INSERT column list is explicit, so the
   drop can only follow a fully drained deploy.

### Emerged

2. **Re-point the LP icons instead of nulling them out.** The DROP had to break
   the tie: either delete the field from the LP DTO (an API contract change) or
   move it to the table that actually holds icons. Chose the move — same field,
   no contract change, and it turns a permanently-NULL field into a populated
   one. The alternative (hardcoding NULL) would have been a misleading
   fallback.
3. **Strip the parser's two dead fields as well.** Not in the original scope,
   which stopped at the CH columns — but with the columns gone, `ExtractedAsset`
   would have carried two fields that no producer sets and no consumer reads.
   Removing them keeps producer and schema telling the same story.
4. **Left the Postgres-era `audit-harness` references alone** — `10_assets.sql`
   (psql `\echo`, PG constraints) and `horizon-diff.rs` (`sqlx::PgPool`) still
   name the columns, but PG was retired by 0244 and neither can run.
5. **Left `technical-design-general-overview.md` alone** — see the docs AC.

## Future Work

- `domain::Asset` (`crates/domain/src/asset.rs`) has no references anywhere in
  the workspace — a PG-era struct (`id: i32`, `NUMERIC(28,7)` doc) carrying the
  same dead fields. Deletion candidate, but out of this task's blast radius.
- `account_balances_current` (0 rows, no writer/reader) still waits on task
  0214's acceptance criterion being rewritten onto `balances` first.

## Notes

- **Why separate from 0298?** 0298 (`ch-atomicity-hardening`) is broad hardening
  research (orphan guards, insert-dedup tokens, restore drills). This is a
  concrete ordered migration checklist with a hard prerequisite — kept separate.
- **Why separate from 0293?** 0293 carries the additive rollout (its own change);
  this is the deferred destructive cleanup, gated on that rollout. Splitting keeps
  the reversible part and the irreversible part on distinct changes.
- Evidence + aggregate design: `0293/notes/G-assets-aggregate-clobber-proof.md`.
  Per-table engine/PK/version inventory: `0293/README.md`.
