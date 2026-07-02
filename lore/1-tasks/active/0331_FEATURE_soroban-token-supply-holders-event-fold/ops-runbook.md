# 0331 OPS Runbook — deploy unified balances + seed contract-held 0/1

> Companion to task **0331**. This is the **prod execution plan** for the unified
> `balances` model (Option C) + the contract-held type-0/1 re-key (ADR 0051). It is a
> **separate, gated run** — nothing here is executed by CI or the app; an operator runs
> it against prod ClickHouse. All CH migrations are **manual `chq` SQL** (the `db-migrate`
> Lambda is Postgres-only, and PG is retired).
>
> Kept IN task 0331 (not a separate task) by decision 2026-07-02.

## Preconditions (verified 2026-07-02)

- [x] **0339 phase-2 DONE in prod** — `chq`: `assets` `asset_type=2` rows = **0**;
      `asset_sac` = **46,712 rows (3,780 deployed)**. So the SAC→classic map the re-key reads
      is populated, and the type-2 duplicate rows are already gone. (0339 Phase 1 reader is
      therefore also live.)
- [x] **Snapshot** taken (DB + host).
- [ ] Indexer binary built from `feat/0331…` (the re-key + `assets.id`-writing `AssetRow::staged` + single-write `balances`).
- [ ] `assets.id` column does NOT yet exist on prod (`chq`: column absent) — Step 1 adds it.

## Ordering is load-bearing

The indexer is **single-write**: once deployed it writes `balances` only and STOPS writing
`account_balances_current`. So deploying it is a **cutover**, not a dual-write. And the
_current_ prod indexer has NO `assets.id` code, so it would keep writing `id=0` — therefore
the `assets.id` backfill must be bracketed by **stop-old → backfill → deploy-new**, never
"backfill then restart the old binary".

---

## Step 1 — [DB] add the `assets.id` column (indexer still running)

```sql
ALTER TABLE assets ADD COLUMN IF NOT EXISTS id Int64 DEFAULT 0;
```

`CREATE TABLE IF NOT EXISTS` (init.sql) can't add a column to an existing table, hence this
manual ALTER. All existing rows are now `id = 0`.

## Step 2 — STOP the indexer

Required for the whole-table `EXCHANGE` in Step 3 (a concurrent write between staging-build
and swap would be lost), and so the old (no-`id`) binary stops re-introducing `id=0` rows.

## Step 3 — [run] backfill `assets.id` (indexer STOPPED)

```bash
# benchmark / preview first
backfill-runner --target clickhouse assets-id-backfill --dry-run
# for real
backfill-runner --target clickhouse assets-id-backfill
```

Computes `id = ids::asset_id(...)` in **Rust** (CH `cityHash64` differs → cannot be done in
SQL) into a temp map, builds a staging `assets`, and `EXCHANGE TABLES`-swaps it. Idempotent.

**Gate:** the command exits non-zero if any `id=0` remains. Confirm:

```sql
SELECT count() FROM assets FINAL WHERE id = 0;   -- MUST be 0
```

## Step 4 — [deploy] the new indexer (single-write cutover)

`init.sql` (idempotent) creates `balances`, `balance_aggregates`, and `balance_aggregates_mv`.
From here: live writes go to `balances`; `account_balances_current` is frozen (no longer
written); `assets.id` is stamped on every new/rewritten row; contract-held 0/1 balances are
re-keyed onto their classic/native id via `asset_sac`.

## Step 5 — [DB] migrate classic `account_balances_current` → `balances`

Pre-check (MUST return 0, else STOP — Step 3 didn't finish):

```sql
SELECT count() FROM assets FINAL WHERE asset_type IN (0, 1) AND id = 0;
```

Migration (`Decimal128(7)` → raw `Int128 ×10⁷`; join reads `assets.id` directly — do NOT
compute the hash in SQL; RMT-idempotent, safe to re-run):

```sql
INSERT INTO balances (holder_id, asset_id, amount, last_updated_ledger)
SELECT abc.account_id, a.id, toInt128(abc.balance * 10000000), abc.last_updated_ledger
FROM account_balances_current abc FINAL
INNER JOIN assets a FINAL
   ON a.asset_code = abc.asset_code
  AND a.issuer_id  = abc.issuer_id
  AND a.asset_type = if(abc.asset_type = 0, 0, 1);   -- Horizon native/alphanum → project native/classic-credit
```

## Step 6 — [run] catch-up to tip

Let the indexer catch up to the chain head before seeding (the seed reads current chain state
and must not be superseded by a still-lagging live writer). Confirm `max(sequence)` in
`ledgers` is at tip.

## Step 7 — [run] balance-seed (after catch-up)

```bash
# benchmark the ~4.46B-row SAC scan + read the funnel counts FIRST
backfill-runner --target clickhouse balance-seed --soroban-rpc-url <url> --dry-run
# for real
backfill-runner --target clickhouse balance-seed --soroban-rpc-url <url>
```

Seeds **type-3** (`read_seed_candidates`, G+C holders) **and contract-held 0/1**
(`read_sac_seed_candidates`, `is_sac`, C-only holders) from current chain state (lag-immune;
live ingest supersedes via RMT on catch-up). Account-held 0/1 is NOT seeded here — it came via
the Step 5 migration.

## Step 8 — [validate] against on-chain getters

Required gate — cross-check per-SAC / per-pool sums against independently readable state:

```bash
# example: the AMM pool holding XLM + EURC (~1.17M XLM, ~202k EURC)
compare-with-stellar-api …   # or invoke get_reserves() directly
```

Validate: ≥10 type-3 incl. a vault (MERU) + rebasing (EUTBL/eurSAFO); classic USDC + a few
account portfolios; ≥1 contract portfolio via `get_reserves`; `holder_count` vs independent
enumeration on ≥3 tokens; a dormant holder's `removed→0`. **Log any dropped counts — never
claim 100% enumeration.** Measure the frozen (`authorized=false`) magnitude for the record
(policy = counted normally; no action, just visibility).

## Step 9 — [deploy] API + frontend

Read-cutover — deploy ONLY after `balances` is populated (Steps 5+7). The API read path serves
classic/SAC/soroban supply + portfolios from `balances`/`balance_aggregates`; deploying it over
empty tables = wrong/empty reads.

## Step 10 — [DB] drop `account_balances_current`

Post-validation only (not written since Step 4):

```sql
DROP TABLE account_balances_current;
```

## Step 11 — feed 0199

Soroban-LP reserves now live in `balances` → unblocks Soroban-DEX TVL (cross-linked in 0199).

---

## Rollback

Single-write cutover ⇒ rolling the indexer back is **lossy** (the window's account updates
went to `balances` only). Recovery = restore the pre-window snapshot + reprocess. Do not skip
the snapshot precondition. The `assets.id` swap and the classic migration are both re-runnable
(idempotent), so a mid-run failure there is recovered by re-running, not rollback.

## Command summary

| #   | Where    | Command / SQL                                                               |
| --- | -------- | --------------------------------------------------------------------------- |
| 1   | `chq`    | `ALTER TABLE assets ADD COLUMN IF NOT EXISTS id Int64 DEFAULT 0`            |
| 2   | ops      | stop indexer                                                                |
| 3   | shell    | `backfill-runner … assets-id-backfill [--dry-run]` → verify `count(id=0)=0` |
| 4   | ops      | deploy new indexer (creates `balances` via init.sql; cutover)               |
| 5   | `chq`    | classic `account_balances_current` → `balances` INSERT…SELECT               |
| 6   | run      | catch-up to tip                                                             |
| 7   | shell    | `backfill-runner … balance-seed --soroban-rpc-url <url> [--dry-run]`        |
| 8   | validate | `get_reserves` / stellar-api cross-checks                                   |
| 9   | ops      | deploy API + frontend                                                       |
| 10  | `chq`    | `DROP TABLE account_balances_current`                                       |
| 11  | —        | feed 0199                                                                   |
