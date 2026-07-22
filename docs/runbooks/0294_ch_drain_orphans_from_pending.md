# Runbook: 0294 — CH orphan + SAC/Fungible drain from `*_pending`

> **RETIRED — task 0392 (2026-07-22).** The `nfts_pending` /
> `nft_ownership_pending` tables this runbook operates on no longer exist, and
> neither does `backfill-runner nft-reclassify`. NFT visibility is now a
> read-time filter on the contract's verdict, so nothing is promoted or
> drained; a contract's rows surface as soon as it is classified. See
> [ADR 0053](../../lore/2-adrs/0053_nft-membership-decided-at-write-time-from-wasm.md).
> Kept as a record of the operations that were actually run on prod.

**Task:** [0294 — SAC labeling + orphan composition](../../lore/1-tasks/active/0294_BUG_sac-labeling-and-orphan-composition/README.md)
**Targets:** ClickHouse `nfts_pending` **and** `nft_ownership_pending`
**Idempotent:** yes — `ALTER TABLE DELETE WHERE … IN (…)` deletes 0 rows on re-run; `OPTIMIZE … FINAL` collapses RMT parts.
**Frequency:** run after a CH backfill/reclassify window settles. Cheap (single SELECT + single ALTER + single OPTIMIZE per table).

---

## What this drains — and what it does NOT

This is **symptom relief**: it deletes the false-positive rows that the
classification leak parked in the quarantine tables. It covers two converging
leaks in one predicate:

- **0221 (SAC + Fungible leak):** `route_for` has no DB lookup, so transfers of
  contracts classified `Token`/`Fungible`/SAC in an earlier ledger fall through
  to `*_pending` instead of `Drop`.
- **0294 (orphan / un-deployed-SAC mass):** non-SAC "orphans" (no deploy, NULL
  `wasm_hash`) are mislabeled `is_sac=false`. They are un-deployed SACs surfaced
  via CAP-67; their `i128` transfer **amounts** are mis-read as `token_id` →
  bulk false-positive pending rows. The 2026-06-18 full census proved the orphan
  **pending mass is ~100% un-deployed SAC false positives** (5,503/5,503 emitters
  cryptographically derive to their own SAC address; real-NFT orphans hold ~0
  pending rows) → blanket drop has ~0 collateral.

It does **NOT**:

- **Relabel orphans** (`is_sac=true`, `contract_type=Token`) — that is 0294
  Step 2 (extend the SAC-override to classic-payment events, `state.rs`), a
  writer-coupled code change. Without it, **live ingestion refills `*_pending`**
  with the same false positives (this is why `nft_ownership_pending` grew
  143M → 332.74M between 2026-06-18 and 2026-06-23).
- **Touch `soroban_contracts`** — the SAC-skeleton placeholders there are
  load-bearing as the G9 routing-verdict cache for pre-window SACs (0294 Step 3
  HARD CONSTRAINT). This runbook only deletes from `*_pending`, so it cannot
  violate that constraint.

**Durable close = this drain (once) + 0294 Step 2 relabel + 0221 routing fix.**
The latter two are restart/writer-coupled → batch them into the 0281 window.

## The drop predicate (0221 ∪ 0294)

```sql
   is_sac = true                                       -- SAC (incl. mislabeled? no — see orphan clause)
OR contract_type IN (0, 3)                             -- 0221: Token + Fungible
OR ( is_sac = false                                    -- 0294: orphan (un-deployed SAC)
     AND (deployed_at_ledger IS NULL OR deployed_at_ledger = 0)  -- NB: BOTH NULL and =0 sentinel
     AND wasm_hash IS NULL )
```

> **Predicate correctness:** the orphan clause MUST include
> `deployed_at_ledger = 0`. The current skeleton split is ~307k across
> `deployed_at_ledger NULL` + `= 0` sentinel; a `NULL`-only predicate
> under-drains (misses the `= 0` rows). `contract_type IN (0,3)` only bites
> post-0306 rebuild, when the column is populated — pre-rebuild it was SAC-only.

## ContractType discriminant mapping

Source: [`crates/domain/src/enums/contract_type.rs`](../../crates/domain/src/enums/contract_type.rs).
`0`=Token, `1`=Other, `2`=Nft, `3`=Fungible.

---

## Drain procedure

Run per table. `nfts_pending` was already SAC/Fungible-drained by the 0306
reclassify (residual = orphans), so its big win here is the orphan clause;
`nft_ownership_pending` got neither 0306 nor 0221 yet, so expect a large drop.

### Step 1 — pre-drain census (OOM-safe: no big-table `FINAL`, no JOIN)

The 6 GB `max_memory_usage` cap makes a `*_pending FINAL ⋈ soroban_contracts FINAL`
JOIN OOM on the 332M table. Use an `IN`-subquery against the small contracts
side instead (the subquery materialises ~tens-of-thousands of `Int64` ids; the
332M scan is a cheap hash-probe):

```sql
SELECT count() AS total,
       countIf(contract_id IN (
           SELECT id FROM soroban_contracts FINAL
           WHERE is_sac = true
              OR contract_type IN (0, 3)
              OR (is_sac = false
                  AND (deployed_at_ledger IS NULL OR deployed_at_ledger = 0)
                  AND wasm_hash IS NULL)
       )) AS droppable
FROM nft_ownership_pending;        -- and again with FROM nfts_pending
```

### Step 2 — drain (async mutation)

```sql
ALTER TABLE nft_ownership_pending        -- repeat verbatim for nfts_pending
DELETE WHERE contract_id IN (
    SELECT id FROM soroban_contracts FINAL
    WHERE is_sac = true
       OR contract_type IN (0, 3)
       OR (is_sac = false
           AND (deployed_at_ledger IS NULL OR deployed_at_ledger = 0)
           AND wasm_hash IS NULL)
);
```

`ALTER … DELETE` is **async** (returns immediately, materialises in background).
It does **not** require a CH restart and is safe to run during live ingestion.

### Step 3 — monitor

```sql
SELECT table, command, is_done, parts_to_do, latest_fail_reason, create_time
FROM system.mutations
WHERE table IN ('nfts_pending', 'nft_ownership_pending') AND is_done = 0
ORDER BY create_time DESC;
```

Wait for `is_done = 1` before `OPTIMIZE`.

### Step 4 — collapse parts

```sql
OPTIMIZE TABLE nft_ownership_pending FINAL;   -- and nfts_pending
```

### Step 5 — post-drain verify

Re-run Step 1. Expect `droppable = 0` (or, if live ingestion is mid-flight, a
small and shrinking number — confirms recurrence, see "does NOT" above).

---

## Conservative alternative — match-gated drain

If you do not want to trust the blanket orphan clause, gate the orphan deletes
on a per-contract cryptographic SAC match instead:

1. Export one event per orphan (`topics_xdr` → asset `CODE:ISSUER` in topic[3]):
   see [notes/G-orphan-split-queries.md](../../lore/1-tasks/active/0294_BUG_sac-labeling-and-orphan-composition/notes/G-orphan-split-queries.md)
   **Q5** (`INTO OUTFILE`). Mind the 100 GB/h `read_bytes` quota on `topics_xdr`.
2. Derive `Asset(code,issuer).contractId(mainnet)` in Rust via
   `xdr_parser::sac::derive_sac_contract_id` and keep only ids that equal the
   orphan strkey.
3. `DELETE WHERE contract_id IN (<matched-id list>)`.

This drops ~51.6M with provably 0 collateral but costs the export + derive pass.
The blanket path is faster and, per the full census, equivalent on the pending
mass.

---

## Expected magnitude (measure with Step 1 — do not treat as a promise)

- 2026-06-18 census: `is_sac OR type IN (0,3) OR orphan` was **~99.4%** of
  `nfts_pending` (61.2M / 61.56M droppable, ~360k residual). Tables have grown
  via live ingestion since (nfts_pending ~87.4M, nft_ownership_pending ~332.7M
  on 2026-06-23) — Step 1 gives the live number.
- The orphan transfer mass dominates `nft_ownership_pending`; expect the bulk to
  drop.

## Empirical execution log

_(fill on first prod run: pre/post `droppable` per table, mutation wall-clock.)_
