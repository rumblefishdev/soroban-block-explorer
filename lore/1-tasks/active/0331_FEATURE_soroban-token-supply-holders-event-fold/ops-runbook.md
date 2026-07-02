# 0331 OPS Runbook — deploy unified balances + seed contract-held 0/1

> Companion to task **0331**. Prod execution plan for the unified `balances` model
> (Option C) + the contract-held type-0/1 re-key (ADR 0051). **Separate, gated run** —
> nothing here runs in CI or the app; an operator runs it against prod ClickHouse. All
> CH migrations are **manual `chq` SQL** (`db-migrate` Lambda is Postgres-only; PG is
> retired). Kept IN task 0331 (not a separate task) by decision 2026-07-02.

**Every step has an explicit VALIDATE gate; steps with a scan/rewrite have a BENCHMARK
first.** A step that fails its gate STOPS the run — do not proceed. All `chq` reads
count against the prod read quota (2B rows / 100 GB per server-hour); the heavy ones
are flagged.

## Preconditions (verified 2026-07-02)

- [x] **0339 phase-2 DONE in prod** — `chq`: `assets` `asset_type=2` rows = **0**;
      `asset_sac` = **46,712 rows (3,780 deployed)**. SAC→classic map is populated; type-2
      duplicate rows gone; 0339 Phase-1 reader is live.
- [x] **Snapshot** taken (DB + host).
- [ ] Indexer binary built from `feat/0331…` (re-key + `assets.id`-writing `AssetRow::staged` + single-write `balances`); CI green on PR #293.
- [ ] `assets.id` column absent on prod (Step 1 adds it).

**Verify the preconditions now:**

```sql
SELECT
  (SELECT count() FROM assets FINAL WHERE asset_type = 2)                       AS type2_rows,      -- MUST be 0
  (SELECT countIf(sac_deployed = 1) FROM asset_sac)                            AS sac_deployed,    -- MUST be > 0 (~3780)
  (SELECT count() FROM system.columns
     WHERE database = currentDatabase() AND table = 'assets' AND name = 'id')  AS assets_id_exists;-- MUST be 0 (added in Step 1)
```

## Ordering is load-bearing

The indexer is **single-write**: once deployed it writes `balances` only and STOPS writing
`account_balances_current` — deploying it is a **cutover**, not a dual-write. The _current_
prod indexer has no `assets.id` code, so it keeps writing `id=0`. Therefore the `assets.id`
backfill must be bracketed **stop-old → backfill → deploy-new**, never "backfill then
restart the old binary".

---

## Step 1 — [DB] add the `assets.id` column (indexer still running)

```sql
ALTER TABLE assets ADD COLUMN IF NOT EXISTS id Int64 DEFAULT 0;
```

`CREATE TABLE IF NOT EXISTS` (init.sql) can't add a column, hence this manual ALTER. All
existing rows are now `id = 0`.

**VALIDATE — column exists:**

```sql
SELECT count() FROM system.columns
WHERE database = currentDatabase() AND table = 'assets' AND name = 'id';  -- MUST be 1
```

## Step 2 — STOP the indexer

Required for the whole-table `EXCHANGE` in Step 3 (a concurrent write between staging-build
and swap is lost) and so the old (no-`id`) binary stops re-introducing `id=0` rows.

**VALIDATE — writes have stopped:** capture `m1`, wait ≥30 s, capture `m2`; they must be
equal, and the Lambda/worker must show no in-flight invocations.

```sql
SELECT max(last_updated_ledger) FROM assets FINAL;   -- run twice, ≥30s apart → m1 == m2
```

## Step 3 — [run] backfill `assets.id` (indexer STOPPED)

**BENCHMARK first (`--dry-run` builds staging, reports, drops it — live table untouched):**

```bash
time backfill-runner --target clickhouse assets-id-backfill --dry-run
# reads: total_rows (≈ full assets count), id_zero_before; expect id_zero_after=0.
# Record wall-clock + total_rows — the for-real run is ~the same (one extra EXCHANGE, ~0.1s).
```

**For real:**

```bash
backfill-runner --target clickhouse assets-id-backfill
```

Computes `id = ids::asset_id(...)` in **Rust** (CH `cityHash64` differs → cannot be SQL) into
a temp map, builds a staging `assets` via `a.* REPLACE (… AS id)` (no hardcoded column list),
`EXCHANGE TABLES`-swaps it. Idempotent; exits non-zero if any `id=0` remains.

**VALIDATE — every row keyed, and the ids match the Rust hash for a known asset:**

```sql
SELECT count() FROM assets FINAL WHERE id = 0;                                        -- MUST be 0
SELECT count() FROM assets FINAL WHERE asset_type = 0 AND id = 0;                     -- native MUST be 0
-- Spot-check a known classic (USDC): its id must be non-zero AND stable across a re-run.
SELECT id FROM assets FINAL WHERE asset_type = 1 AND asset_code = 'USDC' LIMIT 1;     -- non-zero
```

## Step 4 — [deploy] the new indexer (single-write cutover)

`init.sql` (idempotent) creates `balances`, `balance_aggregates`, `balance_aggregates_mv`.
From here: live writes go to `balances`; `account_balances_current` is frozen; `assets.id` is
stamped on every new/rewritten row; contract-held 0/1 balances are re-keyed via `asset_sac`.

**VALIDATE — tables exist + live writes flowing + new indexer stamps `id`:**

```sql
SELECT count() FROM system.tables
WHERE database = currentDatabase() AND name IN ('balances','balance_aggregates','balance_aggregates_mv'); -- MUST be 3
SELECT count() FROM balances;                    -- run twice, ~1 min apart → strictly INCREASING
SELECT countIf(id = 0) FROM assets FINAL WHERE last_updated_ledger > (…Step-3 max…);  -- new rows: 0
```

## Step 5 — [DB] migrate classic `account_balances_current` → `balances`

**Pre-check (MUST return 0, else STOP — Step 3 didn't finish):**

```sql
SELECT count() FROM assets FINAL WHERE asset_type IN (0, 1) AND id = 0;               -- MUST be 0
```

**BENCHMARK the source size (this INSERT scans `account_balances_current FINAL`):**

```sql
SELECT count() FROM account_balances_current FINAL WHERE balance != 0;                -- rows to migrate
```

**Migration** (`Decimal128(7)` → raw `Int128 ×10⁷`; join reads `assets.id` directly — do NOT
hash in SQL; RMT-idempotent, safe to re-run):

```sql
INSERT INTO balances (holder_id, asset_id, amount, last_updated_ledger)
SELECT abc.account_id, a.id, toInt128(abc.balance * 10000000), abc.last_updated_ledger
FROM account_balances_current abc FINAL
INNER JOIN assets a FINAL
   ON a.asset_code = abc.asset_code
  AND a.issuer_id  = abc.issuer_id
  AND a.asset_type = if(abc.asset_type = 0, 0, 1);   -- Horizon native/alphanum → project native/classic-credit
```

**VALIDATE — no orphans, no version regression, value parity on a known holder:**

```sql
-- (a) every migrated row matched an asset (no asset_id = 0 from a join miss):
SELECT countIf(asset_id = 0) FROM balances WHERE last_updated_ledger <= (…cutover ledger…); -- 0
-- (b) the live indexer (post-cutover) must WIN any (holder,asset) tie — migrated rows carry
--     the OLD frozen ledger, live rows a newer one, so RMT(max) keeps live. Spot-check a
--     holder that moved USDC after cutover: the amount must be the LIVE value, not migrated.
-- (c) value parity: pick one USDC holder; raw balance MUST equal trustline × 10^7.
SELECT b.amount FROM balances b FINAL
  JOIN assets a FINAL ON a.id = b.asset_id
 WHERE a.asset_code = 'USDC' AND b.holder_id = <known_account_id>;                    -- == abc.balance*1e7
```

## Step 6 — [run] catch-up to tip

Let the indexer reach chain head before seeding (the seed reads current state and must not be
superseded by a lagging live writer).

**VALIDATE — at tip (compare to RPC `latestLedger`):**

```sql
SELECT max(sequence) FROM ledgers;   -- MUST be within a few ledgers of Soroban RPC getLatestLedger
```

## Step 7 — [run] balance-seed (after catch-up)

**BENCHMARK first — `--dry-run` runs the heavy candidate scan (⚠️ ~4.46B-row SAC event scan)
and reports the funnel WITHOUT writing:**

```bash
time backfill-runner --target clickhouse balance-seed --soroban-rpc-url <url> --dry-run
# Records: tokens, holders_enumerated, keys_requested, entries_returned, balances_decoded.
# Read the drops between levels (keyed<enumerated = malformed; returned<keyed = no live entry;
# decoded<returned = unknown value shape). Note wall-clock + rows scanned for capacity planning.
```

**For real:**

```bash
backfill-runner --target clickhouse balance-seed --soroban-rpc-url <url>
```

Seeds **type-3** (`read_seed_candidates`, G+C holders) **and contract-held 0/1**
(`read_sac_seed_candidates`, `is_sac`, C-only) from current state (lag-immune; live ingest
supersedes via RMT). Account-held 0/1 came via Step 5, not here.

**VALIDATE — rows landed + a contract-held sum matches an independent oracle:**

```sql
SELECT count() FROM balances WHERE last_updated_ledger >= (…seed latestLedger…);      -- > 0
```

Cross-check a known Soroban-AMM pool's holdings against `get_reserves()` (see Step 8).

## Step 8 — [validate] against on-chain getters (acceptance gate)

Cross-check per-SAC / per-pool sums against independently readable state:

```bash
# the AMM pool holding XLM + EURC (~1.17M XLM, ~202k EURC):
compare-with-stellar-api …          # or invoke get_reserves() directly
```

Validate: ≥10 type-3 incl. a vault (MERU) + rebasing (EUTBL/eurSAFO); classic USDC + a few
account portfolios; ≥1 contract portfolio via `get_reserves`; `holder_count` vs independent
enumeration on ≥3 tokens; a dormant holder's `removed→0`. **Log any dropped counts — never
claim 100% enumeration.**

**MEASURE the known bounded leaks (do NOT publish supply as "correct" until you have these):**

```sql
-- Frozen (authorized=false) magnitude — policy is "count normally"; measure for disclosure:
-- (requires threading the flag; until then, estimate from SAC set_authorized events.)
-- TTL-archived positive balances that can no longer self-correct: compare a fresh seed's
-- per-asset sum vs the live table's — the delta is the archived-tail drift.
-- Classic supply completeness: our sum = trustlines + SAC contract-held; it EXCLUDES
-- claimable balances + native-protocol LP reserves (task 0210). Disclose the denominator;
-- expect a residual gap vs Horizon on heavily-pooled assets (USDC ~ −(claimable+LP)).
```

**BENCHMARK the MV cost at prod scale** (the `REFRESH EVERY 2 MINUTE` full `GROUP BY asset_id`
over `balances FINAL` recurs forever — confirm it fits the read quota):

```sql
-- After the MV runs once post-migration, read its cost from the query log:
SELECT read_rows, read_bytes, query_duration_ms
FROM system.query_log
WHERE query LIKE '%balance_aggregates%' AND type = 'QueryFinish'
ORDER BY event_time DESC LIMIT 3;   -- read_rows should be « the 2B/h quota per refresh
```

## Step 9 — [deploy] API + frontend

Read-cutover — deploy ONLY after `balances` is populated (Steps 5+7); over empty tables it
serves wrong/empty reads.

**VALIDATE — endpoints return non-empty supply/portfolio for known assets:**

```bash
curl -s /v1/assets/USDC-<issuer> | jq '.total_supply, .holder_count'   # non-null, plausible
curl -s /v1/accounts/<known_account> | jq '.balances | length'         # > 0
```

## Step 10 — [DB] drop `account_balances_current`

**VALIDATE it hasn't been written since Step 4 BEFORE dropping** (guards against a stray writer):

```sql
SELECT max(last_updated_ledger) FROM account_balances_current;   -- MUST be ≤ the cutover ledger
```

```sql
DROP TABLE account_balances_current;
```

## Step 11 — feed 0199

Soroban-LP reserves now live in `balances` → unblocks Soroban-DEX TVL (cross-linked in 0199).

---

## Rollback

Single-write cutover ⇒ rolling the indexer back is **lossy** (the window's account updates
went to `balances` only). Recovery = restore the pre-window snapshot + reprocess; do NOT skip
the snapshot precondition. The `assets.id` swap and the classic migration are both re-runnable
(idempotent), so a mid-run failure there is fixed by re-running, not rollback. **Escape hatch:**
if Step 8 fails, the old `account_balances_current` still holds the pre-cutover classic state
(not dropped until Step 10) — you can serve reads off a reverted API build while investigating.

## Command + gate summary

| #   | Where    | Command / SQL                                           | Gate (MUST hold)                           |
| --- | -------- | ------------------------------------------------------- | ------------------------------------------ |
| 1   | `chq`    | `ALTER TABLE assets ADD COLUMN IF NOT EXISTS id …`      | `system.columns` id → 1                    |
| 2   | ops      | stop indexer                                            | `max(last_updated_ledger)` stable 30s      |
| 3   | shell    | `assets-id-backfill [--dry-run]` (benchmark, then real) | `count(id=0)=0`; USDC id ≠ 0               |
| 4   | ops      | deploy new indexer (creates `balances`)                 | 3 tables exist; `count(balances)` rising   |
| 5   | `chq`    | classic `account_balances_current` → `balances`         | pre `id=0`→0; no orphan; USDC value parity |
| 6   | run      | catch-up to tip                                         | `max(sequence)` ≈ RPC latestLedger         |
| 7   | shell    | `balance-seed …` (dry-run benchmark, then real)         | rows added; `get_reserves` match           |
| 8   | validate | on-chain cross-checks + measure leaks + MV cost         | ≥10 tokens match; MV « quota               |
| 9   | ops      | deploy API + frontend                                   | endpoints non-empty                        |
| 10  | `chq`    | `DROP TABLE account_balances_current`                   | not written since Step 4                   |
| 11  | —        | feed 0199                                               | —                                          |
