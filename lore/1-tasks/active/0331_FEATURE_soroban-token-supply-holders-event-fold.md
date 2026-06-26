---
id: '0331'
title: 'FEATURE: bespoke Soroban-token total_supply + holder_count via event-fold MV'
type: FEATURE
status: active
related_adr: ['0043', '0044']
related_tasks: ['0304', '0194', '0210', '0138', '0243']
tags: [clickhouse, soroban, assets, enrichment, effort-medium, milestone-2]
milestone: 2
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/api/src/assets/queries_ch.rs
  - crates/xdr-parser/src/scval.rs
history:
  - date: '2026-06-26'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0304 metadata-validation session. Bespoke Soroban fungible
      tokens (asset_type=3) render "—" for Total supply + Holders in the assets
      list. Prototype validated 2026-06-26 that an event-fold over soroban_events
      reproduces on-chain total_supply() exactly (6/6 sample). New angle vs the
      scoped-out 0138 (storage-scan); covers the type-3 gap that 0194 (types 1,2)
      and 0210 (classic parity) leave open.
  - date: '2026-06-26'
    status: active
    who: karolkow
    note: Promoted to active to start implementation.
---

# FEATURE: bespoke Soroban-token total_supply + holder_count via event-fold MV

## Summary

Compute `total_supply` and `holder_count` for **bespoke Soroban fungible tokens**
(`asset_type = 3`) by folding the standardised SEP-41 `transfer` / `mint` / `burn`
events already ingested in `soroban_events`, and surface them in the assets
list/detail. Today these columns are `—` for every Soroban token.

## Prior art / why this is a distinct task

- **0194 (completed)** populates `total_supply` + `holder_count` for **asset_type
  1 & 2** (classic credit + SAC) by summing `account_balances_current`
  (`asset_aggregates_mv WHERE asset_type IN (1,2)`). **Type 3 is explicitly not
  covered** — bespoke Soroban tokens have no trustlines.
- **0210 (backlog, classic-parity BUG)** extends classic `total_supply` to 4
  Horizon sources; its **Phase 3 ("SAC contract holdings from `contract_data`")**
  overlaps mechanically — the event-fold here is a **better mechanism for that
  Phase too** (standardised events vs a contract-storage scan). 0210 also puts
  `holder_count` parity **out of scope**; this task owns holders for type 3.
  → cross-linked; a note added to 0210.
- **0138 (archived — SCOPE-OUT)** tried exactly this (Soroban token balances) via
  a **`contract_data` storage scan** and was abandoned because **token storage
  key layouts are non-standard** (per-contract `Balance(addr)` schemas). The
  event path **supersedes that approach** — SEP-41 `transfer`/`mint`/`burn` topics
  ARE standardised, so identification is reliable.

## Context

- Soroban token **balances** live in per-holder **persistent `ContractData`**
  (`Balance(addr)` keys), not in `account_balances_current` — so the existing
  aggregate can't see them. (Token **metadata** lives in the single
  `ScContractInstance.storage` map — that is what 0297/0304 extract; balances are
  a different, sprawling storage tier — hence the asymmetry.)
- SEP-41 events are already ingested **decoded** in `soroban_events` (`topics_xdr`
  / `data_xdr` are tagged JSON, not raw XDR). The NFT side already folds
  `mint`/`transfer`/`burn` the same way (`nft_reparse.rs`).

## Validation (prototype, 2026-06-26)

Event-fold `Σmint − Σ(burn+clawback)` matched on-chain `total_supply()` **exactly**
on every sampled token that exposes the getter: nk_ustry, native-USDC-RAUM-LP,
PPRIME-USDC-SOROSWAP-LP, SMOL, AQUA-XTAR-SOROSWAP-LP (264575131),
LIBRE-oUSD-SOROSWAP-LP (487976433). Method = `simulate total_supply()` via
py-stellar-sdk against mainnet RPC. Several tokens (GIGGLE, NRX, pool shares)
**revert `total_supply()` on-chain** — the fold supplies a number the getter can't,
which is part of the value.

## Implementation Plan

### Event shapes (confirmed)

- `topics_xdr` = JSON array `[{"type":"sym","value":"transfer"},{"type":"address","value":"…"},…]`.
  Event name = element 1 `value`.
- `transfer` (3 topics): `[transfer, from, to]`. `mint`: `[mint, admin, to]` (3) **or**
  `[mint, to]` (2). `burn`: `[burn, from]`. (`clawback` rare.)
- `data_xdr` amount = **two shapes**: `{"type":"i128","value":"<dec>"}` (most) **and**
  `{"type":"map","value":[{"key":{…amount},"value":{"type":"i128","value":"<dec>"}}]}`
  (~29832 mints). Extractor MUST handle both. Use **`Int128` / `Decimal256`** (NOT
  `Decimal128(7)` — overflows on 0-dec / huge-supply tokens, e.g. PIKA decimals=43224).

### Aggregation (CRITICAL construction — see CH gotcha #19b)

- **Supply** = `sumIf(amt, ev='mint') − sumIf(amt, ev IN ('burn','clawback'))` grouped by
  `contract_id` (numeric), **with ZERO joins over the event stream**.
- **Holders** = per-address fold: recipient (`+amt`) = **last** topic for `transfer`/`mint`;
  sender (`−amt`) = topic 2 for `transfer`, last topic for `burn`/`clawback`. Then
  `balance = Σdelta` per `(contract_id, address)`, `holder_count = countIf(balance > 0)`.
- Materialise as `soroban_asset_aggregates {contract_id Int64, total_supply, holder_count}`
  (mirror `asset_aggregates_mv`). Decide refreshable-MV (full recompute, simple) vs
  incremental/scheduled based on `soroban_events` size/cost.
- **Resolve `id → strkey/symbol/decimals` SEPARATELY** at read time on the small
  per-page set (GROUP-BY-collapsed CTE per gotcha #13/#19) — never fold + join in one
  full-table query (re-introduces the RMT ×N row multiplication; see #19b — this is the
  ×100 the prototype hit, not a fold error).

### Read path

- `assets/queries_ch.rs`: for `asset_type = 3`, LEFT JOIN `soroban_asset_aggregates` on
  the numeric `contract_id` over the page set. `AssetRow.total_supply` / `holder_count`
  already exist (currently NULL for Soroban) — just populate.

## Acceptance Criteria

- [ ] `soroban_asset_aggregates` (MV/table) computes supply + holder_count from
      `soroban_events` with **no joins over the event stream** (gotcha #19b).
- [ ] Amount extractor handles `i128`-direct **and** `map{amount}` shapes; `Int128`/`Decimal256`.
- [ ] Holders per-address fold covers `transfer`/`mint`/`burn`/`clawback`.
- [ ] Assets list + detail surface supply + holders for `asset_type = 3` (no `—` where
      data exists); id→symbol/decimals resolved on the small per-page set.
- [ ] Supply matches on-chain `total_supply()` on a ≥10-token sample **including** a
      map-shape mint token and the AQUA-XTAR / LIBRE ×100-regression cases; holders spot-checked.
- [ ] Caveats documented: conformant tokens only (non-emitters stay `—`); non-standard
      event shapes logged, not silently mis-summed.
- [ ] **Docs updated** (ADR 0032): `docs/architecture/database-schema/**` (new MV/table) + `endpoint-queries` assets SQL.
- [ ] **API types** — N/A expected (`total_supply`/`holder_count` already in the assets
      response, currently NULL; populating ≠ schema change). Confirm no DTO change at PR time.

## Notes

- **Sequencing — do this AFTER live ingest resumes and catches up (`L_stop` → tip).**
  0331 is read-side (no conflict with ingest running, unlike the 0304 backfill which
  needed it stopped), but the fold reads `soroban_events`: the 0304/0281 window left a
  gap from ~`L_stop`, so deploying the MV before catch-up **under-counts** recent supply.
  The MV also stays fresh only while live ingest keeps writing events. No other blockers
  (assets `asset_type=3` read path already on `ch`).

- Reuses the validated CI check: sample tokens → `simulate total_supply()` (py-stellar-sdk)
  → compare vs the fold. Catches drift from missed/non-standard events.
- Mechanism is **also a candidate for 0210 Phase 3** (SAC contract holdings) — same
  event-fold instead of a `contract_data` scan. Coordinate if both land.

## Future Work

- Refreshable-MV (full recompute) vs incremental aggregation — pick by `soroban_events`
  cost; spawn a follow-up only if the simple MV proves too heavy.
- Non-standard / non-conformant token event shapes (logged class) — separate follow-up if
  the residual is material.
