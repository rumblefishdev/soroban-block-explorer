# Phase 2 Migration Runbook — fold `asset_type=2` (SAC) into the facet model

> Companion to task **0339** and **[ADR 0051](../../2-adrs/0051_sac-as-facet-of-classic-credit.md)**.
> Phase 1 is the code + schema change (PR #298). **This runbook is the Phase 2 prod
> data-pass** that folds the existing `~31k asset_type=2` (SAC) rows into the facet model,
> then removes them. It is a **separate, gated run** — nothing here is executed by CI or the
> app; an operator runs it against prod ClickHouse under the preconditions below.
>
> **Every statement below was validated end-to-end on `clickhouse/clickhouse-server:26.3`
> (the prod version) against a synthetic pre-migration state covering: a classic-wrap SAC
> with a trustline row, an event-only SAC with NO trustline row, an un-deployed SAC, a
> native-wrap SAC, a no-SAC classic, a soroban token, a duplicate `soroban_contracts` part,
> and a re-run (idempotency).**

## What it does (and why it is needed)

`assets` splits a SAC and its classic asset into two rows: `type=1` (classic) and `type=2`
(sac, carrying the SAC surrogate in `contract_id`). ADR 0051 makes SAC a **facet** in the
`asset_sac` side table. Phase 1 stops WRITING new `type=2` and writes `asset_sac`; the
existing `type=2` rows stay untouched. Phase 2 must:

1. **seed `asset_sac`** from each `type=2` row,
2. **ensure the `type=1`/`type=0` identity row exists** (esp. SAC-event-only assets that
   never had a trustline row → no `type=1` row),
3. **DELETE the `type=2` rows** — without this, `/v1/assets` still lists two rows per SAC
   asset (the duplication 0336/0339 set out to kill). `asset_type` is part of the `assets`
   `ORDER BY`, so a `type=2` row **cannot be relabelled `→1` in place** (that is a different
   key → a new row, old one stays); it must be re-inserted as identity + deleted.

## Preconditions (writer-first — ordering is load-bearing)

1. **Phase 1 (PR #298) merged and deployed** to prod:
   - the **indexer no longer emits `type=2`** and writes `asset_sac` (so deleted rows do not
     regrow), and the **API reader tolerates `type=2`** + reads `asset_sac` (so it degrades
     gracefully, not 500s, during the window).
2. **`asset_sac` exists on prod** — `apply_init_sql` (idempotent `CREATE TABLE IF NOT
EXISTS`) via the indexer's schema-apply or a manual run.
3. **Backup `assets`** (Step 3's `DELETE` is a mutation — irreversible without a copy):
   ```sql
   CREATE TABLE assets_pre0339 AS assets;
   INSERT INTO assets_pre0339 SELECT * FROM assets FINAL;
   ```
   (or a `FREEZE` + rsync per the standard prod snapshot convention — cf. the 0266/0304 runs.)
   Note: this copy is point-in-time consistent **only for the `type=2` subset** (the writer no
   longer touches `type=2`, precondition 1) — the non-`type=2` rows may already be stale vs live
   by the time it finishes. That is fine: rollback only re-inserts `WHERE asset_type = 2`. Do
   NOT treat `assets_pre0339` as a full-table snapshot.
4. **Run off-peak** — the Step-3 mutation rewrites every `assets` part (`assets` is a
   non-partitioned state table, ~350k rows → seconds, but hold resources).

Record a baseline for the acceptance checks:

```sql
SELECT count() AS type2_rows FROM assets FINAL WHERE asset_type = 2;   -- ~31k expected
```

## Step 1 — seed `asset_sac` from the `type=2` rows

The `type=2` row's `contract_id` **is** the SAC surrogate (`cityhash64(C…)`, the same value
the reader hashes for the deep-link), so it maps 1:1 onto `asset_sac.sac_contract_id`. The
carrier `asset_type` is `0` for a native-wrap SAC (empty code + issuer 0), else `1`.

`sac_deployed = 1` iff the SAC is **actually deployed** — a `soroban_contracts` row with a
non-NULL `deployed_at_ledger`. **Do NOT use bare `id IN (SELECT id FROM soroban_contracts)`**:
that table also holds Pass-2 FK **stubs** (`wasm_uploaded_at_ledger=0`, `deployed_at_ledger
IS NULL`) written for any referenced `C…`, so a merely-referenced un-deployed SAC would be
mismarked `deployed=1`. That matters permanently: `asset_sac` is `AggregatingMergeTree(max)`,
so a wrong `1` max-wins forever over the live writer's correct `0`, and it also contradicts
the reader — which derives display deployed-ness from the same `soroban_contracts.
deployed_at_ledger` join (`queries_ch.rs`). The `IS NOT NULL` filter keeps the seed aligned
with both. Uses an `IN`-subquery (not a join) to avoid fan-out on un-merged
`soroban_contracts` parts. Idempotent (`max`-merge) and harmless alongside facets the
post-Phase-1 writer already wrote.

```sql
INSERT INTO asset_sac (asset_type, asset_code, issuer_id, contract_id, sac_contract_id, sac_deployed)
SELECT
    if(asset_code = '' AND issuer_id = 0, 0, 1) AS asset_type,
    asset_code,
    issuer_id,
    0                AS contract_id,             -- facet keyed on the carrier (contract_id 0)
    contract_id      AS sac_contract_id,          -- the type=2 row's contract_id IS the SAC surrogate
    toUInt8(contract_id IN (
        SELECT id FROM soroban_contracts WHERE deployed_at_ledger IS NOT NULL  -- real deploy, not a stub
    )) AS sac_deployed
FROM assets FINAL
WHERE asset_type = 2;
```

## Step 2 — insert the missing `type=1` classic identity rows

For classic-wrap SACs, the `(code, issuer)` `type=1` row usually already exists (from
trustlines). SAC-**event-only** assets (seen only via a CAP-67 event, no trustline) have
**no** `type=1` row — they must get one, or the asset is lost. Native-wrap SACs are
**excluded** (`asset_code != ''`): the native singleton already exists and re-inserting
`(0,'',0,0)` with `name=NULL` would clobber its `"Stellar Lumen"` name on the versionless
RMT. The `NOT IN` guard makes this idempotent.

```sql
INSERT INTO assets (asset_type, asset_code, issuer_id, contract_id, name)
SELECT DISTINCT 1, asset_code, issuer_id, 0, NULL
FROM assets FINAL
WHERE asset_type = 2
  AND asset_code != ''                                   -- classic-wrap only (native singleton stays)
  AND (asset_code, issuer_id) NOT IN (
      SELECT asset_code, issuer_id FROM assets FINAL WHERE asset_type = 1
  );
```

## GATE — hard checks before the irreversible DELETE

Step 3 is a **point of no return**: after it, a SAC is reconstructable only from an
`asset_sac` facet plus a surviving identity row, so **every** `type=2` asset MUST already
have both. Run these and **do not proceed unless all three return `0`** (a non-zero means
Step 1/2 did not fully cover some asset — investigate before deleting):

```sql
-- (g1) every classic-wrap type=2 (code,issuer) now has a type=1 identity row
SELECT count() FROM (
    SELECT DISTINCT asset_code, issuer_id FROM assets FINAL WHERE asset_type = 2 AND asset_code != ''
) t2
LEFT ANTI JOIN (SELECT asset_code, issuer_id FROM assets FINAL WHERE asset_type = 1) t1
    USING (asset_code, issuer_id);                                              -- expect 0

-- (g2) if native-wrap type=2 rows exist, the native singleton (type=0) must too
SELECT if(
    (SELECT count() FROM assets FINAL WHERE asset_type = 2 AND asset_code = '') > 0
    AND (SELECT count() FROM assets FINAL WHERE asset_type = 0) = 0,
    1, 0);                                                                      -- expect 0

-- (g3) every type=2 (code,issuer) is present as a facet in asset_sac
--      (asset_sac may have MORE — post-Phase-1 writer additions — so ANTI-join type2 → facets)
SELECT count() FROM (
    SELECT DISTINCT asset_code, issuer_id FROM assets FINAL WHERE asset_type = 2
) t2
LEFT ANTI JOIN (SELECT DISTINCT asset_code, issuer_id FROM asset_sac) s
    USING (asset_code, issuer_id);                                             -- expect 0
```

## Step 3 — DELETE the `type=2` rows

`assets` is a `MergeTree`-family table, so deletion is a mutation. The writer no longer emits
`type=2` (precondition 1), so they do not regrow; concurrent live inserts are unaffected (a
mutation applies to parts existing when it starts, and new parts carry no `type=2`). **Before
running, confirm no `type=2` was written recently** (belt-and-suspenders on the writer
rollout): `SELECT max(last_updated_ledger) …` is not on `assets`, so instead re-check the
count is stable across two reads a minute apart; if it grew, the writer is not fully rolled
out — stop. Re-running the DELETE later is safe (idempotent) if a straggler slips in.

```sql
ALTER TABLE assets DELETE WHERE asset_type = 2 SETTINGS mutations_sync = 2;
```

Confirm the mutation finished:

```sql
SELECT is_done, latest_fail_reason
FROM system.mutations
WHERE table = 'assets' AND command LIKE '%asset_type = 2%'
ORDER BY create_time DESC LIMIT 1;
```

## Step 4 — validation (acceptance criteria)

```sql
-- (a) no type=2 rows remain
SELECT count() FROM assets FINAL WHERE asset_type = 2;                       -- expect 0

-- (b) facet count is at least the pre-migration distinct-type2 baseline (writer may add more).
--     Assertion: expect >= the baseline recorded before Step 1; a shortfall means asset lost.
SELECT
    uniqExact((asset_code, issuer_id))                                             AS facets,
    (SELECT count() FROM assets_pre0339 FINAL
     WHERE asset_type = 2)                                                         AS type2_baseline_rows
FROM asset_sac;   -- facets >= distinct(type2_baseline (code,issuer)); gate g3 already proves coverage

-- (c) no orphan facets — every facet has a matching identity row
SELECT count() FROM (
    SELECT asset_type, asset_code, issuer_id, contract_id FROM asset_sac
    GROUP BY asset_type, asset_code, issuer_id, contract_id
) s
LEFT JOIN assets a FINAL USING (asset_type, asset_code, issuer_id, contract_id)
WHERE a.asset_code IS NULL;                                                  -- expect 0

-- (d) native singleton name intact
SELECT name FROM assets FINAL WHERE asset_type = 0;                          -- expect 'Stellar Lumen'
```

Then spot-check known SACs through the live API (use the `compare-with-stellar-api` skill):

- `GET /v1/assets?filter[sac]=true` returns the expected SAC cohort (deterministic, no dupes),
- `GET /v1/assets/USDC-GA5ZSEJY…` and `GET /v1/assets/{its C…}` both resolve to the one row,
  with `sac_contract_id` set, `sac_deployed=true`, and `deployed_at_ledger` populated,
- `GET /v1/assets/CAS3J7GY…` (XLM-SAC) resolves to `native`,
- a pool with a SAC-backed classic leg shows its SAC contract mirror.

## Rollback

- **Before Step 3** (only Steps 1–2 ran): drop the seeded facets and re-seed if needed —
  `TRUNCATE TABLE asset_sac` then re-run Step 1 (idempotent). Steps 1–2 only ADD rows; the
  `type=2` data is untouched, so the reader still serves the pre-migration shape.
- **After Step 3**: restore the deleted rows from the backup, then remove the seeded facets:
  ```sql
  INSERT INTO assets SELECT * FROM assets_pre0339 WHERE asset_type = 2;
  -- (optionally) TRUNCATE TABLE asset_sac;   -- if fully reverting Phase 1 too
  ```
  Rolling back the code deploy is the cleaner revert if the model itself is in question.

## Post-migration cleanup (a separate small PR, once `type=2` is gone)

- Remove the transitional `!= 2` carve-out in `list_asset_transactions`
  (`crates/api/src/assets/handlers.rs`).
- Align `backfill-enrichment-runner` (`crates/backfill-enrichment-runner/src/main.rs`)
  `asset_type IN (1, 2)` → `= 1`.
- Archive **0336** + **0337** as `superseded by: 0339`; mark **0339** complete; tick the
  ADR 0051 Delivery Checklist.
- Drop `assets_pre0339` once the migration is confirmed stable.

## Notes

- **Idempotent**: all three steps re-run safely (Step 1 via `AggregatingMergeTree(max)`,
  Step 2 via `NOT IN`, Step 3 is empty once `type=2` is gone). Validated by running Steps 1–2
  twice before Step 3 → identical end state.
- **Scale**: `asset_sac` ends at ~one row per SAC-having asset (~31k). The reader aggregates
  the whole (small) table per query — no skip-index needed at this size (see the `asset_sac`
  note in `init.sql`).
