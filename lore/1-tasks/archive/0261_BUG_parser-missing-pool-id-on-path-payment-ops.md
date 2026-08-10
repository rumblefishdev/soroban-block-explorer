---
id: '0261'
title: 'BUG: parser does not tag `operations_appearances.pool_id` for path_payment ops crossing a liquidity pool'
type: BUG
status: completed
related_adr: ['0033', '0044', '0047', '0053']
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
      per ADR 0053) to avoid re-parsing the range twice. Linked 0199/0247/0266.
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
  - date: '2026-06-11'
    status: completed
    who: stkrolikiewicz
    note: >
      Merged to develop (PR #254; 3 commits 44ff886a/5c3d38ef/9aebddaf).
      Claim-atom extractor (path payments + 3 offer ops, success-only
      tx gating), pool_id→pool_ids Array in CH + init.sql,
      gross_volume_a column, API DTO pool_ids[], has() queries, bloom
      skip index, docs. ~10 parser tests + 2 CH fold tests + integration
      assertion. Max-effort code review (15 findings); all 6 actionable +
      cleanup addressed in-PR. has() coercion + bloom pruning verified on
      a throwaway CH 26.3 (prod-snapshot restore judged not worth it —
      snapshot predates the fix). Forward-ingest correct from the 0281
      indexer redeploy; historical backfill = 0266, E20 = 0267, prod
      schema ALTER = 0268/0281. ADR 0047 linked (PG retirement context).
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
until the Prices API is live (ADR 0053 read-time join) — avoids re-parsing the
historical range twice. **Do not drop `gross_volume_a` from the re-parse scope.**

Net: 0261 + 0247 + 0266 + 0268 + the on-chain input of 0199 collapse into one
extractor + one backfill. USD display of volume/fee remains gated on prices (same
blocker as TVL), independent of this capture. See ADR 0053 and
[`0199 notes/S-ch-tvl-enrichment-and-decision.md`](../backlog/0199_FEATURE_lp-analytics/notes/S-ch-tvl-enrichment-and-decision.md).

## Repro

```bash
# tx that crosses an LP via path_payment_strict_send
HASH=43fa84e750b42883118d9567b6e5fca65c30440d6aa7981a433efa5c33cd24ef

curl -s "https://horizon.stellar.org/transactions/$HASH/operations?limit=200" \
  | jq '.["_embedded"].records[] | {type, source_account, send_asset, dest_asset, path}'
# → type=path_payment_strict_send, path includes a pool-share asset

# CH side (post-fix schema, task 0268): pool_ids is the Array column.
# Pre-backfill (0266) this is still [] for historical path payments;
# post-backfill / forward-ingest it carries the crossed pool(s).
docker exec -i app-clickhouse-1 clickhouse-client --query="
SELECT type, empty(pool_ids) AS pool_empty
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
   - emit the full crossed-pool list as `pool_ids` on the op's fold
     row (Array shape — one row per op identity, NOT one row per
     pool: the RMT sort key `(ledger, tx, app_order)` + the 0163
     fold collapse multi-row-per-op; see the 2026-06-10 audit).
     Schema side = 0268, executed as the prod migration around the
     0281 window; `init.sql` parity lands with this task;
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

- [x] Parser fix lands on develop with tests covering:
      single-pool path payment, multi-hop path payment (full
      `pool_id` list from claim atoms), failed path payment
      (no claim atoms → no pool rows), plain payment (negative —
      no pool_id). **Plus** (emerged): failed-**tx**-with-op-success,
      fee-bump unwrap, offer-op crossing a pool (PR #254).
- [x] Extractor exposes per-atom `amount_sold` / `amount_bought`
      (`claimedAtoms` in op details) so 0266 / 0247 / 0199 can
      compute `gross_volume_a` without a second parse pass.
- [x] Backfill handed off to 0266 (shared claim-atom re-parse;
      `gross_volume_a` kept in its scope); 0252 E20 re-run tracked
      as 0267. _(Backfill execution + E20 are 0266/0267, post the
      0281 window — not this task.)_
- [x] **Docs updated** —
      `docs/architecture/xdr-parsing/xdr-parsing-overview.md`
      records that path-payment **and offer** ops contribute pool
      ids to `operations_appearances` when they cross an LP, plus
      the success-only rule.

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

## Implementation Notes

Shipped in PR #254 (merged to develop, commits `44ff886a`,
`5c3d38ef`, `9aebddaf`).

- **Parser** (`crates/xdr-parser/src/operation.rs`): `tx_op_results`
  (success-only, unwraps fee-bump) + `claim_lp_atoms` (path payments +
  3 offer ops) + `append_pool_claims` writes `poolIds` + `claimedAtoms`
  to op details, called once after the body match. New public exports:
  `tx_op_results`, `collect_tx_results` (`transaction.rs`).
- **CH writer** (`db-clickhouse`): `operations_appearances.pool_id` →
  `pool_ids Array(FixedString(32))`; 0163 fold key carries sorted+
  deduped `pool_ids`; `liquidity_pool_snapshots.gross_volume_a`
  Nullable column added (writer emits NULL; 0266 populates); bloom
  skip index `idx_oa_pool_ids` on `pool_ids` in `init.sql`.
- **API**: `OperationItem.pool_id` (nullable scalar) → `pool_ids:
Vec<String>`; LP-tx driver + canonical SQL 03/20 →
  `has(pool_ids, toFixedString(unhex(?),32))`; OpenAPI + TS regen.
- **3 call sites** rewired with op results (indexer, API enrichment);
  audit-harness passes `None` (audits PG order, pool claims irrelevant).
- **Docs**: `xdr-parsing-overview`, `database-schema-overview`,
  `clickhouse-pilot`, canonical SQL headers.
- Empirically verified on CH 26.3 (throwaway container, not the prod
  snapshot — see Issues): `has()` coerces `unhex` String→FixedString(32)
  correctly, and the bloom index prunes granules (25→1) for `has()`.

**Tests:** xdr-parser +8 (single/multi-hop/failed-op/failed-tx/
fee-bump/offer/order-book/plain); db-clickhouse fold +2 (pool-set
split, offer-op); api integration assertion updated to `pool_ids`
array; column-order pinning extended.

**Broken/modified tests:**

- `tx_op_results_*` test rewritten: TxFailed now → `None` (was
  asserting `Some`). Intentional — success-only gating, not a regression.
- `tests_integration.rs` op-fields loop: dropped `pool_id` scalar
  assertion, added `pool_ids`-is-array check. Intentional — DTO shape change.
- `smoke.rs`, `repair_tier1.rs`: column rename `pool_id`→`pool_ids`.

## Issues Encountered

- **`has()` needle coercion doubt (review #9).** Suspected
  `has(Array(FixedString(32)), unhex)` (String needle) might not match.
  Verified empirically on a throwaway CH 26.3 container (NOT the prod
  snapshot — restoring snapshot B was deemed not worth it: it predates
  the fix, has scalar `pool_id` NULL for path payments, so it cannot
  give a realistic cost test for the rows that matter). Result: 26.3
  coerces correctly; kept `toFixedString` as defensive-explicit. Realistic
  read-cost at 5.8B-row scale is deferred to 0281 against post-0266 data.
- **Code-review surfaced 6 broken queries/tests** against the new
  schema (smoke, integration test, `repair_tier1`) and 2 design gaps
  (failed-tx, offer ops). All fixed in the same PR — see Implementation
  Notes / Design Decisions.

## Design Decisions

### From Plan

1. **Claim-atom extractor over asset-pair lookup** (2026-06-09
   Decision): pools from `OperationResult`, accurate + amounts in one pass.
2. **`pool_ids` Array, one row per op identity** — RMT sort key + 0163
   fold forbid row-per-pool; multi-hop lossless (supersedes 0268 motivation).
3. **`init.sql` parity here; prod ALTER deferred to 0268/0281.**

### Emerged

4. **Success-only `tx_op_results`** (review #5): a TxFailed transaction
   rolls every op back, yet an op before the failing one still shows
   op-level `Success` with claim atoms. Gating on tx success keeps those
   phantom crossings out of `pool_ids`/`gross_volume_a`. Not in the
   original plan (which only considered op-level failure).
5. **Generalized to offer ops** (review #6): `ManageSellOffer` /
   `ManageBuyOffer` / `CreatePassiveSellOffer` carry the same claim atoms
   (`ManageOfferSuccessResult.offers_claimed`); offers fill against AMMs
   (CAP-38). Plan named only path payments; extending now keeps the 0266
   single-run premise intact (else a second 12M-ledger re-parse).
6. **`gross_volume_a` transported via `claimedAtoms` in details JSON,
   summed downstream** — not computed at ingest (0266/0247 own the sum).
7. **`toFixedString` cast on the `has()` needle** (review #9): defensive,
   not a fix — 26.3 coerces fine (verified), but explicit > implicit.
8. **Bloom skip index in `init.sql`** (review #11): fresh-install/E20
   floor; bounded prod seek stays 0281 C.
9. **PG path keeps the legacy scalar**, mapped to a 0/1-element array in
   Rust; DTO doc states the PG↔CH divergence (PG retirement, ADR 0047).
   No backport to a doomed store.

## Future Work

- Backfill execution + `gross_volume_a` wiring → **0266** (already scoped).
- E20 re-validation → **0267**.
- Prod schema ALTER + projection/seek redesign → **0268** / **0281**.
- Audit-harness CH-model divergence (`OpTypedAudit` mirrors PG, not the
  CH split-fold) — only matters if/when the harness audits CH; noted,
  not yet a task. Raise if CH-order auditing is needed.
