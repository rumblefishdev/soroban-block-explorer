---
id: '0261'
title: 'BUG: parser does not tag `operations_appearances.pool_id` for path_payment ops crossing a liquidity pool'
type: BUG
status: backlog
related_adr: ['0033', '0044']
related_tasks: ['0252']
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
---

# BUG: parser missing `operations_appearances.pool_id` on path_payment ops

## Summary

Path-payment ops that route through a liquidity pool do not get
their `pool_id` recorded in `operations_appearances`. CH's
`/liquidity-pools/:id/transactions` endpoint therefore misses those
tx; Horizon serves them. Surfaced by 0252 E20 — ~9 % of LP samples
(12 / 200) hit this gap.

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
2. For `path_payment_strict_send` / `path_payment_strict_receive`
   ops, walk the path:
   - For each path entry whose `asset_type == 'pool_share'`, derive
     the `pool_id` from the trustline (CAP-23 pool-share asset).
   - For each consecutive (sent → received) asset pair along the
     path, resolve the matching LP via `liquidity_pools` lookup
     (constant-product or stable-pool keyed on the asset pair) and
     emit one `operations_appearances` row per crossed pool.
3. Backfill migration on Hetzner CH: re-derive `pool_id` for path
   payment ops in the retention-valid window
   (`56,657,428 ≤ ledger ≤ 62,527,999`) by joining
   `operations_appearances` (filtered to `type IN (2, 13)`) against
   `liquidity_pools` on the asset pair. EXCHANGE TABLES swap shape
   mirroring task 0255 Phase 2.
4. Re-run 0252 E20 → expect Type A failures to drop to zero;
   residual Type B failures (Horizon-hides-failed) stay as
   documented tolerance.

## Acceptance Criteria

- [ ] Parser fix lands on develop with tests covering:
      single-pool path payment, multi-pool path payment, plain
      payment (negative — no pool_id).
- [ ] Backfill migration on Hetzner CH executed; row count delta
      logged.
- [ ] 0252 E20 re-run reports `hash_set_equal` fail ≤ 1 %.
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
- Same shape as 0255 fix (parser + migration in one task). Keep the
  pattern.
