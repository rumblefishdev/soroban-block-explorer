# 0331 OPS Run Log — live execution record

> Chronological record of the **actual** prod execution of [`ops-runbook.md`](./ops-runbook.md).
> The runbook is the PLAN; this is the LOG (commands run, numbers observed, gate verdicts,
> anomalies, decisions). Append-only, one block per step. Filled live during the OPS window.

## Meta

- **Started:** 2026-07-02
- **Operator (all WRITE/DDL/deploy):** Karol Kowalczyk (`karolkow`) — holds the mTLS **write/admin** cert.
- **Guide + READ-only validation:** Claude (this session) — holds the mTLS **read-only** cert (`chq`, user `dev_read`).
- **Rule:** every WRITE to prod is run by the operator; Claude only issues `chq` READ queries,
  analyzes pasted output, and gates each step. A failed gate STOPS the run.

## Connection method (mTLS, per Filip's guide 2026-07-02)

- Host: `https://ch.sorobanscan.rumblefish.dev` — identity from the client cert, **no** `--user`/`--password`, **no** `--cacert` (public Let's Encrypt cert, system trust).
- **Read-only** (`chq`, this session): `curl --cert ~/.certs/<cn>.crt --key ~/.certs/<cn>.key <host> --data-binary "<SQL>"` → user `dev_read`. Writes → `Cannot execute query in readonly mode` (by design).
- **Write/admin** (operator): cert → user **`dev_shared`**, verified 2026-07-02 via `SHOW GRANTS`:
  `SELECT, INSERT, ALTER, CREATE, DROP, UNDROP, TRUNCATE, OPTIMIZE, SYSTEM, … ON *.*`. Covers every
  runbook WRITE (Step 1 ALTER, Step 4 CREATE, Step 5 INSERT, Step 10 DROP, SYSTEM REFRESH/WAIT VIEW).
  Same `chw` helper + host as read, no `--cacert`. Used for every Step marked `[DB]`/WRITE.
- Per-query limits (read cert): **30 s** wall, **4 GB** memory. Heavy analytics must be range/partition-scoped.
- `chq` read quota (org): 2B rows / 100 GB per server-hour — heavy reads flagged in the runbook.

## Preconditions — VERIFIED ✅ (2026-07-02, `chq`)

Query:

```sql
SELECT
  (SELECT count() FROM assets FINAL WHERE asset_type = 2)                       AS type2_rows,
  (SELECT countIf(sac_deployed = 1) FROM asset_sac)                            AS sac_deployed,
  (SELECT count() FROM system.columns
     WHERE database = currentDatabase() AND table = 'assets' AND name = 'id')  AS assets_id_exists;
```

| field              | observed | required               | verdict |
| ------------------ | -------- | ---------------------- | ------- |
| `type2_rows`       | **0**    | 0 (0339 folded type-2) | ✅      |
| `sac_deployed`     | **3780** | > 0 (~3780)            | ✅      |
| `assets_id_exists` | **0**    | 0 (Step 1 adds it)     | ✅      |

0339 phase-2 confirmed live in prod; `assets.id` absent (pending Step 1). Cleared to start.

### Post-merge re-check (2026-07-02) — `feat/0331` merged to `develop`

Operator merged the branch to `develop`. Re-verified prod is UNAFFECTED (no auto-deploy fired):
`assets.id` col = **0** (absent), `balances`/`balance_aggregates`/`balance_aggregates_mv` = **0**
(not created), `account_balances_current` = **1** (present). Prod still pristine pre-Step-1. Deploy
is manual (Step 4/9) → merge changes nothing for the SQL steps; it only makes `develop` the build source.

**Where to run:** `chq`/`chw` are curl-over-HTTPS to the single global prod CH — folder/branch/worktree
independent; run from `~` (env vars live there). Only `cargo build`/deploy (Steps 3/4/9) are run from a
repo checkout.

---

## Runbook pre-flight review vs real prod DB (2026-07-02)

Full pass over all 11 steps against the actual prod CH schema (`chq` reads + init.sql DDL).

**Bugs found + FIXED in runbook:**

1. **Step 2 liveness** — `max(last_updated_ledger) FROM assets FINAL` → `assets` has NO ledger column
   (`DESCRIBE`: asset_type, asset_code, issuer_id, contract_id, name, total_supply, holder_count,
   icon_url, id). Fixed → `max(sequence) FROM ledgers`.
2. **Step 4 gate** — `countIf(id=0) FROM assets FINAL WHERE last_updated_ledger > …` → same missing
   column. Fixed → `countIf(id=0) FROM assets FINAL` (must stay 0).
3. **Summary table** Step 2 row — updated to `max(sequence)`.
4. **Step 3 USDC spot-check** — `USDC` is NOT unique (many issuers on prod); improved to pin the issuer.

**Verified CORRECT against real data (core migration logic):**

- **Step 5 join is 1:1 — NO fan-out.** `SELECT code, issuer, count() … FROM assets type-1 GROUP BY
code, issuer HAVING count>1` returned **empty** → each (code, issuer) maps to exactly one type-1 row.
  The `(code, issuer, type)` join can't multiply an abc row. ✅ (`USDC` repeats only across distinct issuers.)
- **Native representation matches** — assets native = (type 0, code `''`, issuer 0, contract 0, id 0);
  abc native = type 0, code `''`, issuer 0, **21,024,448 rows**. Join `''=''  ∧ 0=0 ∧ if(0)=0` matches. ✅
- **Decimals math** — `abc.balance` is `Decimal(38,7)`; `toInt128(balance * 1e7)` → raw stroops. ✅
- **balances family DDL** (init.sql) matches Step 5 INSERT + read: `balances(holder_id, asset_id, amount
Int128, last_updated_ledger)` RMT(last_updated_ledger); `balance_aggregates(asset_id, total_supply
Nullable(Int128), holder_count Nullable(Int32))`; MV `sum(amount)` + `countIf(amount>0)` GROUP BY
  asset_id FROM balances FINAL, REFRESH EVERY 2 MINUTE, TO … atomic (reads need no FINAL). ✅
- **Step 7 sources** — `soroban_contracts.is_sac Bool` ✅; `asset_sac(asset_type, asset_code, issuer_id,
contract_id, sac_contract_id SAF(max,Int64), sac_deployed SAF(max,UInt8))` = the re-key map. ✅

**Flags added to runbook (not blocking):**

- **Step 5 SCALE** — `account_balances_current` = **~59.8M raw rows** (~21M native + ~39M classic).
  The migration INSERT is the heaviest DB op; measure the FINAL/`balance!=0` count first, chunk by
  `holder_id` if it nears the 30 s / 4 GB `chw` single-query cap.
- **Step 8 MV cadence** — `REFRESH EVERY 2 MINUTE` over ~60M `balances FINAL` ≈ ~1.8B rows/h from the MV
  alone → near the 2B rows/h quota. Measure at Step 8; relax to `EVERY 5 MINUTE` if heavy.

**Verdict:** runbook is **sound + appropriate for our DB** after the 4 fixes; core migration logic
confirmed against real data. Cleared to continue.

### Missing-objects check + pre-create decision (2026-07-02)

Compared init.sql's 27 CREATE objects vs prod `system.tables`. **Only 3 missing on prod** (all 0331):
`balances`, `balance_aggregates`, `balance_aggregates_mv`. Everything else already live (0293 engine
swaps `ledgers`/`wasm_interface_metadata` → RMT; 0297 `soroban_contract_metadata`; 0339 `asset_sac`;
accounts bloom index; `assets.id` from Step 1). Prod-only leftovers `asset_aggregates` /
`asset_aggregates_mv` (old read-path, still served by the CURRENT API → drop only AFTER Step 9) +
`assets_pre0339` (0339 backup) — NOT dropped here.

**DECISION: do NOT manually pre-create the 3 tables.** The new indexer runs `apply_init_sql` at
cold-start, so **Step 4 deploy creates them automatically** (runbook default). Manual pre-create was
only optional de-risking; skipped for simplicity.

**Safety fact (recorded to kill a recurring worry):** `apply_init_sql` / init.sql is **create-only**
— ZERO `DROP`/`TRUNCATE` (grep-verified), just `CREATE … IF NOT EXISTS` in a loop. Removing a table
from the _file_ never drops it from the _DB_; there is no reconcile/sync-to-file. `account_balances_current`
(the Step-5 copy source) is (a) still in the file [line 344, kept on purpose] and (b) unremovable by
schema apply regardless. In this project every DROP is a separate explicit manual step (Step 10).

---

## Step 1 — [DB] add `assets.id` column — ✅ DONE (2026-07-02)

**Prod-safety analysis (Claude, pre-run):** safe. `ADD COLUMN ... DEFAULT` is metadata-only in CH
(no part rewrite, near-instant, non-blocking); `IF NOT EXISTS` = idempotent. All writers to `assets`
(indexer `writer.rs:239` via the `clickhouse` crate + the `write.rs` INSERTs) use an **explicit named
column list** → the new column is transparently `DEFAULT 0`, no column-count/decode break. CH reads
(`queries_ch.rs`) select explicit columns. Old indexer may keep running (writes `id=0`, fixed by Step 3).

**WRITE command (operator):**

```sql
ALTER TABLE assets ADD COLUMN IF NOT EXISTS id Int64 DEFAULT 0;
```

**VALIDATE (gate — MUST be 1):**

```sql
SELECT count() FROM system.columns
WHERE database = currentDatabase() AND table = 'assets' AND name = 'id';
```

- **Ran:** `chw "ALTER TABLE assets ADD COLUMN IF NOT EXISTS id Int64 DEFAULT 0"` → empty output (OK).
- **Gate result (`chq`):** `system.columns` → `id | Int64 | DEFAULT | 0` → column exists = **1**. ✅
- **Baseline (`chq`, for Step 3):** `assets FINAL` = **329,277** rows, `id=0` = **329,277**, `id!=0` = **0**.
  Step 3 must drive `id=0` → **0** (all 329,277 keyed by the Rust `ids::asset_id`).
- **Gate verdict:** ✅ PASS. Proceed to Step 2.

---

## Step 2 — STOP indexer — PENDING (operator AWS action; not `chw`)

**Runbook bug found + fixed (2026-07-02):** the runbook's Step 2 & Step 4 VALIDATE queries read
`max(last_updated_ledger) FROM assets FINAL` / `WHERE last_updated_ledger > …` — but `assets` has
**NO ledger column** (`DESCRIBE assets` = asset_type, asset_code, issuer_id, contract_id, name,
total_supply, holder_count, icon_url, id). Fixed in runbook: Step 2 liveness → `max(sequence) FROM
ledgers` (per-ledger heartbeat); Step 4 → `countIf(id=0) FROM assets FINAL` must stay 0 (no ledger
filter possible). `account_balances_current` + `balances` DO have `last_updated_ledger` → Steps 5/10
unaffected (verified via `system.columns`).

**Liveness baseline (`chq`, 2026-07-02 10:20 UTC):** `ledgers` max sequence = **63,293,670**;
`soroban_events` max = **63,293,670** (agree). Indexer live, near tip.

**Mechanism (IaC, chosen 2026-07-02):** stop = set `indexerLambdaConcurrency` in
`infra/envs/production.json` → 0, deploy the compute stack. CDK applies concurrency 0 → indexer
Lambda throttled to 0 = stopped (S3 trigger wired only when > 0). Cleaner than a manual
`aws lambda put-function-concurrency` (which the next CDK deploy would reset to the config value).
Reversible, no data loss (S3 backlog drains on restart).

- **Committed value on develop was `1`** (real steady-state; indexer requires exactly 1 for gapless
  ordering) → **RESTORE value for Step 6 = `1`.**
- Operator's local edit `1→0` committed + pushed direct to develop: **`cbe92d57`**
  (`chore(lore-0331): pause indexer …`), husky lint/typecheck green.
- ⚠️ **Push ≠ stop.** The prod Lambda still has deployed concurrency 1 until a deploy runs.
  **Stop happens on `make -C infra deploy-production-compute`** — which ALSO ships the new indexer
  code (develop has feat/0331 merged), i.e. it does Step 2 (stop) + Step 4 (new indexer code) in one,
  scoped to the compute stack only (API/FE untouched → deployed in Step 9). `deploy-production` (--all)
  also works (operator accepts API/FE breaking meanwhile) but is unnecessary now.
- Restart later = `indexerLambdaConcurrency` back to 1 + `deploy-production-compute` (or manual set).

**Pre-Step-2 readiness gate (minimize downtime — do NOT stop until both ready):**

- Step 3 artifact: `backfill-runner` built + dry-run CLEAN against prod (see Step 3 below). ✅
- Step 4 artifact: new indexer deployable — CI green on `develop`, `make deploy-production` ready. ⬜ (last gate before stop)

- **Status:** Step 3 dry-run ✅. Awaiting Step 4 deploy-readiness confirm → then stop → freeze-validate.

## Step 3 — [run] backfill `assets.id` — DRY-RUN ✅ / REAL pending (2026-07-02)

**Invocation** (from `feat-0331` worktree; mTLS via operator `WRITE_*` = admin cert → `dev_shared`):

```bash
cargo run --release -p backfill-runner --bin backfill-runner -- \
  --target clickhouse --clickhouse-url https://ch.sorobanscan.rumblefish.dev \
  --ch-cert "$WRITE_CERT" --ch-key "$WRITE_KEY" --ch-ca "$WRITE_CA" \
  assets-id-backfill --dry-run
```

**DRY-RUN:** `total_rows=329278  id_zero_before=329278  id_zero_after=0` ✅

- `id_zero_after=0` → Rust `ids::asset_id` keys ALL rows; none escape the map (core check).
- +1 vs Step-1 baseline (329277→329278) = live indexer added 1 asset — expected.
- Builds staging + reports + drops; live `assets` untouched (dry-run). **Step 3 de-risked.**
- REAL run (drop `--dry-run`; does `EXCHANGE`) pending — requires indexer STOPPED (Step 2 first).

## Steps 2, 4–11 — remaining

Per [`ops-runbook.md`](./ops-runbook.md). Each filled here as executed:

| #   | Step                                                              | Status                      |
| --- | ----------------------------------------------------------------- | --------------------------- |
| 2   | STOP indexer (Lambda concurrency→0; `max(sequence)` stable ≥30 s) | ⬜ next                     |
| 3   | `assets-id-backfill` REAL (`count(id=0)=0`)                       | ◧ dry-run ✅ / real pending |
| 4   | deploy new indexer (creates `balances*`; writes rising)           | ⬜                          |
| 5   | migrate `account_balances_current` → `balances` (value parity)    | ⬜                          |
| 6   | catch-up to tip (`max(sequence)` ≈ RPC latestLedger)              | ⬜                          |
| 7   | `balance-seed` (dry-run benchmark → real)                         | ⬜                          |
| 8   | validate vs on-chain getters + measure leaks + MV cost            | ⬜                          |
| 9   | deploy API + FE (endpoints non-empty)                             | ⬜                          |
| 10  | drop `account_balances_current` (not written since Step 4)        | ⬜                          |
| 11  | feed 0199                                                         | ⬜                          |
