---
type: G
title: 'Prod pre-flight — evidence that assets.{total_supply,holder_count,icon_url} are safe to DROP'
status: mature
spawned_from: '0310'
date: '2026-08-11'
---

# Pre-flight evidence — prod, 2026-08-11 (chq, read-only)

Purpose: answer "can we actually drop these three columns?" with measurements
rather than the task's inherited prose. Everything below is a read against prod
ClickHouse; no statement here mutates anything.

## 1. Shape of the table

```sql
SELECT name, type FROM system.columns
WHERE database = currentDatabase() AND table = 'assets' ORDER BY position;
```

| #   | column         | type                       |
| --- | -------------- | -------------------------- |
| 1   | `asset_type`   | `Int16`                    |
| 2   | `asset_code`   | `LowCardinality(String)`   |
| 3   | `issuer_id`    | `Int64`                    |
| 4   | `contract_id`  | `Int64`                    |
| 5   | `total_supply` | `Nullable(Decimal(38, 7))` |
| 6   | `holder_count` | `Nullable(Int32)`          |
| 7   | `icon_url`     | `Nullable(String)`         |
| 8   | `id`           | `Int64`                    |

Two corrections to the task body fall out of this:

- **`name` no longer exists in prod.** Both `init.sql` and `rows.rs` carried a
  note saying the prod `DROP COLUMN name` "batches with 0310's assets
  deploy-drain window". It does not — it already ran. Notes struck.
- **`id` exists in prod**, so the 0331 `ADD COLUMN id` + backfill note in
  `init.sql` is also historical.

## 2. The DROP is a metadata op, not a re-sort

```sql
SELECT sorting_key, primary_key FROM system.tables
WHERE database = currentDatabase() AND name = 'assets';
```

```
sorting_key: asset_type, asset_code, issuer_id, contract_id
primary_key: asset_type, asset_code, issuer_id, contract_id
```

None of the three columns participates in the key, so `ALTER TABLE assets DROP
COLUMN …` is cheap and cannot invalidate the ordering. (`id` is not in the key
either — it is the 0331 surrogate, referenced by `balances.asset_id`.)

## 3. How dead is "dead"

```sql
SELECT count() AS rows,
       countIf(total_supply IS NOT NULL) AS ts_nn,
       countIf(holder_count IS NOT NULL) AS hc_nn,
       countIf(icon_url     IS NOT NULL) AS icon_nn
FROM assets;          -- and again with FINAL
```

|                         | raw parts | `FINAL` (deduped) |
| ----------------------- | --------- | ----------------- |
| rows                    | 411,654   | 343,816           |
| `total_supply` non-NULL | 42        | 40                |
| `holder_count` non-NULL | 42        | 40                |
| `icon_url` non-NULL     | **0**     | **0**             |

`icon_url` has never held a value in prod — not one row out of 411k.

## 4. The 40 survivors are pre-0293 leftovers, in the old scale

Joining them to the live aggregate (`assets.id = balance_aggregates.asset_id`):

| asset    | `assets.total_supply` | `balance_aggregates.total_supply` | holders (both) |
| -------- | --------------------- | --------------------------------- | -------------- |
| APFC     | 6,500,450,000         | 65,004,500,000,000,000            | 2              |
| AUDICOIN | 10,000.0487804        | 100,000,487,804                   | 1              |
| BICO     | 7,500,000,000         | 75,000,000,000,000,000            | 2              |
| GNO      | 12,500,000            | 125,000,000,000,000               | 2              |

Same magnitudes, different scale: `assets` held display units (`Decimal128(7)`),
`balance_aggregates` holds **raw** `Int128` that the read scales by the asset's
`decimals`. So the 40 rows are not a competing source of truth that a DROP would
destroy — they are frozen pre-0293 values that no code path consults, and the
live figures for the same assets are already served from `balance_aggregates`.

## 5. Nothing downstream depends on them

- **No view/MV reads `assets`.** The only two materialized views are
  `accounts_recent_mv` (off `accounts`) and `balance_aggregates_mv`
  (`REFRESH EVERY 2 MINUTE`, `SELECT … FROM balances FINAL GROUP BY asset_id`).
  Neither touches `assets`, so the DROP cannot break a refresh.
- **The assets endpoints already read elsewhere** — `queries.rs` takes
  supply/holders from `balance_aggregates` and the icon from
  `argMax(asset_enrichment.icon_url, version)`.
- **Two liquidity-pool queries did still read `assets.icon_url`** (detail +
  list, `max(a.icon_url)` feeding `asset_a_icon_url` / `asset_b_icon_url`).
  Since the column is 0/411,654 populated, every LP leg icon in the API response
  has always been NULL. Re-pointed to `asset_enrichment` — the ADR 0050 source,
  `ReplacingMergeTree`, **11,977 of 341,611 rows carry an icon** — so the field
  starts returning real icons instead of a silent NULL.
- **The remaining hits are Postgres-era** (`audit-harness/sql/10_assets.sql`,
  `horizon-diff.rs` — `sqlx::PgPool`, `::TEXT` casts). PG was retired by 0244;
  left untouched rather than maintained.

## 6. Order of operations (why the code lands first)

The ClickHouse client writes an explicit column list, so dropping a column the
indexer still names would fail every INSERT with `No such column`. The reverse —
code that no longer names a column that still exists — is safe: the column keeps
taking its `NULL` default. Hence: **deploy the stripped code, drain the old
Lambda versions, then `ALTER`.**

Operator statements (not run here):

```sql
ALTER TABLE assets DROP COLUMN total_supply;
ALTER TABLE assets DROP COLUMN holder_count;
ALTER TABLE assets DROP COLUMN icon_url;
```

## Out of scope, still true

`account_balances_current` is empty (0 rows) against `balances` at 75,659,065.
Still gated on task 0214's acceptance criterion being rewritten first, per the
task history — not touched by this change.
