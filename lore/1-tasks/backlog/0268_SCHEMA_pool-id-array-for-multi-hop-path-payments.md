---
id: '0268'
title: 'SCHEMA: operations_appearances.pool_id → pool_ids Array(FixedString(32)) for multi-hop path payments'
type: SCHEMA
status: backlog
related_adr: ['0033', '0044']
related_tasks: ['0243', '0252', '0261', '0266', '0267', '0281']
tags:
  [priority-medium, effort-medium, schema, clickhouse, multi-hop, milestone-2]
milestone: 2
links:
  - lore/1-tasks/backlog/0261_BUG_parser-missing-pool-id-on-path-payment-ops.md
  - lore/1-tasks/backlog/0266_OPS_3machine-s3-reparse-path-payment-pool-ids.md
  - lore/2-adrs/0033_soroban-events-appearances-read-time-detail.md
history:
  - date: '2026-05-25'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0261 follow-up. The current CH
      `operations_appearances.pool_id` is `Nullable(FixedString(32))`
      — a single pool per op. Path payments routing through multiple
      LPs (multi-hop) cannot be represented losslessly: only one
      pool_id per op fits, the other crossings are dropped. ~10–15 %
      of path-payment ops on mainnet are multi-hop; without this
      schema change those rows under-report pool participation.

      Option A (default plan in 0266) sidesteps the issue by storing
      only the "primary" pool of the path; this task — Option B —
      lifts that constraint with an array column. Decision blocker
      for 0266 INSERT payload shape.
  - date: '2026-06-10'
    status: backlog
    who: stkrolikiewicz
    note: >
      Settled + repurposed after the 0261 claim-atom decision and plan audit:
      Flow B is the plan of record (Array payload everywhere; scalar Option A
      dead). The code-side array shape (init.sql, fold identity, write path,
      query rewrites) lands with 0261; this task becomes the PROD schema
      migration on existing data, executed around the 0281 window. Sequence
      corrections: MATERIALIZE COLUMN (not OPTIMIZE FINAL) to materialise the
      default; ADD + MATERIALIZE pre-run ONLINE before the window; the
      oa_pool_seek projection (prod-only, via 0243 ALTER) must be
      dropped/replaced before DROP COLUMN pool_id; REMOVE DEFAULT before the
      drop; the drop itself gated on 0267 E20 green (scalar = read-path
      rollback until then). Scope +: liquidity_pool_snapshots.gross_volume_a
      ALTER (the 0266 backfill writes it; column missing from prod and
      init.sql today).
---

# SCHEMA: operations_appearances.pool_id → pool_ids Array

## Summary

Migrate `operations_appearances.pool_id` (scalar `Nullable(FixedString(32))`)
to `pool_ids Array(FixedString(32))` so a single path-payment op can
record every liquidity pool it crossed, not only one. Indexer write
path emits the full list; API queries change from `WHERE pool_id = X`
to `WHERE has(pool_ids, X)`. ReplacingMergeTree dedup logic
unchanged (sort key still per-op).

## Why

- Stellar path payments can route through multiple liquidity pools
  in one op (multi-hop). Current scalar `pool_id` forces a lossy
  choice — schema cannot store > 1 pool per op.
- Task 0261 parser fix derives the full list of crossed pools from
  op_meta. Option A (scalar storage) intentionally throws away
  every-non-first pool; ~10–15 % of path-payment ops on mainnet
  are multi-hop per quick napkin estimate.
- For E20 endpoint correctness (and the audit story in the M2 close)
  the multi-hop case must round-trip — Horizon
  `/liquidity_pools/:id/transactions` does surface the tx for
  every pool in the path.

## Scope

### Schema

```sql
-- Before
pool_id Nullable(FixedString(32))

-- After
pool_ids Array(FixedString(32))
```

`Array(FixedString(32))` cannot itself be `Nullable`, but the
empty array `[]` substitutes for `NULL` cleanly. CH normalises
`has([], x) = 0` so order-book-only ops with `pool_ids = []`
correctly miss the pool filter.

Companion column (same migration arc, consumed by the 0266
backfill — task 0247/0199 input):

```sql
ALTER TABLE liquidity_pool_snapshots
  ADD COLUMN gross_volume_a Nullable(Decimal128(7));
```

`init.sql` parity for both columns rides the 0261 code PR.

### Migration on existing prod data (phased around the 0281 window)

```sql
-- Phase 1 — ONLINE, before the 0281 window. Live ingestion keeps
-- running: old-indexer INSERTs omit the column, so the DEFAULT
-- fills it from the scalar and semantics stay consistent.
ALTER TABLE operations_appearances
  ADD COLUMN pool_ids Array(FixedString(32)) DEFAULT
    if(pool_id IS NULL, [], [assumeNotNull(pool_id)]);
ALTER TABLE operations_appearances MATERIALIZE COLUMN pool_ids;
-- (mutation; hours on 5.8B rows — monitor system.mutations)

-- Phase 2 — IN the 0281 window, paired with the indexer redeploy:
ALTER TABLE operations_appearances DROP PROJECTION oa_pool_seek;
-- + create the replacement seek chosen in 0281 C
--   (skip index / arrayJoin projection / helper table)

-- Phase 3 — only after 0267 E20 green (the scalar stays until then
-- as the read-path rollback; CH refuses to DROP a column referenced
-- by a DEFAULT expression or a projection — hence this ordering):
ALTER TABLE operations_appearances MODIFY COLUMN pool_ids REMOVE DEFAULT;
ALTER TABLE operations_appearances DROP COLUMN pool_id;
```

Phased deploy (add online → swap writer + reads + projection in the
window → drop after validation) keeps the API path readable
throughout.

### Indexer write path

The CH fold lives in `crates/db-clickhouse/src/persist/stage.rs`
(`OpTyped::from_details` + the 0163 identity fold; `OpKey.pool_ids`
must be **sorted + deduped** for deterministic fold identity):

- `OpTyped::pool_id_hex: Option<String>` → `pool_ids_hex: Vec<String>`
- `LiquidityPoolDeposit` / `LiquidityPoolWithdraw` emit a single-element
  vector (`vec![pool_id]`).
- `PathPaymentStrictReceive` / `PathPaymentStrictSend` emit the full
  list of crossed pools (per the 0261 parser fix).

Write SQL changes from one-row-per-op `pool_id` insert to one-row-per-op
`pool_ids` array insert.

### API query rewrites

Affected canonical SQL files in
`docs/architecture/database-schema/endpoint-queries-clickhouse/`:

- `10_get_assets_transactions.sql`
- `18_get_liquidity_pools_list.sql` _(if it filters by pool — most LP list does not)_
- `19_get_liquidity_pools_by_id.sql`
- `20_get_liquidity_pools_transactions.sql`
- `22_get_search.sql` _(if pools are queried by pool_id in search)_
- `23_get_liquidity_pools_participants.sql` _(via lp_positions — not operations_appearances; verify)_

Pattern: `oa.pool_id = unhex('…')` → `has(oa.pool_ids, unhex('…'))`.

### Validation

Re-run task 0267 `compare_e20.py` post-migration. Multi-hop
hash-set divergence drops to 0; coverage hits 100 %.

## Sequence (settled 2026-06-10 — Flow B, around the 0281 window)

1. 0261 parser fix merges: claim-atom extractor, Array emit,
   `init.sql` parity, fold + canonical query rewrites behind it.
2. Phase 1 ALTERs pre-run ONLINE (ADD + MATERIALIZE `pool_ids`,
   ADD `gross_volume_a`).
3. 0281 window: indexer redeploy (writer emits `pool_ids`) +
   projection swap (Phase 2) + API read path to `has(pool_ids, …)`
   (0281 C) + the other batched window items.
4. 0266 backfill (Array payload, full range) → merges settle.
5. 0267 E20 re-validate (expect 100 %).
6. Phase 3 cleanup: `REMOVE DEFAULT` + `DROP COLUMN pool_id`.

Former Flow A (scalar first) is dead — kept in git history only.

## Acceptance Criteria

- [ ] ALTER TABLE adds `pool_ids Array(FixedString(32))` with a
      lossless backfill default from the legacy `pool_id` column.
- [ ] Indexer write path updated to emit `pool_ids` (multi-element
      for multi-hop ops).
- [ ] All affected canonical SQL files updated to
      `has(pool_ids, …)`.
- [ ] Legacy `pool_id` column dropped only after API + indexer
      cutover verified.
- [ ] `compare_e20.py` post-migration `fail_total = 0` (100 %
      hash-set coverage).
- [ ] **Docs updated** — `docs/architecture/database-schema/database-schema-overview.md`
      records the array semantic.
- [ ] **API types regenerated** — required (response shape changes
      from `pool_id: string | null` to `pool_ids: string[]` in the
      LP-tx endpoints).

## Notes

- ReplacingMergeTree dedup logic unchanged — sort key
  `(ledger_sequence, transaction_id, application_order)` still
  per-op uniqueness; only the `pool_id` payload column reshapes.
- API contract change is breaking for downstream consumers of the
  affected endpoints. Coordinate with frontend (`web/`) before
  cutover.
- `Nullable(Array(...))` is awkward in CH and not idiomatic; the
  empty array `[]` is the canonical "no pool" marker.
- `oa_pool_seek` (prod-only projection from the 0243 ALTER, absent
  from `init.sql`) references `pool_id` — CH rejects `DROP COLUMN`
  while it exists. Drop it before the 0266 INSERTs regardless:
  per-part projection rebuild during a bulk backfill is pure waste.
