---
prefix: G
title: 'Proof — assets aggregate clobber in prod (→ pre-computed asset_aggregates via refreshable MV)'
status: mature
spawned_from: '0293'
date: '2026-06-17'
who: karolkow
---

# Proof — `assets.total_supply` / `holder_count` clobbered in prod

## The bug

`assets` is written by TWO paths:
1. **Per-ledger indexer** (`crates/db-clickhouse/src/persist/stage.rs:869,897,914`)
   — emits the asset identity row with `total_supply = None, holder_count = None`
   on EVERY ledger that touches the asset (`asset_seen` HashSet is batch-local,
   no DB visibility → re-emitted, not gated on novelty).
2. **Batch recompute** (`crates/backfill-runner/src/asset_aggregates.rs`) —
   `sum(balance)` / `countIf(balance>0)` over `account_balances_current FINAL`,
   published via `EXCHANGE TABLES`.

`assets` is `ReplacingMergeTree` with **NO version column**, so among rows with
the same identity key RMT keeps the LAST-inserted on merge. The indexer's
`None`-aggregate row, inserted after the batch ran, **wins** → the batch's
computed value is reverted to `NULL`. ClickHouse has no partial `UPDATE`, so the
indexer can't write "identity only" — every INSERT carries all columns. (On
Postgres this was a safe inline `UPDATE assets SET total_supply, holder_count`;
the PG→CH port lost that.)

## Evidence (prod ClickHouse, 2026-06-17, via `chq`)

```sql
SELECT count() AS total,
       countIf(asset_type IN (1,2)) AS classic_sac,
       countIf(asset_type IN (1,2) AND total_supply IS NULL) AS null_supply
FROM assets FINAL
```
Result: `total = 319210`, `classic_sac = 319207`, `null_supply = 79207`.

**Why this proves clobber:** for `asset_type IN (1,2)` the batch writes
`ifNull(sum, 0)` — minimum `0`, **never NULL** (`asset_aggregates.rs:99-103`).
So a classic/SAC asset with `total_supply = NULL` can only mean the batch's row
is NOT the merge winner — the indexer's `None` row won. **79,207 of 319,207
(~25%) classic/SAC assets currently show NULL aggregates in prod.** The API
serves NULL supply/holders for those.

Sample (all `asset_type=1` classic, NULL supply + NULL holders):
`yETH`, `yUSDC`, `yXLM`, `xrp777`, `xrpAIG`.

### Stronger evidence (rules out "legitimately empty")

A plain NULL count can't tell "value lost" from "no holders". These two queries
remove the doubt — they prove the served data is WRONG (a real value exists and
is not shown):

```sql
-- Q-A: NULL-supply classic/SAC assets that DO have active holders
SELECT count() FROM
  (SELECT asset_code, issuer_id FROM assets FINAL
     WHERE asset_type IN (1,2) AND total_supply IS NULL) a
INNER JOIN
  (SELECT asset_code, issuer_id FROM account_balances_current FINAL
     WHERE asset_type IN (1,2) AND balance > 0 GROUP BY asset_code, issuer_id) h
USING (asset_code, issuer_id)
-- → 75816
```
**75,816** of the 79,207 NULL-supply assets (96%) have active holders — their
supply is computable from current balances yet served as NULL.

```sql
-- Q-B: concrete asset yUSDC (issuer_id 1796653227778802488)
SELECT countIf(balance > 0) AS real_holders, toString(sum(balance)) AS real_supply
FROM account_balances_current FINAL
WHERE asset_type IN (1,2) AND asset_code = 'yUSDC' AND issuer_id = 1796653227778802488
-- → real_holders = 10085, real_supply = 2845935.987509
```
`assets` serves `total_supply = NULL` for `yUSDC`, while it actually has **10,085
holders** and **2,845,935.99 supply** right now. Knowable, and wrong.

## The fix — pre-computed per-asset table via a refreshable MV (decided + implemented)

`total_supply`/`holder_count` are no longer served from `assets` (the columns are
kept but DEAD — option A backward-compat) and become a **pre-computed per-asset
table** maintained entirely inside ClickHouse:

- New `asset_aggregates` (plain `MergeTree`, ORDER BY `(asset_code, issuer_id)`) —
  one row per classic/SAC asset with the FINAL `total_supply` / `holder_count`
  ready to read. Columns are `Nullable` so a read-side LEFT-JOIN miss (native /
  soroban — no row) decodes as NULL under the readonly `api_reader`
  (`join_use_nulls = 0` fills a Nullable column with its default, which IS NULL),
  while a real 0-supply asset (has a row) reads 0 — no sentinel, no `nullIf`. (No
  `asset_type` in the key — fixed by the code's length, functionally determined by
  `asset_code`; the retired batch grouped by `(asset_code, issuer_id)` too.)
- `asset_aggregates_mv` is a **refreshable** MV (`REFRESH EVERY 2 MINUTE`) that
  recomputes the whole table from `account_balances_current FINAL`
  (`sum(balance)`, `countIf(balance > 0)`, `WHERE asset_type IN (1,2) GROUP BY
  asset_code, issuer_id`). 100% CH-side. The refresh is a batch admin job, not
  subject to the `api_reader` read quota.
- Reads are a trivial 1:1 `LEFT JOIN asset_aggregates` in `ASSET_CH_SELECT`
  (`crates/api/src/assets/queries_ch.rs`) — no read-time GROUP BY, exactly like
  the `asset_enrichment` join.

**Idempotent by construction:** each refresh is a full recompute-and-replace from
the current `account_balances_current` state, so an indexer crash / backfill
re-run can't corrupt the figures — the next refresh reflects whatever the balances
now say. Tradeoff: eventual consistency — figures lag by up to the refresh
interval (tunable).

**Why this over the alternatives:** an incremental per-asset sum can't be kept
from an absolute current-state source without either (a) per-holder state + a
read-time GROUP BY (an `AggregatingMergeTree` with `argMaxState` — correct + fresh
to the ledger, but the read sums the page's holders), or (b) the indexer tracking
balance deltas (read-modify-write, which breaks the absolute-state idempotency
THIS task validated). The refreshable MV trades to-the-ledger freshness for the
simplest correct read (a ready 1:1 join) plus a periodic batch recompute. It
replaces the retired one-shot `asset-aggregates` CLI outright (no fallback).

## Prod migration (sequence matters)

1. Apply the new schema: `asset_aggregates` + `asset_aggregates_mv`.
2. Trigger the first refresh (the table is empty until the MV runs) — no manual
   backfill INSERT; the refresh computes everything from `account_balances_current`:
   ```sql
   SYSTEM REFRESH VIEW asset_aggregates_mv;
   SYSTEM WAIT VIEW    asset_aggregates_mv;  -- block until it finishes
   ```
3. Deploy the new API (reads via the `asset_aggregates` LEFT JOIN). NOTE:
   `CREATE TABLE IF NOT EXISTS` is a no-op on the existing `assets` table → the
   dead `total_supply`/`holder_count` columns stay (option A, backward-compat).
4. **Before the prod flag flip:** read-rows smoke on a mega-holder asset page.
   Lower risk than an AMT — the read is a 1:1 join against a small per-asset table
   (no read-time `GROUP BY` over holders); the heavy `GROUP BY` lives in the
   refresh (an admin job, off the `api_reader` quota).
5. **Cleanup task (deferred, 0310):** `ALTER TABLE assets DROP COLUMN total_supply,
   DROP COLUMN holder_count` + prod engine migration `wasm`/`ledgers`→RMT.
6. Retire the `asset-aggregates` CLI (kept through the develop merge; removal
   moved to 0310).

**Freshness note:** figures lag by up to `REFRESH EVERY` (2 min as written) —
eventually consistent, not to-the-ledger. Tune the interval if needed (pure
schema change).

Verified: `cargo check --workspace --tests` green; `init_sql_parses_into_statements`
passes (24 statements = 22 tables + 1 materialized view + 1 dictionary).
