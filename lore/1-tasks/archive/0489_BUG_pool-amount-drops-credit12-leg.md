---
id: '0489'
title: 'BUG: the pool Amount column drops every credit_alphanum12 leg'
type: BUG
status: completed
related_adr: ['0051']
related_tasks: ['0279', '0490', '0491']
tags: [api, clickhouse, layer-backend, priority-high, effort-small]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/371'
history:
  - date: '2026-08-17'
    status: active
    who: stkrolikiewicz
    note: >
      Found within the hour after the first CI tag release
      (production-2026.08.17-1) shipped 0279's Amount column to production.
      A trade renders one-sided instead of `A → B`; root cause proven on prod
      the same session. Read-path only, no re-index needed.
  - date: '2026-08-17'
    status: completed
    who: stkrolikiewicz
    note: >
      Shipped in production-2026.08.17-2 and verified on production the same
      day. Pool LCGK…CFJT (yXLM type 1 / CETES type 2) renders
      `0.0025 yXLM → 0.0056817 CETES`, and Horizon's `liquidity_pool_trade`
      effect for that operation reports `sold 0.0056817 CETES / bought
      0.0025000 yXLM` — signs, amounts and direction all agree. 3 files of
      code, 1 new helper, 1 call site; 30 + 9 tests. Two follow-ups spawned
      (0490, 0491). One acceptance criterion was found to be unmeasurable and
      is rewritten below rather than ticked.
---

# The pool Amount column drops every credit_alphanum12 leg

## Summary

`/liquidity-pools/:id/transactions` returns `null` for any leg that is a
`credit_alphanum12` asset, so the frontend cannot render the `A → B` pair issue
#371 asked for. The data on disk is correct and complete — this is a surrogate
mismatch on the read path, fixable in one expression, with no re-parse and no
backfill.

**Measured on prod 2026-08-17: 279,452 of 1,738,948 recent pool operations
(16.1%) are affected**, in two shapes:

| pool                                        | legs resolved | rendered                  |
| ------------------------------------------- | ------------- | ------------------------- |
| type 2 beside type 0/1 (240,751 ops)        | one           | trade shown **one-sided** |
| both legs type 2 (38,701 ops, 13,703 pools) | neither       | **nothing at all**        |

The blank case is why this survived review and a release. An empty Amount cell
is the _documented_ signal for "the amount index has not reached this row yet"
(`PoolTransactions.tsx`), so on a 2/2 pool the bug is indistinguishable from a
pending backfill.

## Context

Task [0279](./0279_FEATURE_lp-op-details-amount-column.md) indexed per-(operation,
pool, asset) amounts into `lp_operation_amounts` and un-hid the Amount column
on the pool detail page. It shipped in the first tag-driven release.

Issue #371 asked for the stellar.expert reading of a trade — both legs with a
direction, `12,059 XLM → 38.5M KALE`. The frontend already implements exactly
that: `formatPoolAmount` builds the arrow form when both legs are present with
opposite signs, and `PoolTransactions.test.tsx` pins `100 XLM → 40 USDC`. The
test fixture is an XLM/USDC pool, which is asset types 0/1 — the two types that
happen to work — so the suite stayed green over a bug affecting a sixth of
production traffic.

## Root cause

Two different meanings of `asset_type = 2` in two different tables.

`ids::asset_id(asset_type, code, issuer_id, contract_id)` handles only `0`
(native) and `1` (classic credit). Everything else falls through to
`_ => contract_id`. Its comment states the assumption:

> the retired type-2 (SAC) also lands here

That is true of the `assets` table, where ADR 0051 / task 0339 re-keyed SAC
rows to the classic id and left the facet space as `{0, 1, 3}`. It is **not**
true of `liquidity_pools`, whose `asset_a_type` / `asset_b_type` carry the raw
XDR asset type, where `2` means `credit_alphanum12` and is very much alive.

So the two sides disagree:

| side                                        | call                                                                         | credit12 result           |
| ------------------------------------------- | ---------------------------------------------------------------------------- | ------------------------- |
| writer — `persist/stage.rs`                 | `credit_asset_id(code, issuer)` = `asset_id(1, code, account_id(issuer), 0)` | real hash                 |
| reader — `queries.rs::fetch_pool_asset_ids` | `asset_id(asset_a_type, code, issuer_id, 0)` with `asset_a_type = 2`         | `contract_id`, i.e. **0** |

`asset_id = 0` is never stored, so the join for that leg cannot match and the
handler emits `null`.

### Evidence gathered on prod

```
-- 1. liquidity_pools uses the XDR type space; 2 is alive
asset_a_type  asset_b_type  pools
1             2             23515
1             1             22100
2             2             13703
0             1              8825
0             2              7648
-- 44,866 of 75,791 pools (59%) carry at least one type-2 leg

-- 2. the indexer writes BOTH legs
n_legs  ops
1          165      -- legitimate net-to-zero cases
2    1,738,956

-- 3. asset_id = 0 is never stored
zero_rows  total
0          3,478,077

-- 4. concrete: pool 8CA53441… (yXLM type 1 / CETES type 2)
ledger    asset_id             amount
63992676  4032595941348833451  +129951   -- CETES, dropped
63992676   258332573254456524   -56712   -- yXLM,  renders
```

The leg↔id mapping above is pinned by
`pool_leg_surrogates_match_production_rows`, not eyeballed — an earlier note in
this file had the two ids the other way round.

## Implementation

A pool leg is always a classic asset — native or credit, never a contract — so
the reader must not route it through the SAC-aware branch at all.

In `fetch_pool_asset_ids` (`crates/api/src/liquidity_pools/queries.rs`), resolve
each leg as native-or-classic instead of passing the raw XDR type through:

```rust
// Types matched EXPLICITLY, not `0 => native, else => credit`: an `else`
// hands a future type the credit formula and a well-formed id that matches
// nothing — the same silent blank, minus the obviously bogus 0 that made
// this one findable. Soroban-AMM legs (issue #405) would be exactly that.
pub fn pool_leg_asset_id(asset_type: i16, asset_code: &str, issuer_id: i64) -> i64 {
    match asset_type {
        0 => NATIVE_ASSET_ID,
        1 | 2 => asset_id(1, asset_code, issuer_id, 0),
        other => { /* debug_assert! + tracing::warn!, never panic */ }
    }
}
```

Leave `ids::asset_id` alone — `2` has to keep meaning SAC for the paths that
read the `assets` facet space, and widening it there would break them.

## Acceptance Criteria

- [x] The leg surrogates the API computes equal the `asset_id` values
      production actually stores — pinned against pool `8CA53441…`
      (yXLM type 1 / CETES type 2) in
      `pool_leg_surrogates_match_production_rows`
- [x] A regression test whose fixture leg is type 2 — the existing suite was
      `"TF"` (type 1) and agreed with the bug
- [x] `native / credit4 / credit12` all covered, so the next type mix-up fails
      a test instead of a sixth of production
- [x] Proven to catch the bug: with the old resolution restored the test fails
      `left: 0, right: 3098242843307699806`, not merely passes with the fix
- [x] No re-parse and no backfill — read path only, one expression changed
- [x] A trade on such a pool renders `A → B` on the page, checked on prod
      after deploy, and against Horizon for the same operation — pool
      `LCGK…CFJT`, tx `24d04961…c5b5`: the page shows
      `0.0025 yXLM → 0.0056817 CETES`, Horizon's `liquidity_pool_trade` effect
      for pool `8ca53441…` reports `sold 0.0056817 CETES / bought 0.0025000
yXLM`. Both trade directions render on that page
- [x] ~~Post-deploy: re-run the blast-radius query and confirm the one-sided
      share drops to the net-to-zero floor (~0.01%)~~ — **this criterion was
      unmeasurable as written.** The blast-radius query counts operations on
      pools that HAVE a type-2 leg, which is a property of the pool
      definitions, not of the reader. A read-path fix cannot move it, and it
      did not (15.54% after deploy, against 16.1% before — the drift is new
      ledgers entering the window, nothing else). What the query CAN show is
      the data floor it was meant to compare against: 162 of 1,812,838
      operations carry a single leg, 0.0089%, and those are the genuine
      net-to-zero cases. The read-side proof is the criterion above; a SQL
      count over ClickHouse was never going to prove anything about the API
- [x] **Docs updated** — per ADR 0032,
      `20_get_liquidity_pools_transactions.sql` now states which resolver the
      leg uses and why the other one is wrong
- [x] **API types regenerated** — ran `@rumblefish/api-types:generate`, zero
      drift, so the `API types freshness` gate is green. N/A confirmed rather
      than assumed, since `crates/api/**` is touched

## Implementation Notes

Three files, +1 helper, one call site changed.

- `ids::pool_leg_asset_id(asset_type, code, issuer_id)` — native for type 0,
  the single classic-credit surrogate for everything else. Lives next to
  `credit_asset_id` so the surrogate scheme stays in one module, as that
  module's own header asks.
- `fetch_pool_asset_ids` calls it instead of `ids::asset_id`.
- `ids::asset_id` is deliberately untouched: `2` must keep meaning the retired
  SAC facet for the paths reading the `assets` enum, and widening it there
  would break them to fix this.

Verification: 30 `liquidity_pools` tests, 90 `db-clickhouse` tests, clippy
clean on both crates.

## Issues Encountered

- **An acceptance criterion that could not be measured.** "Re-run the
  blast-radius query and confirm the share drops" sounded like verification
  and was not: the query counts pool _definitions_, which a read-path fix
  never touches. Writing it felt rigorous because it carried a number. The
  lesson is narrow and reusable — when a fix changes what the API _answers_,
  the proof has to come from the API, not from the store behind it.
- **A leg↔id mapping stated from inference.** The two `asset_id` values were
  first labelled by eyeballing which asset the screenshot rendered. The golden
  test proved them inverted. Corrected in place, with the correction called
  out, since the raw numbers had already been quoted.

## Design Decisions

### From Plan

1. **Fix at the reader, not in `asset_id`.** The two callers disagree about
   what `2` means; only one of them is wrong about its own table.

### Emerged

2. **A second test against real production values.** The first test only
   proves this module is self-consistent — both sides reduce to the same
   `asset_id(1, …)` call, so they could drift together and stay green. Pinning
   the two `asset_id` values prod actually stores makes the equality answerable
   from outside the code. Found in review of this task's own diff.
3. **Explicit `match 0 | 1 | 2`, not an `else`.** A total function over `i16`
   hands any future type the credit formula and a well-formed id that matches
   nothing — the same silent blank, harder to spot than the old `0`. Soroban-AMM
   indexing (issue #405) would bring exactly that leg. `debug_assert!` so a test
   fails, `tracing::warn!` so prod says something, and no panic: a wrong id
   blanks a cell, a panic takes down a read path over a display value.
4. **Left the mapping wrong-way-round note in place.** The evidence block first
   labelled the two ids inverted; corrected, and the correction is called out
   rather than quietly overwritten, because the raw numbers were already quoted
   in the session that found the bug.

## Future Work

- [0490](../backlog/0490_BUG_pool-amount-cell-row-height-unbounded.md) — the
  Amount cell stacks one line per operation and an arbitrage bundle produces
  ten of them under a single `Trade` chip that maps to none of them. Note this
  fix WIDENS every line to the `A → B` form, so it makes 0490 worse first.
- [0491](../backlog/0491_FEATURE_pool-activity-per-operation-rows-and-trades-filter.md)
  — the structural answer (one row per operation) plus the trades filter,
  which are the two thirds of #371 that 0279 did not cover.
- The same XDR-type-vs-facet-type collision may exist on other read paths that
  feed `liquidity_pools.asset_*_type` into `ids::asset_id`. Worth one grep
  before closing.
