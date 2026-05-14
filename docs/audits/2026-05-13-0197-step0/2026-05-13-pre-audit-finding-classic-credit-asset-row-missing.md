# Pre-audit finding: classic-credit asset rows never written by indexer

**Date:** 2026-05-13
**Status:** open
**Source:** Step 0 (local audit environment setup) of task 0197 (DB completeness audit)
**Severity:** **critical** — blocks the entire M2 enrichment chain for classic credits.

## TL;DR

The indexer writes `account_balances_current` rows for classic credits
(USDC, AQUA, BTC, ETH, …) but **never creates the corresponding entity row
in the `assets` table**. The entity-row producer in
`crates/xdr-parser/src/state.rs::detect_assets()` covers only SAC
deployments and WASM-fungible deployments — classic credits have no
producer path. Every downstream task (0188 / 0191 / 0194 / 0195) assumes
the row exists; recompute / upsert / enrichment paths all no-op for
classic credits because there is nothing to update or enrich.

## Repro

Local backfill of pubnet ledgers `51000000..51000300` on 2026-05-13:

```text
ledgers:                                    301
operations_appearances type=6 (ChangeTrust): 5499
account_balances_current rows (non-native): 17186
account_balances_current distinct (code, issuer) pairs: 3904
                  (AQUA / VELO / USDC / XRP / NUNA / TIDE / BTC / SSLX / ETH / RIO ...)
assets rows:                                1  (native XLM only, asset_type=0)
```

Expected: at least 3904 `assets` rows with `asset_type IN (1, 2)`
matching the distinct credit pairs seen in balances. Observed: 0.

## Root cause

File: `crates/xdr-parser/src/state.rs`
Function: `pub fn detect_assets(deployments, interfaces) -> Vec<ExtractedAsset>`

Function doc-comment is explicit about coverage:

> Two paths produce an `ExtractedAsset`:
>
> 1. **SAC deployments** — `TokenAssetType::Sac` row.
> 2. **WASM-based deployments classifying as `Fungible`** —
>    `TokenAssetType::Soroban` row.

`TokenAssetType::ClassicCredit` is not produced anywhere in the codebase:

```bash
$ grep -rn "asset_type: TokenAssetType::ClassicCredit" crates/ | grep -v test
(no matches)
```

The downstream pipeline is otherwise ready for the missing row:

- **Schema:** partial unique index `uidx_assets_classic_asset` ON
  `(asset_code, issuer_id) WHERE asset_type IN (1, 2)` exists since
  migration 0002.
- **Staging:** `staging.rs::asset_rows` dedups `ClassicCredit` by
  `(code, issuer)`.
- **Write:** `write.rs::upsert_assets_classic_like` handles the
  `ClassicCredit` insert path.
- **Recompute:** `write.rs::recompute_asset_aggregates` (task 0194)
  collects affected `(code, issuer_id)` pairs from `balance_rows` /
  `trustline_removals` and runs an UPDATE — but the UPDATE matches zero
  rows because the assets entity row was never created.
- **Enrichment producer:** `enrichment_publish.rs` emits SQS messages
  reading from the parser's `ExtractedAsset` slice — empty for classic
  credits.
- **Enrichment worker:** `enrich_and_persist::sep1_assets::enrich_asset`
  (task 0191 / 0195 §2a) updates `assets.icon_url` and `assets.name`
  on `asset_type IN (1, 2)` — no-op when no row exists.

## Impact

| Task                               | Stated behaviour for classic credits      | Actual behaviour                             |
| ---------------------------------- | ----------------------------------------- | -------------------------------------------- |
| 0188 (SEP-1 detail)                | `description` + `home_page` runtime fetch | Works (runtime, no DB dependency)            |
| 0191 (icon enrichment)             | Lambda 2 writes `assets.icon_url`         | No-op (no row)                               |
| 0194 (holder_count + total_supply) | UPDATE on (code, issuer_id)               | No-op (no row)                               |
| 0195 §2a (sep1_assets name)        | Lambda 2 writes `assets.name`             | No-op (no row)                               |
| API `GET /v1/assets`               | Lists every asset type                    | Returns only native + SAC + Soroban-fungible |

## Audit context

This finding emerged at Step 0 of task 0197 (the bulk volumetric audit).
The intent of Step 0 is to populate a local DB so the coverage matrix in
Step 1 can be checked empirically. The diversity check (`SELECT COUNT(*)
FROM assets …`) returned 1, which is when the gap surfaced.

Audit 0197 itself is **paused** until this is fixed — running the
coverage matrix now would surface every classic-credit row in the matrix
as FAIL with the same root cause, producing noise rather than signal.

## Proposed follow-up

Spawn a `BUG` task on develop:

- Title: `BUG: indexer never writes classic-credit assets entity rows`
- Priority: **high** (blocks M2 enrichment chain for classic credits)
- Likely fix path: extend `detect_assets()` (or add a sibling producer
  hooked off `account_states.balances` JSON) to emit
  `ExtractedAsset { asset_type: ClassicCredit, asset_code, issuer_address, … }`
  for every distinct (code, issuer) pair observed in trustline / balance
  changes within the ledger.
- Acceptance: after re-running the 300-ledger backfill in this finding's
  Repro section, `SELECT COUNT(DISTINCT (asset_code, issuer_id)) FROM
assets WHERE asset_type IN (1, 2)` ≥ the same count from
  `account_balances_current` (both should match).

After the fix lands and 0197 Step 0 is re-run, the audit proceeds
normally.

## Related

- Task 0119 — added classic-credit trustline support to
  `account_balances_current`; bug exists because that task added the
  balance path without the entity-row path.
- Task 0194 — added recompute logic that assumes entity rows exist;
  recompute is a no-op without them.
- ADR 0043 §Decision — "List endpoint + on-chain → indexer" requires the
  rows to be there; classic-credit allocation violates the rule today.

## Raw queries (reproducible)

```sql
-- 1 row (native only) — should be ~3905 (native + 3904 distinct credits)
SELECT asset_type, COUNT(*) FROM assets GROUP BY asset_type;

-- 17186 classic credit balance rows
SELECT COUNT(*) FROM account_balances_current WHERE asset_code IS NOT NULL;

-- 3904 distinct (code, issuer) pairs visible in balances
SELECT COUNT(DISTINCT (asset_code, issuer_id))
FROM account_balances_current
WHERE asset_code IS NOT NULL;

-- 5499 ChangeTrust ops in the 301-ledger range
SELECT COUNT(*) FROM operations_appearances WHERE "type" = 6;

-- Top classic credits by holder count, none of which appear in assets:
SELECT asset_code, COUNT(*) AS holders
FROM account_balances_current
WHERE asset_code IS NOT NULL
GROUP BY asset_code
ORDER BY holders DESC
LIMIT 10;
-- AQUA / VELO / USDC / XRP / NUNA / TIDE / BTC / SSLX / ETH / RIO
```
