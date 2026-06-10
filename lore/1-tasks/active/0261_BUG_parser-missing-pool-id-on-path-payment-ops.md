---
id: '0261'
title: 'BUG: parser does not tag `operations_appearances.pool_id` for path_payment ops crossing a liquidity pool'
type: BUG
status: active
related_adr: ['0033', '0044', '0048']
related_tasks: ['0199', '0247', '0252', '0266', '0267', '0268']
tags:
  [
    priority-medium,
    effort-small,
    layer-parser,
    layer-indexer,
    data-completeness,
  ]
milestone: 2
links:
  - crates/xdr-parser/src/operation.rs
  - docs/architecture/database-schema/endpoint-queries-clickhouse/20_get_liquidity_pools_transactions.sql
history:
  - date: '2026-05-25'
    status: backlog
    who: stkrolikiewicz
    note: >
      Surfaced by task 0252 E20 (`/liquidity-pools/:id/transactions`
      compare CH ↔ Horizon). 200-pool retention-valid sample
      reported 18 / 961 hash-set mismatches. Diagnosis split:

        - 12 cases = Horizon returns tx that CH does not. Inspecting
          one (tx `43fa84e7…`): single op = `path_payment_strict_send`
          routing through an LP (path entry references the pool
          asset). Horizon's `/liquidity_pools/:id/transactions`
          surfaces it (because the path payment crosses the pool);
          CH's `operations_appearances.pool_id` column is NULL for
          path-payment ops, so the canonical E20 SQL filter
          (`WHERE pool_id = unhex(...)`) misses them.
        - 6 cases = CH-broader (Horizon hides failed LP-touching
          tx despite `include_failed=true` — separate Horizon
          quirk; documented as tolerance in
          [[ch-horizon-semantic-diffs]]).

      Type A is a real parser gap. Fix: extend
      `crates/xdr-parser/src/operation.rs::extract_operations`
      (or the `operations_appearances` writer in
      `crates/db-clickhouse/src/persist/`) so that
      `path_payment_strict_send` / `path_payment_strict_receive`
      ops resolve each `path[]` asset whose `asset_type ==
      'pool_share'` (or whose `(send_asset, dest_asset)` pair has
      an active LP) and write the pool_id alongside.
  - date: '2026-06-09'
    status: backlog
    who: stkrolikiewicz
    note: >
      Added a Decision section: implement via a shared path-payment OperationResult
      claim-atom extractor (NOT the asset-pair lookup sketched above), so one parse
      yields pool_id + gross_volume_a (0247 / 0199) + the full pool list (multi-hop,
      subsumes 0268). The 0266 historical re-parse captures both in one run;
      gross_volume_a is captured now (USD volume/fee stay off until the Prices API
      per ADR 0048) to avoid re-parsing the range twice. Linked 0199/0247/0266.
  - date: '2026-06-10'
    status: backlog
    who: stkrolikiewicz
    note: >
      Realigned Plan + Acceptance Criteria to the claim-atom variant (Decision
      2026-06-09): extractor parses the OperationResult claim atoms and emits the
      full crossed-pool list plus per-atom amounts; the asset-pair join backfill
      migration is dropped — backfill is delegated to the 0266 shared re-parse,
      E20 verification to 0267. Scope of this task narrows to the parser fix.
  - date: '2026-06-10'
    status: active
    who: stkrolikiewicz
    note: >
      Promoted to active. Starting implementation of the claim-atom
      extractor in crates/xdr-parser.
---

# BUG: parser missing `operations_appearances.pool_id` on path_payment ops

## Summary

Path-payment ops that route through a liquidity pool do not get
their `pool_id` recorded in `operations_appearances`. CH's
`/liquidity-pools/:id/transactions` endpoint therefore misses those
tx; Horizon serves them. Surfaced by 0252 E20 — ~9 % of LP samples
(12 / 200) hit this gap.

## Decision (2026-06-09): unified claim-atom extractor — pool_id + gross_volume_a in one pass

Implement the fix via a **path-payment `OperationResult` claim-atom extractor**
(parse each `ClaimLiquidityAtom`), **not** the asset-pair lookup against
`liquidity_pools` sketched in the 2026-05-25 history note. Rationale:

- A `ClaimLiquidityAtom` carries `liquidityPoolId` **and**
  `amountSold`/`amountBought`, so one extractor yields **both**
  `operations_appearances.pool_id` (this task) **and** `gross_volume_a` per
  `(pool, ledger)` (tasks 0247 / 0199). One parse pass, two outputs.
- It is **accurate** (records the pools actually crossed, from the result) and
  yields the **full list** → multi-hop solved for free (supersedes 0268's
  scalar→Array motivation; emit `pool_ids`).
- The asset-pair lookup is approximate (cannot distinguish a pool fill from an
  order-book offer; ambiguous when several pools share an asset pair) and yields
  no amounts.

**Shared backfill.** The historical re-parse (0266, 3-machine S3) holds the same
claim atoms, so it backfills `pool_id` **and** `gross_volume_a` in one run.
Capturing `gross_volume_a` now — even though USD `volume`/`fee_revenue` stay off
until the Prices API is live (ADR 0048 read-time join) — avoids re-parsing the
historical range twice. **Do not drop `gross_volume_a` from the re-parse scope.**

Net: 0261 + 0247 + 0266 + 0268 + the on-chain input of 0199 collapse into one
extractor + one backfill. USD display of volume/fee remains gated on prices (same
blocker as TVL), independent of this capture. See ADR 0048 and
[`0199 notes/S-ch-tvl-enrichment-and-decision.md`](../blocked/0199_FEATURE_lp-analytics/notes/S-ch-tvl-enrichment-and-decision.md).

## Repro

```bash
# tx that crosses an LP via path_payment_strict_send
HASH=43fa84e750b42883118d9567b6e5fca65c30440d6aa7981a433efa5c33cd24ef

curl -s "https://horizon.stellar.org/transactions/$HASH/operations?limit=200" \
  | jq '.["_embedded"].records[] | {type, source_account, send_asset, dest_asset, path}'
# → type=path_payment_strict_send, path includes a pool-share asset

# CH side — pool_id is NULL for this op
docker exec -i app-clickhouse-1 clickhouse-client --query="
SELECT type, pool_id IS NULL AS pool_null
FROM operations_appearances
WHERE transaction_id = (SELECT id FROM transactions FINAL
                        WHERE hash = unhex('$HASH') LIMIT 1)
"
```

## Plan

1. Identify the parser path that writes `operations_appearances` (CH
   writer is `crates/db-clickhouse/src/persist/...`; the upstream
   `ExtractedOperation` field is set in `crates/xdr-parser/src/operation.rs`).
2. Claim-atom extractor (per Decision above): for
   `path_payment_strict_send` / `path_payment_strict_receive`, parse
   the op's `OperationResult` success branch (`offers: Vec<ClaimAtom>`)
   and collect every `ClaimAtom::LiquidityPool` →
   `ClaimLiquidityAtom { liquidity_pool_id, asset_sold, amount_sold,
   asset_bought, amount_bought }`:
   - emit one `operations_appearances` row per crossed pool
     (`pool_id` = atom's `liquidity_pool_id`); the result holds the
     full list, so multi-hop is covered (0268 superseded);
   - expose `amount_sold` / `amount_bought` per atom so
     `gross_volume_a` per `(pool, ledger)` can be computed downstream
     (consumed by 0247 / 0199; written by the 0266 backfill). The
     extractor lands here; the volume wiring is tracked there.
   - failed path payments carry no claim atoms → no pool rows
     (consistent with result-derived semantics).
3. Backfill: delegated to the **0266 shared re-parse** (3-machine S3),
   which runs this same extractor over the historical range and
   INSERTs `pool_id` + `gross_volume_a` in one run. The asset-pair
   join / EXCHANGE TABLES migration sketched on 2026-05-25 is
   dropped. Keep `gross_volume_a` in the 0266 scope (see Decision).
4. Verification: 0252 E20 re-run is task **0267** (post-0266) →
   expect Type A failures to drop to zero; residual Type B failures
   (Horizon-hides-failed) stay as documented tolerance.

## Acceptance Criteria

- [ ] Parser fix lands on develop with tests covering:
      single-pool path payment, multi-hop path payment (full
      `pool_id` list from claim atoms), failed path payment
      (no claim atoms → no pool rows), plain payment (negative —
      no pool_id).
- [ ] Extractor exposes per-atom `amount_sold` / `amount_bought`
      so 0266 / 0247 / 0199 can compute `gross_volume_a` without a
      second parse pass.
- [ ] Backfill handed off to 0266 (shared claim-atom re-parse;
      `gross_volume_a` kept in its scope); 0252 E20 re-run tracked
      as 0267 with `hash_set_equal` fail ≤ 1 %.
- [ ] **Docs updated** —
      `docs/architecture/xdr-parsing/xdr-parsing-overview.md`
      records that path-payment ops contribute pool_id to
      `operations_appearances` when the path crosses an LP.

## Notes

- Horizon's `include_failed=true` quirk on
  `/liquidity_pools/:id/transactions` (Type B in the
  0252 E20 diagnosis) is **not** in scope for this task. Documented
  in [[ch-horizon-semantic-diffs]] as a Horizon-side narrowing of
  the pool-tx list.
- Originally scoped 0255-shape (parser + migration in one task). Per
  the 2026-06-09 Decision the arc is split instead: parser extractor
  here, backfill in 0266 (shared with `gross_volume_a`), E20
  verification in 0267.
