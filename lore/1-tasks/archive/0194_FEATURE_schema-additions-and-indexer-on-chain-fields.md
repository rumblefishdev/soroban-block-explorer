---
id: '0194'
title: 'Indexer recompute for assets.total_supply + assets.holder_count'
type: FEATURE
status: completed
related_adr: ['0032', '0043']
related_tasks:
  ['0119', '0135', '0156', '0188', '0191', '0195', '0196', '0197', '0199']
tags: [layer-indexer, layer-db, milestone-2]
milestone: 2
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-05-06'
    status: backlog
    who: karolkow
    note: 'Spawned from M2 enrichment planning. First of four tasks (0194-0197) under the ADR 0043 field-allocation rule (list-endpoint + on-chain → indexer).'
  - date: '2026-05-06'
    status: active
    who: karolkow
    note: 'Activated. Branch cut from develop; 0191 SQS enrichment merged in for context. Sub-blocks 1b + 1c + 1e landed. 1a (speculative usd_price columns) pulled as YAGNI.'
  - date: '2026-05-07'
    status: active
    who: karolkow
    note: 'Sub-block 1d (LP volume + fee_revenue) pulled after correctness review (snapshot-delta nets opposite swaps; no USD reference). Re-spawned as task 0199. Final scope: 1b + 1c + 1e (verify).'
  - date: '2026-05-11'
    status: completed
    who: karolkow
    note: 'Closed. Recompute fn (`recompute_asset_aggregates`) shipped + observability (`aggregates_ms`) + docs updated (db-schema §4.10/§4.15, indexing-pipeline §5.2 step 14, audit §9.3 supersede note, endpoint queries 08/09). Benchmarked +4% per-ledger overhead; +1ms p99 on aggregates step. Full Horizon-parity total_supply (LP reserves + claimable_balances + SAC contract holdings) explicitly deferred to Future Work.'
---

# Indexer recompute for `assets.total_supply` + `assets.holder_count`

## Summary

Populate two long-NULL list-endpoint columns on `assets` via per-ledger recompute in the indexer, on-chain-derivable from `account_balances_current`. Per [ADR 0043](../../2-adrs/0043_field-allocation-rule.md): list-endpoint + on-chain → indexer (not Lambda 2).

## Final Scope

- **§1b — `assets.total_supply` (classic credit)** — `SUM(account_balances_current.balance)` per touched `(code, issuer_id)`.
- **§1c — `assets.holder_count`** — `COUNT(*) FILTER (WHERE balance > 0)` (active-holder semantics, matching StellarExpert / Stellarchain.io convention). Supersedes blocked task 0135.
- **§1e — `account_balances_current` trustlines** — verify-only. Task 0119 (FilipDz, completed 2026-04-15) already populates non-XLM trustline rows; recompute relies on that path.

Implementation: one new fn `recompute_asset_aggregates` in `crates/indexer/src/handler/persist/write.rs`, called after `upsert_balances` in `run_all_steps`. Single UPDATE with `UNNEST + LEFT JOIN LATERAL`, `COALESCE` so fully-removed asset rows land on `0/0`. Index-only seek via `idx_abc_asset (asset_code, issuer_id)`. `aggregates_ms` timing added to `StepTimings` for CloudWatch observability.

## Pulled Sub-Blocks

| Sub-block | What                                                                    | Where it went                                                                                                                                                                                                         |
| --------- | ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| §1a       | `assets.usd_price` + `usd_price_updated_at` columns + 5 partial indexes | Future-work — speculative, no shipped sort variant uses it. `0195 §2c (asset_usd_price kind)` also dropped from M2.                                                                                                   |
| §1d       | `liquidity_pool_snapshots.volume` + `fee_revenue`                       | **Task 0199** — initial snapshot-delta approach netted opposite swaps within a ledger and lacked USD reference. Proper fix needs per-op PathPayment `claimedOffers[].amount_sold` + USD oracle (blocked on 0195 §2b). |

## Acceptance

- [x] §1b shipped — `recompute_asset_aggregates` SUM per `(code, issuer_id)`.
- [x] §1c shipped — same fn, COUNT FILTER `balance > 0`.
- [x] §1e verified — `upsert_balances_credit` populates all NOT NULL columns on non-XLM rows.
- [x] ADR 0043 on develop (landed independently as `745e56b` + `148bf3c`).
- [x] Docs updated per ADR 0032: `docs/architecture/database-schema/database-schema-overview.md` §4.10 + §4.15; `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` §5.2 step 14; `docs/audits/2026-04-10-pipeline-data-audit.md` §9.3 supersede note; endpoint-queries `08_get_assets_list.sql` + `09_get_assets_by_id.sql`.
- [x] `cargo check -p indexer + clippy --all-targets -D warnings + cargo test -p indexer --lib` clean.
- [x] `nx run @rumblefish/api-types:check-generated` clean (no DTO changes — `holder_count` / `total_supply` already on existing DTOs).

## Future Work (no follow-up task — fold into the right place when needed)

- **Full Horizon-parity `total_supply`** — current MVP sums only trustlines. Horizon `/assets` aggregates 4 sources (trustlines + claimable_balances + LP reserves + SAC contract holdings). Known DeFi-asset drift up to ~20-50% (USDC w/ Soroswap + SAC). Adding LP reserves is trivial (schema in place, prototyped in implementation Round 4). Adding `claimable_balances` extraction + per-asset SAC contract holdings tracking is dedicated design work; defer until drift becomes user-visible.
- **`assets.usd_price`** — column + indexes + Lambda 2 population deferred. Revisit when a real product ask materialises.
- **Classic credit `assets.name`** — off-chain (issuer SEP-1 TOML `CURRENCIES[].name`) → owned by **0195 §2a** (icon kind extended to also persist `name`).
- **`holder_count` + `total_supply` bulk recount for dormant assets** — per-ledger recompute only visits assets touched by this ledger. Dormant rows stay at their pre-deployment state until something touches them. **Scope for task 0196** (enrichment-backfill crate) — same crate that drains pre-existing un-enriched rows for 0195's columns.

## Performance Note

Benchmarked on local Docker PG 17.6, two ranges (500 + 1500 ledgers). Per-ledger overhead: **+4%** mean (vs paper estimate +8%); `aggregates_ms` p99 = 1ms, max = 3ms. 10-year backfill projection: **+5–8 hours wall-clock** across full pubnet on 16 parallel workers; **<$10 RDS** marginal cost.
