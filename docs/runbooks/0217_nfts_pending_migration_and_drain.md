# Runbook: 0217 — `nfts_pending` initial migration + post-backfill drain

> **RETIRED — task 0392 (2026-07-22).** The `nfts_pending` /
> `nft_ownership_pending` tables this runbook operates on no longer exist, and
> neither does `backfill-runner nft-reclassify`. NFT visibility is now a
> read-time filter on the contract's verdict, so nothing is promoted or
> drained; a contract's rows surface as soon as it is classified. See
> [ADR 0053](../../lore/2-adrs/0053_nft-membership-decided-at-write-time-from-wasm.md).
> Kept as a record of the operations that were actually run on prod.

**Task:** [0217 — PG+CH nfts_pending quarantine](../../lore/1-tasks/active/0217_FEATURE_nfts-quarantine-table.md)
**Targets:** Postgres (`nfts`, `nft_ownership`, `nfts_pending`, `nft_ownership_pending`) + ClickHouse (same set)
**Idempotent:**

- **Postgres** — yes for both flows (Initial migration runs in one
  transaction; Drain uses `ON CONFLICT DO NOTHING` + `TRUNCATE`).
- **ClickHouse** — yes only **after the trailing `OPTIMIZE TABLE …
FINAL`** in each flow collapses duplicate parts via the
  `ReplacingMergeTree` semantics. A partial rerun after Step 2
  (copy) but before Step 3 (delete) / Step 5 (OPTIMIZE) leaves
  in-flight duplicate parts. Read paths that issue `… FINAL` (or run
  after the next scheduled background merge) still observe the
  correct row set, but storage stays inflated until the final
  `OPTIMIZE`. Operators who abort mid-flow should rerun from the
  current step rather than from Step 1.

**Frequency:** initial migration runs once per environment on the deploy that ships task 0217; drain runs once per environment after the full Soroban-era backfill completes

---

## Background

Task 0217 introduces `nfts_pending` / `nft_ownership_pending` quarantine
tables so that contracts whose classifier verdict is `Other`/NULL
(i.e. no usable WASM observed yet) no longer pollute the API-facing
`nfts` / `nft_ownership` tables.

Two operational pieces ship together with the schema + persist
routing change:

1. **Initial migration** — moves existing `Other`-classified rows that
   live in the hot tables (legacy of pre-0217 persist behaviour) into
   the quarantine. Run once per environment immediately after the
   0217 deploy.
2. **Post-backfill drain** — once the full Soroban-era backfill has
   observed every `wasm_upload` op, drains the remainder of
   `nfts_pending` / `nft_ownership_pending`: promote any
   `Nft`-classified residue, drop the rest. Run once per environment
   after the backfill completes.

The 0118 cleanup runbook (`0118_phase3_cleanup_nfts.md`) sweeps
**`Fungible`**/**`Token`**-classified rows that landed in the hot
tables under the pre-Patch-C parser. After 0118 runs, the hot tables
contain only `Other`/`Nft` rows; this migration then moves the
`Other` portion to the quarantine. Order: **0118 cleanup first, then
0217 initial migration**.

## ContractType discriminant mapping

Source of truth: [`crates/domain/src/enums/contract_type.rs`](../../crates/domain/src/enums/contract_type.rs).

| Discriminant | Variant    |
| ------------ | ---------- |
| `0`          | `Token`    |
| `1`          | `Other`    |
| `2`          | `Nft`      |
| `3`          | `Fungible` |

---

## Part 1 — Initial migration (run on the 0217 deploy)

### Postgres

Runs as a single transaction so the move is atomic; readers see either
"row in hot" or "row in pending", never both.

#### Step 1 — pre-migration sanity

```sql
-- How many Other / NULL-classified rows currently sit in the hot tables?
-- This is the volume the migration will move.
SELECT COUNT(*) AS rows_to_migrate
  FROM nfts
 WHERE contract_id IN (
     SELECT id FROM soroban_contracts
      WHERE contract_type = 1  -- Other
         OR contract_type IS NULL
 );
```

#### Step 2 — move rows hot → pending (transactional)

`nft_ownership` is moved first via a natural-key projection (the
quarantine schema carries `(contract_id, token_id, …)` directly,
no `nft_id` FK), then `nfts` (which cascades any remaining
ownership rows). Final DELETE on `nfts` drops the source rows.

```sql
BEGIN;

-- 2a. Copy ownership rows to pending (natural-key projection).
INSERT INTO nft_ownership_pending (
    contract_id, token_id, transaction_id, owner_id, event_type,
    ledger_sequence, event_order, created_at
)
SELECT n.contract_id, n.token_id, o.transaction_id, o.owner_id,
       o.event_type, o.ledger_sequence, o.event_order, o.created_at
  FROM nft_ownership o
  JOIN nfts n ON n.id = o.nft_id
 WHERE n.contract_id IN (
     SELECT id FROM soroban_contracts
      WHERE contract_type = 1  -- Other
         OR contract_type IS NULL
 )
ON CONFLICT (contract_id, token_id, created_at, ledger_sequence, event_order)
  DO NOTHING;

-- 2b. Copy nfts rows to pending.
INSERT INTO nfts_pending (
    contract_id, token_id, collection_name, name, media_url,
    minted_at_ledger, current_owner_id, current_owner_ledger
)
SELECT contract_id, token_id, collection_name, name, media_url,
       minted_at_ledger, current_owner_id, current_owner_ledger
  FROM nfts
 WHERE contract_id IN (
     SELECT id FROM soroban_contracts
      WHERE contract_type = 1  -- Other
         OR contract_type IS NULL
 )
ON CONFLICT (contract_id, token_id) DO NOTHING;

-- 2c. Drop the source rows. ON DELETE CASCADE on nft_ownership.nft_id
--     means we don't need an explicit DELETE on nft_ownership here.
DELETE FROM nfts
 WHERE contract_id IN (
     SELECT id FROM soroban_contracts
      WHERE contract_type = 1  -- Other
         OR contract_type IS NULL
 );

COMMIT;
```

#### Step 3 — reclaim space

```sql
VACUUM ANALYZE nfts;
VACUUM ANALYZE nft_ownership;
VACUUM ANALYZE nfts_pending;
VACUUM ANALYZE nft_ownership_pending;
```

#### Step 4 — verify

```sql
-- Should be 0 — Other/NULL rows were all moved out of hot.
SELECT COUNT(*) FROM nfts
 WHERE contract_id IN (
     SELECT id FROM soroban_contracts
      WHERE contract_type = 1 OR contract_type IS NULL
 );
```

### ClickHouse

CH mutations are asynchronous (track via `system.mutations`). Run in
order: copy first, then delete; OPTIMIZE last.

> **Task 0220 note — re-insert is preferred to a hand-run migration.**
> If the indexer hasn't yet re-run over the affected ledger range
> with the task-0220 stage routing in place, the cheaper path is:
>
> 1. Force a backfill replay of the affected range with the new
>    writer build (the CH writer-parity routing emits NFT-candidate
>    rows into `nfts_pending` / `nft_ownership_pending` automatically
>    for any `Other`/uncached verdict, including the pre-0220 hot-table
>    pollution).
> 2. Run only Step 3 (`ALTER TABLE … DELETE`) on the legacy pollution
>    that remains in `nfts` / `nft_ownership` from the pre-0220 writer.
>
> Use the manual copy flow below only when a backfill replay is
> impractical (e.g. very large window, no Galexie source still
> available). Both paths converge on the same end state.

#### Step 1 — pre-migration sanity

```sql
SELECT count() AS rows_to_migrate
  FROM nfts FINAL
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type = 1 OR isNull(contract_type)
 );
```

#### Step 2 — copy hot → pending

```sql
-- ownership: project (contract_id, token_id) directly; pending table
-- carries its own natural identity.
INSERT INTO nft_ownership_pending
SELECT *
  FROM nft_ownership FINAL
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type = 1 OR isNull(contract_type)
 );

INSERT INTO nfts_pending
SELECT *
  FROM nfts FINAL
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type = 1 OR isNull(contract_type)
 );
```

#### Step 3 — delete from hot

```sql
ALTER TABLE nfts DELETE
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type = 1 OR isNull(contract_type)
 );

ALTER TABLE nft_ownership DELETE
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type = 1 OR isNull(contract_type)
 );
```

#### Step 4 — wait for mutations

```sql
SELECT table, mutation_id, is_done, latest_fail_reason
  FROM system.mutations
 WHERE table IN ('nfts', 'nft_ownership')
   AND is_done = 0;
-- Repeat until empty.
```

#### Step 5 — collapse parts

```sql
OPTIMIZE TABLE nfts FINAL;
OPTIMIZE TABLE nft_ownership FINAL;
OPTIMIZE TABLE nfts_pending FINAL;
OPTIMIZE TABLE nft_ownership_pending FINAL;
```

---

## Part 2 — Post-backfill drain (run after task 0145 completes)

After the full Soroban-era backfill has indexed every `wasm_upload`
op, the persist-time promotion hook (`reclassify_contracts_from_wasm`
in `crates/indexer/src/handler/persist/write.rs`) has already
processed every WASM observation: pending rows for newly-`Nft`
contracts have been promoted to hot during ingest, pending rows for
newly-`Fungible`/`Token` contracts have been dropped.

What remains in `_pending` at this point is:

- Rows for contracts that **truly are `Other`** (no WASM produces a
  decisive verdict) — definition-of-not-an-NFT for our purposes; drop.
- Stragglers where a WASM upload happened but was never reachable
  (rare; investigate before deleting).

### Postgres

#### Step 1 — sanity probe

```sql
-- Any pending rows still under contracts that are now Nft? They should
-- have been promoted by the persist hook; if this is > 0 something
-- went wrong — investigate before draining.
SELECT COUNT(*) AS unpromoted_nfts
  FROM nfts_pending np
 WHERE np.contract_id IN (
     SELECT id FROM soroban_contracts WHERE contract_type = 2  -- Nft
 );

-- Total drain volume.
SELECT COUNT(*) FROM nfts_pending;
SELECT COUNT(*) FROM nft_ownership_pending;
```

#### Step 2 — promote any stragglers

```sql
BEGIN;

INSERT INTO nfts (
    contract_id, token_id, collection_name, name, media_url,
    minted_at_ledger, current_owner_id, current_owner_ledger
)
SELECT np.contract_id, np.token_id, np.collection_name, np.name,
       np.media_url, np.minted_at_ledger, np.current_owner_id,
       np.current_owner_ledger
  FROM nfts_pending np
 WHERE np.contract_id IN (
     SELECT id FROM soroban_contracts WHERE contract_type = 2  -- Nft
 )
ON CONFLICT (contract_id, token_id) DO NOTHING;

INSERT INTO nft_ownership (
    nft_id, transaction_id, owner_id, event_type,
    ledger_sequence, event_order, created_at
)
SELECT n.id, op.transaction_id, op.owner_id, op.event_type,
       op.ledger_sequence, op.event_order, op.created_at
  FROM nft_ownership_pending op
  JOIN nfts n ON n.contract_id = op.contract_id AND n.token_id = op.token_id
 WHERE op.contract_id IN (
     SELECT id FROM soroban_contracts WHERE contract_type = 2
 )
ON CONFLICT (nft_id, created_at, ledger_sequence, event_order) DO NOTHING;

COMMIT;
```

#### Step 3 — truncate quarantine

```sql
TRUNCATE nft_ownership_pending;
TRUNCATE nfts_pending;
```

(Pending tables have no FK referencing them, so TRUNCATE is the
cleanest reclaim — instant + frees the storage immediately.)

#### Step 4 — verify

```sql
SELECT COUNT(*) FROM nfts_pending;          -- 0
SELECT COUNT(*) FROM nft_ownership_pending; -- 0
```

### ClickHouse

Mirror of the PG flow. Use `INSERT INTO … SELECT … FROM` for the
straggler promotion, then `TRUNCATE TABLE` on both pending tables.

> **Task 0220 note — CH drain is the only path for non-`Nft`
> stragglers.** PG has the persist-time `reclassify_contracts_from_wasm`
> promotion hook: when an `Other → Fungible` / `Other → Nft`
> transition is observed, the matching pending rows get
> promoted or dropped **inside the same persist tx**. CH has no
> per-row UPDATE, so the equivalent CH path is **re-emission on the
> next observation of the same contract** (the new stage routing
> emits a hot-bucket row when the verdict has flipped) plus
> **post-backfill drain (this Part 2)** for stragglers whose
> contracts flipped to a non-`Nft` verdict but were never re-emitted
> (no later ledger touched them).
>
> Run Part 2 once the full Soroban-era backfill has indexed every
> `wasm_upload` op AND a reasonable cooldown has elapsed so the
> re-emission path can do its work. Anything still in
> `nfts_pending` after that is either truly `Other` (drain by
> TRUNCATE below) or has a definitive non-`Nft` verdict that needs
> manual promotion via the `INSERT … SELECT … WHERE contract_type =
2` step below.

```sql
-- Straggler promotion (mirror of PG Step 2).
INSERT INTO nfts
SELECT * FROM nfts_pending FINAL
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL WHERE contract_type = 2
 );

INSERT INTO nft_ownership
SELECT contract_id, token_id, ledger_sequence, event_order,
       transaction_id, owner_id, event_type
  FROM nft_ownership_pending FINAL
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL WHERE contract_type = 2
 );

-- Drain.
TRUNCATE TABLE nfts_pending;
TRUNCATE TABLE nft_ownership_pending;

-- Compact hot tables to absorb the promoted rows + recent DELETEs.
OPTIMIZE TABLE nfts FINAL;
OPTIMIZE TABLE nft_ownership FINAL;
```

---

## After both flows have run

1. Open the 0217 task at
   [`lore/1-tasks/active/0217_FEATURE_nfts-quarantine-table.md`](../../lore/1-tasks/active/0217_FEATURE_nfts-quarantine-table.md),
   tick the operational AC entries, add a `completed` history entry
   with the row counts from Step 1 of each part.
2. `git mv lore/1-tasks/active/0217_* lore/1-tasks/archive/`.
3. Regenerate the lore index.
