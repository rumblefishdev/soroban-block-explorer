# Runbook: 0221 — CH `nfts_pending` drain (SAC + Fungible leak)

> **RETIRED — task 0392 (2026-07-22).** The `nfts_pending` /
> `nft_ownership_pending` tables this runbook operates on no longer exist, and
> neither does `backfill-runner nft-reclassify`. NFT visibility is now a
> read-time filter on the contract's verdict, so nothing is promoted or
> drained; a contract's rows surface as soon as it is classified. See
> [ADR 0053](../../lore/2-adrs/0053_nft-visibility-as-read-time-verdict-filter.md).
> Kept as a record of the operations that were actually run on prod.

**Task:** [0221 — CH stage routes NFT candidates of pre-classified SAC contracts to nfts_pending](../../lore/1-tasks/backlog/0221_BUG_ch-sac-leaks-into-nfts-pending-no-db-lookup.md)
**Target:** ClickHouse (`nfts_pending`)
**Idempotent:** yes — `ALTER TABLE DELETE WHERE …` with `IN (…)` subquery is naturally idempotent; second run deletes 0 rows. `OPTIMIZE TABLE … FINAL` at the end collapses RMT parts.
**Frequency:** run once after each CH backfill window completes (when the hot+pending routing has settled). Operationally cheap — single SELECT + single ALTER + single OPTIMIZE.

---

## Background

The CH stage in `crates/db-clickhouse/src/persist/stage.rs` routes NFT
candidates to one of three buckets:

- **Drop** — verdict in `{Token, Fungible}` (incl. SAC, which is Token)
- **Hot `nfts`** — verdict in `{Nft}`
- **Pending `nfts_pending`** — verdict in `{Other, NULL}` (uncached)

Verdicts are sourced from `verdict_by_contract` HashMap, which is built
**only from contracts emitted into `out.contract_rows` during the
current stage call** (in-window deploys + same-ledger SAC overrides).
**CH stage has no DB lookup**, so a contract classified in an earlier
ledger is invisible to this routing pass: NFT-candidate transfers in a
later ledger fall back to `Pending` instead of `Drop`.

Empirical impact (stable across pilot sizes):

| Pilot                    | nfts_pending total | SAC/Fungible leak | Leak % |
| ------------------------ | ------------------ | ----------------- | ------ |
| 64k (62080000-62143999)  | 1,288,888          | 331,273           | 25.7%  |
| 512k (62016000-62554128) | 9,169,616          | 2,452,683         | 26.75% |

API impact: **none** — pilot endpoints read from `nfts` hot table only,
which contains 0 rows. The leak only inflates `nfts_pending` storage
and skews audit numbers.

## ContractType discriminant mapping

Source of truth: [`crates/domain/src/enums/contract_type.rs`](../../crates/domain/src/enums/contract_type.rs).

| Discriminant | Variant    |
| ------------ | ---------- |
| `0`          | `Token`    |
| `1`          | `Other`    |
| `2`          | `Nft`      |
| `3`          | `Fungible` |

The drain targets `is_sac=true` (any SAC) ∪ `contract_type IN (0, 3)`
(Token + Fungible). These are exactly the verdicts that `route_for`
maps to `NftRoute::Drop`.

---

## Drain procedure

### Step 1 — pre-drain sanity

```sql
SELECT
    count() AS pending_total,
    countIf(sc.is_sac = true OR sc.contract_type IN (0, 3)) AS leaked_drop_candidates
FROM nfts_pending np FINAL
LEFT JOIN soroban_contracts sc FINAL ON sc.id = np.contract_id;
```

### Step 2 — per-contract preview (optional, debug aid)

Top-N offenders. Useful if you need to spot-check (e.g. confirm a
specific Native-XLM SAC `CAS3J7GY…` shows up):

```sql
SELECT
    sc.contract_id,
    sc.is_sac,
    sc.contract_type,
    count() AS leaked_rows
FROM nfts_pending np FINAL
INNER JOIN soroban_contracts sc FINAL
        ON sc.id = np.contract_id
       AND (sc.is_sac = true OR sc.contract_type IN (0, 3))
GROUP BY sc.contract_id, sc.is_sac, sc.contract_type
ORDER BY leaked_rows DESC
LIMIT 20;
```

### Step 3 — drain

```sql
ALTER TABLE nfts_pending
DELETE WHERE contract_id IN (
    SELECT id
    FROM soroban_contracts FINAL
    WHERE is_sac = true
       OR contract_type IN (0, 3)
);
```

CH `ALTER … DELETE` is **async** — returns immediately, materialises in
background. Monitor via:

```sql
SELECT database, table, command, is_done, create_time, latest_fail_reason
FROM system.mutations
WHERE table = 'nfts_pending'
ORDER BY create_time DESC
LIMIT 5;
```

Wait until `is_done = 1` for the latest mutation before proceeding.

### Step 4 — `OPTIMIZE TABLE FINAL` (RMT part collapse)

```sql
OPTIMIZE TABLE nfts_pending FINAL;
```

### Step 5 — post-drain verification

```sql
SELECT
    count() AS pending_total,
    countIf(sc.is_sac = true OR sc.contract_type IN (0, 3)) AS leaked_drop_candidates
FROM nfts_pending np FINAL
LEFT JOIN soroban_contracts sc FINAL ON sc.id = np.contract_id;
```

Expect `leaked_drop_candidates = 0`.

### Step 6 — re-run guard

Re-running Steps 3-5 is safe. The DELETE matches 0 rows the second
time; `OPTIMIZE` is idempotent.

---

## Empirical execution log

Recorded on the 64k pilot (62080000-62143999) during task 0220
post-merge verification:

```
Pre-drain:    nfts_pending = 1,288,888 (leaked = 331,273)
Post-drain:   nfts_pending =   957,615 (leaked =       0)
Δ:                          -331,273 (exact, -25.7%)
```

Math checks: 1,288,888 - 331,273 = 957,615 ✓ exact.

512k pilot would similarly drain ~2.45M rows (26.75% of 9.17M).

---

## What this runbook does NOT do

- **Hot `nfts` table** — already clean post-0118 revert (0 rows in
  current pilot). Mirror SQL for that table is not included; if a
  future regression seeds the hot bucket with SACs, copy this runbook
  and swap `nfts_pending` → `nfts`.
- **`nft_ownership_pending` mirror** — the same 0220 `route_for` path emits
  ownership events, so this table carries the **same SAC/Fungible leak**.
  Confirmed un-drained and growing: 143M (2026-06-18) → **332.74M (2026-06-23)**
  while `nfts_pending` got the 0306 reclassify-delete and this mirror did not.
  Run the full procedure with the table swapped, but use the **OOM-safe verify**
  below — a `nft_ownership_pending FINAL ⋈ soroban_contracts FINAL` JOIN OOMs at
  the 6 GB `max_memory_usage` cap; an `IN`-subquery against the small contracts
  side does not:

  ```sql
  -- pre/post verify (replaces the Step 1/5 JOIN form for this big table)
  SELECT count() AS pending_total,
         countIf(contract_id IN (
             SELECT id FROM soroban_contracts FINAL
             WHERE is_sac = true OR contract_type IN (0, 3)
         )) AS leaked_drop_candidates
  FROM nft_ownership_pending;

  -- drain (async; no restart; safe during live ingestion)
  ALTER TABLE nft_ownership_pending
  DELETE WHERE contract_id IN (
      SELECT id FROM soroban_contracts FINAL
      WHERE is_sac = true OR contract_type IN (0, 3)
  );
  OPTIMIZE TABLE nft_ownership_pending FINAL;
  ```

  To drain the SAC/Fungible leak **and** the 0294 orphan mass in one pass
  (recommended — the orphan clause is the bulk of `nft_ownership_pending`), use
  the combined predicate in
  [`0294_ch_drain_orphans_from_pending.md`](0294_ch_drain_orphans_from_pending.md)
  instead of the SAC/Fungible-only predicate above.

- **Postgres side** — PG has DB-lookup via `ClassificationCache` in
  `resolve_nft_filter` (`crates/indexer/src/handler/persist/write.rs:1436-1500`)
  so this leak is structurally absent on PG. No drain needed there.

## Long-term fix (out of runbook scope)

Either:

1. **Add DB lookup pass in CH stage** — lift `stage::prepare_with_sac_overrides`
   from pure function to a `Stage` struct holding a CH client + per-worker
   cache (mirror PG `ClassificationCache`).
2. **Hook drain into per-window completion** — automate Steps 3-5 at the
   end of every `BatchWindowReport` in the CH writer pipeline.

Both are tracked under task 0221 §"Future Work".

---

## Observed top offenders (62080000-62143999)

| contract_id                                                | is_sac | type | leaked rows |
| ---------------------------------------------------------- | ------ | ---- | ----------- |
| `CBROEYKBQL6536KNWD3DWUVBQLFMWKMFWCDM5GVQHANRPL3J423FYNBJ` | true   | 0    | 44 698      |
| (top 10 collectively account for ~80 % of the leak)        |        |      |             |

The Native XLM SAC `CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA`
is the canonical example operators should expect to see — it transfers
all the time and is the largest single contributor.
