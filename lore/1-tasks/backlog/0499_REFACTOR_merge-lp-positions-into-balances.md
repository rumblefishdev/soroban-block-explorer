---
id: '0499'
title: 'REFACTOR: merge `lp_positions` into `balances` per ADR 0056'
type: REFACTOR
status: backlog
related_adr: ['0056', '0055']
related_tasks: ['0463', '0493', '0496', '0497', '0498', '0126']
tags:
  [
    backend,
    clickhouse,
    xdr-parser,
    api,
    liquidity-pools,
    assets,
    priority-low,
    effort-large,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/405'
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Filed by the lp-holdings decision session that produced ADR 0056.
      TRIGGER-GATED: do not start until task 0493 is scheduled, issue #405 is
      accepted for delivery, or Soroban-AMM feature work begins. The ADR is
      binding for new design immediately; this migration waits for a consumer.
---

# REFACTOR: merge `lp_positions` into `balances`

The full design, rationale, rejected alternatives and rules live in
[ADR 0056](../../2-adrs/0056_liquidity-position-is-a-holding.md) — implement
from there, not from memory of it. This file only carries the work list and
the gates.

## Work list

1. `TokenAssetType::PoolShare = 4`; wire label decided with 0496
   (Horizon's word is `liquidity_pool_shares`).
2. `ALTER TABLE assets ADD COLUMN pool_id FixedString(32) DEFAULT '' , MODIFY
ORDER BY (asset_type, asset_code, issuer_id, contract_id, pool_id)` —
   one statement, metadata-only; mirror in `init.sql`. Karol runs it.
3. `ids::asset_id` arm for pools; parser emits pool asset rows + position
   rows into `balances` from the same pool-share trustline path task 0463
   already stamps with `closed`.
4. Pool-side companion: refreshable MV → plain MergeTree
   `(asset_id, holder_id)`, carrying the position list and
   `first_deposit_ledger` derived from `operations_appearances` type 22.
   **Measurement gate:** the refresh cost; fallback is the sparse column per
   the ADR. The safety net replacement (test on the companion query + probe)
   is part of this item, not optional.
5. Migrate the 40,728 `lp_positions` rows (identity copy — `Decimal128(7)`
   is the same bytes as scaled `Int128`); backfill pool shares from the 0463
   checkpoint snapshot artifact, versioned on entry ledgers, provenance per 0492.
6. Re-point the pool participants read (task 0126's endpoint) at the
   companion; account page (0493) reads `balances` directly.
7. Exclusions: assets list and search `WHERE asset_type != 4` (search may
   opt in deliberately); note on `asset_sac` / `asset_enrichment`.
8. Rework `repair-tier1`: the `lp_positions` entry is deleted (0497 tracks
   the rest); `EXCHANGE`-staging machinery loses its lp target.
9. Retire `lp_positions` (stop writing, then drop after verification) and
   delete the ~30-line LP persist arm from 0463's writer.

## Acceptance criteria

- [ ] A classic pool position renders on the account page from `balances`
      alone, one seek, no glue query
- [ ] Pool participants page serves identical data to before (spot-checked),
      within the accepted refresh staleness
- [ ] `first_deposit_ledger` values match the pre-merge stored ones on a
      sampled set — or the fallback column is in use with the measured reason
      recorded in the ADR
- [ ] Assets list and search show zero pool rows; `total_supply` /
      `holder_count` unchanged for spot-checked non-pool assets
- [ ] The 0463 zero-vs-closed probe extended to pool positions returns clean
- [ ] `repair-tier1` no longer touches lp data; `docs/backfills.md` updated
- [ ] **Docs updated** — schema + read path + frontend contract
- [ ] **API types regenerated** — yes, DTOs change
