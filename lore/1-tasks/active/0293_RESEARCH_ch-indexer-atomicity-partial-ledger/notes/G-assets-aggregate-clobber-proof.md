---
prefix: G
title: 'Proof — assets aggregate clobber in prod (→ event-driven AggregatingMergeTree)'
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

## The fix — event-driven AggregatingMergeTree (decided + implemented)

`total_supply`/`holder_count` are no longer served from `assets` (the columns are
kept but DEAD — option A backward-compat) and become an **event-driven aggregate**
maintained entirely inside ClickHouse:

- New `account_asset_balance_state` (`AggregatingMergeTree`, ORDER BY
  `(asset_code, issuer_id, account_id)`) holds
  `argMaxState(balance, last_updated_ledger)` per holder. (No `asset_type` in the
  key — it's fixed by the code's length, so functionally determined by
  `asset_code`; the retired batch grouped by `(asset_code, issuer_id)` too.)
- `account_asset_balance_state_mv` populates it incrementally on every
  `account_balances_current` insert (`WHERE asset_type IN (1,2)`) — processes
  only each ledger's delta, **no periodic full scan**, 100% CH-side.
- Reads compute `total_supply = sum(argMaxMerge(latest_balance))` and
  `holder_count = countIf(argMaxMerge(latest_balance) > 0)` per asset in a second
  query scoped to exactly the page's assets
  (`crates/api/src/assets/queries_ch.rs::fill_aggregates`).

**Counted at most once per ledger by construction:** `argMaxState` is idempotent
— re-inserting the same `(balance, last_updated_ledger)` after a crash / backfill
re-run is a no-op (argMax keeps the highest ledger; identical states collapse).
`sumState` would double-count a re-processed ledger; argMax does not. So the
aggregate is correct regardless of how many times the MV fires.

**Why not the alternatives:** a version column on `assets` works (`has_name=0`
makes it cheap today) but keeps two writers fighting over one row and still needs
a schedule for freshness; a refreshable MV is GA but does a periodic full scan.
The AMT is event-driven (no interval, no full scan), CH-native and mature
(argMax + AggregatingMergeTree are core engines), and replaces the retired
one-shot `asset-aggregates` CLI outright (no fallback).

## Prod migration (sequence matters)

1. Apply the new schema: `account_asset_balance_state` + `account_asset_balance_state_mv`.
2. Backfill the AMT from existing balances (create the MV FIRST so the overlap is
   covered — idempotent via argMax):
   ```sql
   INSERT INTO account_asset_balance_state
   SELECT asset_code, issuer_id, account_id,
          argMaxState(balance, last_updated_ledger)
   FROM account_balances_current
   WHERE asset_type IN (1,2)
   GROUP BY asset_code, issuer_id, account_id;
   ```
3. Deploy the new API (reads via `fill_aggregates`). NOTE: `CREATE TABLE IF NOT
   EXISTS` is a no-op on the existing `assets` table → the dead
   `total_supply`/`holder_count` columns stay (option A, backward-compat).
4. **Before the prod flag flip:** read-rows smoke on a mega-holder asset page —
   the scoped AMT `GROUP BY` cost is unmeasured (same quota class as 0290/0198).
5. **Cleanup task (deferred):** `ALTER TABLE assets DROP COLUMN total_supply,
   DROP COLUMN holder_count` + prod engine migration `wasm`/`ledgers`→RMT.
5. Retire the `asset-aggregates` CLI (done in code; remove any cron / runbook ref).

Verified: `cargo check` (lib + `--tests`) green across `db-clickhouse` /
`backfill-runner` / `api`; `init_sql_parses_into_statements` passes (22
statements = 20 tables + 1 MV + 1 dictionary).
