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

**Mechanism (AWS, operator):** the indexer is the `production-soroban-explorer-indexer` Lambda
(eu-central-1); its S3 trigger is live only while `reservedConcurrentExecutions > 0` (per the 0241
cutover runbook). STOP = set reserved concurrency to **0**. **Capture the current value first** so
Step 4/6 can restore it exactly. No events are lost — S3 keeps the backlog; it drains on restart.

```bash
# 1) capture current (record N for restore):
aws lambda get-function-concurrency --region eu-central-1 \
  --function-name production-soroban-explorer-indexer
# 2) stop:
aws lambda put-function-concurrency --region eu-central-1 \
  --function-name production-soroban-explorer-indexer \
  --reserved-concurrent-executions 0
```

**VALIDATE — writes have stopped:** capture `m1`, wait ≥30 s, capture `m2`; they must be
equal, and the Lambda must show no in-flight invocations (`aws logs tail` quiet). NOTE: `assets`
has NO ledger column — use `ledgers.sequence` (one row per ledger, the live-ingest heartbeat).

```sql
SELECT max(sequence) FROM ledgers;   -- run twice, ≥30s apart → m1 == m2 (drained)
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
-- Spot-check a known classic: id must be non-zero AND stable across a re-run. NOTE: `USDC` is
-- NOT unique — many issuers share the code (verified on prod); pin the issuer for a meaningful
-- check (Circle USDC = issuer strkey GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN).
SELECT id, issuer_id FROM assets FINAL WHERE asset_type = 1 AND asset_code = 'USDC' ORDER BY id LIMIT 3; -- all non-zero
```

## Step 4 — [deploy/start] the new indexer (single-write cutover)

⚠️ **CORRECTION (2026-07-02): the deploy does NOT create the balance tables.** `apply_init_sql`
is called ONLY by the `db-clickhouse-init` binary (setup) + tests — **NOT by the indexer Lambda**
(grep-verified). And there is no schema-apply hook in the CDK deploy. So a deploy/start of the new
indexer over a missing `balances` table makes its `BalanceRow` INSERT fail every ledger. **Create the
three balance objects EXPLICITLY first** (this run did it via `chw`, verbatim init.sql DDL — see the
run log Step 4a; or pipe init.sql through `clickhouse-client` on the box), THEN start the indexer.

**4a — create tables** (`chw`, verbatim from init.sql): `balance_aggregates` (MergeTree),
`balances` (RMT(last_updated_ledger)), `balance_aggregates_mv` (refreshable MV, full-replace → MT
target is correct, not RMT). **4b — start the indexer:** `indexerLambdaConcurrency` → 1 in
`infra/envs/production.json`, commit/push develop, `make -C infra deploy-production-compute`
(the concurrency-0 deploy DESTROYED the SQS event-source-mapping, so a bare `put-function-concurrency`
is NOT enough — the deploy RECREATES the ESM). From here: live writes go to `balances`;
`account_balances_current` is frozen; `assets.id` is stamped on every new/rewritten row.

**VALIDATE 4a — 3 tables exist + empty + MV scheduled:**

```sql
SELECT name, engine FROM system.tables
WHERE database = currentDatabase() AND name IN ('balances','balance_aggregates','balance_aggregates_mv'); -- 3
SELECT count() FROM balances;                                                         -- 0 before start
SELECT status, exception FROM system.view_refreshes WHERE view = 'balance_aggregates_mv'; -- Scheduled, no exception
```

**VALIDATE 4b — live writes flowing + new indexer stamps `id`:**

```sql
SELECT count() FROM system.tables
WHERE database = currentDatabase() AND name IN ('balances','balance_aggregates','balance_aggregates_mv'); -- MUST be 3
SELECT count() FROM balances;                    -- run twice, ~1 min apart → strictly INCREASING
-- assets has NO ledger column → cannot filter "new rows" by ledger. Instead assert the whole
-- table stays fully keyed: the new indexer stamps `id` on every insert, so any id=0 = a stray
-- old-binary write or a bug.
SELECT countIf(id = 0) FROM assets FINAL;        -- MUST stay 0 (was 0 after Step 3)
```

## Step 5 — [DB] migrate classic `account_balances_current` → `balances`

**Pre-check (MUST return 0, else STOP — Step 3 didn't finish):**

```sql
SELECT count() FROM assets FINAL WHERE asset_type IN (0, 1) AND id = 0;               -- MUST be 0
```

**BENCHMARK the source size (this INSERT scans `account_balances_current FINAL`):**

⚠️ **SCALE (measured 2026-07-02):** `account_balances_current` = **~59.8M raw rows** (~21M native
type-0 + ~39M classic). This INSERT (`abc FINAL` streamed against `assets FINAL` hash side, 329k rows)
is the **heaviest DB op in the runbook** — expect minutes + a large read. FINAL collapses RMT dups +
`balance != 0` prunes retained closed/zero trustlines, so the migrated count is < 59.8M; measure it
first. If it approaches the 30 s / 4 GB single-query cap via `chw`, chunk by `holder_id` range.

```sql
SELECT count() FROM account_balances_current FINAL WHERE balance != 0;                -- rows to migrate
```

**Migration** (`Decimal128(7)` → raw `Int128 ×10⁷`; join reads `assets.id` directly — do NOT
hash in SQL; RMT-idempotent, safe to re-run). **`WHERE balance != 0` is load-bearing** — without it
the migration copies ~29M retained closed/zero trustlines (measured: 59.87M raw = 30.87M nonzero +
29.0M zero) that add 0 to `sum(amount)` and are excluded from `countIf(amount>0)` anyway — pure bloat
on `balances` + the 2-min MV scan. Nonzero-at-cutover rows are the baseline; zero = absent = zero, and
catch-up overrides anything that later changes.

```sql
INSERT INTO balances (holder_id, asset_id, amount, last_updated_ledger)
SELECT abc.account_id, a.id, toInt128(abc.balance * 10000000), abc.last_updated_ledger
FROM account_balances_current abc FINAL
INNER JOIN assets a FINAL
   ON a.asset_code = abc.asset_code
  AND a.issuer_id  = abc.issuer_id
  AND a.asset_type = if(abc.asset_type = 0, 0, 1)    -- Horizon native/alphanum → project native/classic-credit
WHERE abc.balance != 0;                              -- skip ~29M retained zero/closed trustlines
```

**Transport:** ~31M-row INSERT+JOIN. Try `chw`; if it hits a timeout/mem cap, run the same SQL on the
Hetzner box via `clickhouse-client` (no HTTP timeout) or chunk by `holder_id` range. RMT-idempotent →
a partial failure is fixed by re-running, never a partial-state hazard.

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

**Cost + failure policy (measured 2026-07-02 via `chq` on prod):**

- **SAC candidate scan ≈ ~2.64B rows → ~2 min wall-clock, ~600 GiB read** (throughput
  ~22.5M rows/s; benchmarked on one 500k-ledger partition: 451M rows / ~20 s / ~104 GiB,
  extrapolated). NOT the dominant cost — the earlier "~4.46B / 15–45 min" note was wrong.
- **RPC phase ≈ ~3–8 min** — ~100–200k `(contract, holder)` keys, fetched SEQUENTIALLY in
  200-key batches (no concurrency, `DEFAULT_CONCURRENCY` is unused). This is the biggest slice.
- **Total ≈ ~5–12 min.**
- **⚠️ QUOTA:** the ~600 GiB scan exceeds the 100 GB/h read quota (~6×). It's a one-time cost —
  run with quota headroom, or chunk per `intDiv(ledger_sequence, 500000)` partition (~27 chunks
  of ~22 GiB) to stay under.
- **DECISION (2026-07-02): NO retry / no incremental insert — leave all-or-nothing.** The seed
  hard-fails on any RPC/CH error and inserts only at the end (a failure persists nothing). That
  is ACCEPTED: the run is cheap (~5–12 min) and idempotent (RMT), so recovery = just re-run the
  whole command. (If the RPC phase ever grows large enough that a re-run hurts, add per-batch
  retry + streaming insert — the `sync.rs` / `upgradeable_backfill` patterns already in-repo.)

**BENCHMARK first — `--dry-run` runs the candidate scan and reports the funnel WITHOUT writing:**

```bash
time backfill-runner --target clickhouse balance-seed --soroban-rpc-url <url> --dry-run
# Records: tokens, holders_enumerated, keys_requested, entries_returned, balances_decoded.
# Read the drops between levels (keyed<enumerated = malformed; returned<keyed = no live entry;
# decoded<returned = unknown value shape). `keys_requested` confirms the ~100–200k RPC estimate.
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
over `balances FINAL` recurs forever — confirm it fits the read quota). ⚠️ **CONCERN:** after Step 5,
`balances` holds ~tens of millions of rows (~60M account-held + type-3 + contract-held); a full
`balances FINAL` scan every 2 min = ~30 scans/h. At ~60M+ rows that is ~1.8B+ rows/h from the MV
ALONE — near the 2B rows/h quota. If Step-8 query-log shows the refresh scan is heavy, **relax the
cadence to `REFRESH EVERY 5 MINUTE`** (edit init.sql `balance_aggregates_mv`; 2-min freshness is not
load-bearing for supply/holders). Measure before deciding.

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
| 2   | ops      | stop indexer (Lambda concurrency → 0)                   | `max(sequence)` (ledgers) stable 30s       |
| 3   | shell    | `assets-id-backfill [--dry-run]` (benchmark, then real) | `count(id=0)=0`; USDC id ≠ 0               |
| 4   | ops      | deploy new indexer (creates `balances`)                 | 3 tables exist; `count(balances)` rising   |
| 5   | `chq`    | classic `account_balances_current` → `balances`         | pre `id=0`→0; no orphan; USDC value parity |
| 6   | run      | catch-up to tip                                         | `max(sequence)` ≈ RPC latestLedger         |
| 7   | shell    | `balance-seed …` (dry-run benchmark, then real)         | rows added; `get_reserves` match           |
| 8   | validate | on-chain cross-checks + measure leaks + MV cost         | ≥10 tokens match; MV « quota               |
| 9   | ops      | deploy API + frontend                                   | endpoints non-empty                        |
| 10  | `chq`    | `DROP TABLE account_balances_current`                   | not written since Step 4                   |
| 11  | —        | feed 0199                                               | —                                          |
