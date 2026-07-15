---
prefix: R
title: 'G9 prefetch miss rate measured: 100% failure, Nullable(Int16)-as-i16 schema mismatch'
status: mature
spawned_from: README.md §Step 1
date: 2026-07-15
---

# R: Why the G1/G9 prefetch misses already-classified contracts

**Question (Step 1):** measure the G9 miss rate — why is a classified fungible
contract's verdict not in the prefetch window?

**Answer: there is no "window" problem. The G9 lookup has NEVER returned a
verdict on prod — it fails 100% of the time it matters, on a RowBinary schema
mismatch, and the failure is swallowed fail-open.**

## Root cause (code, confirmed at crate-source level)

`query_contract_verdicts` (`crates/db-clickhouse/src/persist.rs:447`)
deserializes into:

```rust
struct ContractVerdictRow {
    contract_id: String,
    contract_type: i16,   // <-- bare i16
}
```

but the column is `soroban_contracts.contract_type Nullable(Int16)`
(`crates/db-clickhouse/schema/init.sql:201`). clickhouse-rs `0.15.0` validates
RowBinaryWithNamesAndTypes on SELECT (`rowbinary/validation.rs`: `SerdeType::Option`
matches `Nullable`, a bare primitive against `Nullable(_)` →
`err_on_schema_mismatch`). Same failure class as the 2026-06-26 ch0.15 ×
`pool_id` INSERT outage — this is its SELECT-side sibling.

Validation is per-value, so the failure mode is perverse:

- query matches **0 rows** (no classified contract in the IN-list) → `Ok(empty)`, silent;
- query matches **≥1 row** (a verdict EXISTS and would have fixed routing) →
  error → `warn!` → fail-open → **whole batch of contracts** routes to Pending.

The `ClassificationCache` is only populated on `Ok` (`extend_definitive`),
so it has never held anything — the query re-fires and re-fails every
event-bearing ledger.

## Prod measurement (CloudWatch, `/aws/lambda/production-soroban-explorer-indexer`)

Error breakdown, last 7 days (2026-07-08 → 07-15), Logs Insights grouped by
`fields.error`:

| message                                          | error                                                                                                                                                                   | count      |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| `0283 live G9: contract verdict prefetch failed` | `schema mismatch: While processing column ContractVerdictRow.contract_type: attempting to (de)serialize ClickHouse type Nullable(Int16) as i16 which is not compatible` | **20,494** |
| `0320 live: prior contract-row prefetch failed`  | `bad response: Code: 47`                                                                                                                                                | 43         |

**100% of G9 failures are the one schema mismatch.** No timeouts, no other causes.

Daily counts (30-day window): first occurrence **2026-06-29** — the day indexer
concurrency was re-enabled after the ch0.15/pool_id outage (lore-0268). Since
then ~2,100–4,400 failures/day, every day (spike 06-29→07-01 ≈ 17k/13k/9k = DLQ
catch-up backlog). Zero G9 warns before 06-29 in the window (indexer down
06-26→06-29; G9 reached prod with that deploy train).

This fully explains 0391's measurements: ~6,575 pending rows/day at 91%
fungible-verdict (verdict exists in `soroban_contracts`, G9 dies before
delivering it → route falls to Pending) **and** the 21 stranded Nft-verdict
collections (Hot routing needs the same lookup).

## G1 is healthy

`WasmVerdictRow` reads `lower(hex(wasm_hash))` + `metadata` — both non-Nullable
`String`. Zero `0283 live G1` warns in 7 days. That's why `soroban_contracts`
verdicts ARE correct (deploy-time classification works); only the event-time
G9 read-back is dead.

## Side finding: 0320 prior-row prefetch is 100% broken too

`fetch_prior_contract_rows` (`persist.rs:517`) still SELECTs `name`, dropped
from `soroban_contracts` by task 0304 (prod `ALTER … DROP COLUMN` executed) →
server-side `Code: 47 UNKNOWN_IDENTIFIER` on every real invocation (43/7d —
rare because `executable_update` events are rare). `SorobanContractRow` already
has no `name` field; the SQL is stale. Consequence: live WASM-upgrade rows are
never written; only `wasm-upgrade-backfill` maintenance recovers them.

## Implication for Step 1 fix

- The "widen the prefetch window" option is off the table — the window logic is
  fine. The fix is `contract_type: Option<i16>` in `ContractVerdictRow` (+ skip
  `None`, though `WHERE contract_type IN (0,2,3)` already excludes NULL) — one
  line.
- Drop `name` from the 0320 SQL — one line, same PR.
- After the fix, G9 delivers `Fungible|Token → Drop` and `Nft → Hot` at write
  time; residual pending = genuinely-unclassified contracts only (Step 2's
  reconcile still needed for verdicts that arrive AFTER events).
- Regression guard: the repo needs a test that round-trips every
  `fetch_*`-side Row struct against the init.sql column types (the 0.15
  validation makes every mismatch a silent functional kill given fail-open).
