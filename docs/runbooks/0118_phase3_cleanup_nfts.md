# Runbook: 0118 Phase 3 — post-backfill NFT false-positive cleanup

> **RETIRED — task 0392 (2026-07-22).** The `nfts_pending` /
> `nft_ownership_pending` tables this runbook operates on no longer exist, and
> neither does `backfill-runner nft-reclassify`. NFT visibility is now a
> read-time filter on the contract's verdict, so nothing is promoted or
> drained; a contract's rows surface as soon as it is classified. See
> [ADR 0053](../../lore/2-adrs/0053_nft-membership-decided-at-write-time-from-wasm.md).
> Kept as a record of the operations that were actually run on prod.

**Task:** [0118 — NFT false positives from fungible token transfers](../../lore/1-tasks/blocked/0118_BUG_nft-false-positives-fungible-transfers.md)
**Targets:** Postgres (`nfts`, `nft_ownership`) + ClickHouse (`nfts`, `nft_ownership`)
**Idempotent:** yes, both stores
**Frequency:** one-shot per environment after the full Soroban-era backfill lands

---

## When to run

Run **after** the full Soroban-era backfill (task 0145 / `backfill-runner`)
has indexed every `wasm_upload` op on the target environment so that
`soroban_contracts.contract_type` is populated with WASM-derived verdicts.

Until then:

- Contracts deployed before the indexed window remain `Other`-classified
  (discriminant `1`) — the script intentionally **does not** delete them
  (they may still be legit NFTs whose WASM was simply never observed).
- The cleanup is a no-op against `Other`/NULL rows. It only acts on the
  definitive `Fungible`/`Token` verdicts (discriminants `3` and `0`).

## What it does

Removes the historical false-positive rows that accumulated under the
**pre-Patch-C** parser behavior (every SEP-41 `i128` fungible transfer
got emitted as an NFT candidate). Patch C (parser-side whitelist in
`crates/xdr-parser/src/nft.rs::looks_like_token_id`) prevents new ones
from accumulating — this script removes the legacy garbage.

Empirical baseline from the
[2026-05-12 CH pilot endpoint audit](../audits/2026-05-12-ch-pilot-endpoint-audit.md):
99.4% of `nfts` rows in the 15.7k-ledger sample were misclassified
fungible transfers. XLM SAC (`CAS3J7GY…`) alone contributed 421 871
rows of the 663 282 total.

## ContractType discriminant mapping

Source of truth: [`crates/domain/src/enums/contract_type.rs`](../../crates/domain/src/enums/contract_type.rs).

| Discriminant | Variant    | Cleanup action                                         |
| ------------ | ---------- | ------------------------------------------------------ |
| `0`          | `Token`    | **DELETE** (SAC — no WASM, classified at deploy time)  |
| `1`          | `Other`    | leave alone (may still be Nft; awaits reclassify)      |
| `2`          | `Nft`      | leave alone (definitive Nft — these are the real rows) |
| `3`          | `Fungible` | **DELETE** (WASM-classified fungible)                  |

## Preconditions checklist

- [ ] Full Soroban-era backfill complete on the target DB (PG and/or CH).
- [ ] DB backup taken (these are `DELETE`s on rows the API would otherwise
      return as "NFTs" — the operation is logically correct but irreversible
      without a backup).
- [ ] Run the sanity probe (step 1 below) and confirm
      `unclassified_with_nft_rows` ≈ 0. A non-trivial count means more
      contracts than expected still lack WASM classification — investigate
      before deleting.

---

## Postgres

The PG cleanup runs as a single transaction (DELETE) followed by
`VACUUM ANALYZE` outside the transaction (VACUUM cannot run inside).

### Step 1 — sanity probe

```sql
-- Should be ~0 after a full Soroban-era backfill.
SELECT COUNT(DISTINCT contract_id) AS unclassified_with_nft_rows
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type = 1  -- Other
 );
```

### Step 2 — pre-delete row count

```sql
-- Rows about to be removed (Fungible=3 / Token=0).
SELECT COUNT(*) AS rows_to_delete
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type IN (0, 3)  -- Token, Fungible
 );
```

### Step 3 — delete (transactional)

`nft_ownership` first (it references `nfts` rows; doing ownership
first is safe regardless of FK presence).

```sql
BEGIN;

DELETE FROM nft_ownership
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type IN (0, 3)  -- Token, Fungible
 );

DELETE FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type IN (0, 3)  -- Token, Fungible
 );

COMMIT;
```

### Step 4 — reclaim space + refresh planner stats

```sql
VACUUM ANALYZE nfts;
VACUUM ANALYZE nft_ownership;
```

### Step 5 — verify

```sql
-- Should return 0.
SELECT COUNT(*) AS remaining_fungible_rows
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type IN (0, 3)
 );
```

---

## ClickHouse

ClickHouse semantics differ from Postgres:

- `ALTER TABLE … DELETE` is **asynchronous** (a mutation). Track via
  `system.mutations` until `is_done = 1` before running `OPTIMIZE`.
- `OPTIMIZE TABLE … FINAL` collapses ReplacingMergeTree parts;
  without it the deleted rows linger as tombstones until normal
  background merges run.
- `FINAL` in the `soroban_contracts` subquery is **required** —
  `soroban_contracts` uses `ReplacingMergeTree(wasm_uploaded_at_ledger)`
  (see [`crates/db-clickhouse/schema/init.sql`](../../crates/db-clickhouse/schema/init.sql)),
  so non-`FINAL` reads can return stale `contract_type = 1` (Other)
  rows for contracts that were later reclassified once their WASM was
  observed.

### Step 1 — sanity probe

```sql
SELECT countDistinct(contract_id) AS unclassified_with_nft_rows
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type = 1  -- Other
 );
```

### Step 2 — pre-delete row count

```sql
SELECT count() AS rows_to_delete
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type IN (0, 3)  -- Token, Fungible
 );
```

### Step 3 — issue mutations

Order doesn't matter (no FK), but issue both before `OPTIMIZE` so the
latter compacts everything in one pass.

```sql
ALTER TABLE nfts DELETE
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type IN (0, 3)  -- Token, Fungible
 );

ALTER TABLE nft_ownership DELETE
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type IN (0, 3)  -- Token, Fungible
 );
```

### Step 4 — wait for mutations to complete

Poll `system.mutations` until both are `is_done = 1`:

```sql
SELECT table, mutation_id, command, is_done, latest_fail_reason
  FROM system.mutations
 WHERE table IN ('nfts', 'nft_ownership')
 ORDER BY create_time DESC;
```

Do **not** run `OPTIMIZE` while `is_done = 0` — the mutation may still
be writing to in-flight parts.

### Step 5 — collapse parts

```sql
OPTIMIZE TABLE nfts FINAL;
OPTIMIZE TABLE nft_ownership FINAL;
```

`FINAL` forces a full merge so the tombstones drop immediately rather
than during the next scheduled background merge.

### Step 6 — verify

```sql
-- Should return 0.
SELECT count() AS remaining_fungible_rows
  FROM nfts FINAL
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type IN (0, 3)
 );
```

---

## After both stores are clean

1. Open the 0118 task at
   [`lore/1-tasks/blocked/0118_BUG_nft-false-positives-fungible-transfers.md`](../../lore/1-tasks/blocked/0118_BUG_nft-false-positives-fungible-transfers.md),
   tick the empirical-dry-run acceptance criterion, add a `completed`
   history entry with the row counts from steps 2 and 6.
2. `git mv lore/1-tasks/blocked/0118_* lore/1-tasks/archive/`.
3. Regenerate the lore index.
