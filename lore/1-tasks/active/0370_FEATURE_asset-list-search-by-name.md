---
id: '0370'
title: 'FEATURE: asset-list search matches display name/symbol, not just asset_code (find Soroban type-3 tokens)'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0371']
tags: ['backend', 'api', 'search', 'assets', 'effort-small']
links: []
history:
  - date: 2026-07-10
    status: backlog
    who: stkrolikiewicz
    note: >
      Created from an Ada (user) report: Soroban-native RWA tokens (Solv BTC,
      deJTRSY / Centrifuge deRWA, xSolvBTC) are indexed as asset_type=3 but are
      unsearchable in the Asset List view — filter[code] matches only the
      (empty) asset_code. Broaden the list search to the already-joined display
      name/symbol.
  - date: 2026-07-10
    status: active
    who: stkrolikiewicz
    note: 'Promoted to active; starting implementation.'
---

# FEATURE: asset-list search by name/symbol

## Summary

The Asset List endpoint (`GET /v1/assets`, `filter[code]`) matches only
`assets.asset_code`. Soroban-native tokens (`asset_type = 3`) are stored with an
**empty** `asset_code` — their display name/symbol are resolved from joined
`soroban_contract_metadata` / `asset_enrichment` at read time — so they are
impossible to text-search in the Asset List, even though they are fully indexed
and browsable. Broaden the list's search predicate to also match the
already-joined name/symbol columns.

## Context

User report (Ada, 2026-07-10): _"I don't see the Solv or Centrifuge assets under
Soroban or SAC."_ The real tokens are Soroban SEP-41 contracts, indexed as
`asset_type = 3` (verified on prod):

- Solv BTC — `CBIJBDNZNF4X35BJ4FFZWCDBSCKOP5NB4PLG4SNENRMLAPYG4P5FM6VN`
- deJTRSY (Centrifuge deRWA) — `CBI7UCH5KGSVQRO5H4SUCZUTZABCITZLRHQQZTWL2TK4RZ72TAR6IHRV`
- xSolvBTC — `CAUP7NFABXE5TJRL3FKTPMWRLC7IAXYDCTHQRFSCLR5TMGKHOOQO772J`

Root cause:

- type-3 asset rows are written with an empty `asset_code`
  (`crates/db-clickhouse/src/persist/stage.rs` ~L1231:
  `AssetRow::staged(Soroban, "", 0, contract_id)`).
- The list search filters only `a.asset_code`:
  `AND positionCaseInsensitive(a.asset_code, ?) > 0`
  (`crates/api/src/assets/queries.rs:403`).
- The display name is resolved from joined `soroban_contract_metadata.name/symbol`
  - `asset_enrichment.name` — already present in `ASSET_LIST_CH_SELECT` (aliases
    `m`, `ae`) but used only for output, not filtering.

Global search `/v1/search` already finds these via its `contract` bucket by
on-chain name, but they surface as _contracts_, not under the asset facets; its
`asset` bucket carries the same `length(asset_code) > 0` guard
(`crates/api/src/search/queries.rs:637`).

## Implementation Plan

### Step 1: Broaden the list search predicate

In `fetch_list` (`crates/api/src/assets/queries.rs`), extend `code_clause` (only
when a search term is present) to OR the already-joined columns:

```
AND (positionCaseInsensitive(a.asset_code, ?) > 0
  OR positionCaseInsensitive(coalesce(m.name, ''),   ?) > 0
  OR positionCaseInsensitive(coalesce(m.symbol, ''), ?) > 0
  OR positionCaseInsensitive(coalesce(ae.name, ''),  ?) > 0)
```

Bind the needle 4× (currently 1×). No new join/table — `m`/`ae` are already in
`ASSET_LIST_CH_SELECT`. The browse path (no term) is unchanged.

### Step 2: Param decision

- **v1 (recommended):** reuse `filter[code]` predicate → no DTO/OpenAPI change →
  no `api-types` regen. Frontend search box works unchanged; update its
  placeholder to "code or name".
- **Alternative:** add an additive `filter[q]` param (clearer contract) →
  OpenAPI change → `nx run @rumblefish/api-types:generate` + minor FE wiring.

### Step 3 (optional): global-search asset-bucket parity

Consider mirroring in `search_assets` (`crates/api/src/search/queries.rs:637`)
so type-3 also match the asset bucket by name. Lower priority — the contract
bucket already covers them.

## Acceptance Criteria

- [ ] `GET /v1/assets?filter[code]=Solv` (and `deJTRSY`, `xSolvBTC`) returns the
      type-3 token(s); `filter[type]=soroban` + search works.
- [ ] Browse path (no search term) query is byte-identical to today (no
      perf regression on the common list/paging path).
- [ ] One `fetch_list` test: a type-3 token is returned by its metadata name and
      by its symbol.
- [ ] **Docs updated** — N/A unless the asset-search contract is documented in
      `docs/architecture/backend/**`; if so, note the widened `filter[code]`
      semantics. Confirm during impl.
- [ ] **API types regenerated** — N/A if v1 reuses `filter[code]` (no DTO
      change). Required only if a new `filter[q]` param is added.

## Notes / Non-goals

- "Centrifuge" (project name) still won't match — the on-chain name is
  `deJTRSY` and type-3 has no issuer/`home_domain`. Curated project-name / domain
  search is tracked in **0371** (low priority).
- No relevance ranking — results keep the natural keyset order
  `(asset_type, asset_code, issuer_id, contract_id)`.
- Perf is safe: `assets` is a small state table; the metadata/enrichment joins
  already run per request; issuer is resolved by a bloom-pruned key-seek (no
  `accounts` OOM). Readonly `join_use_nulls = 0` → a join miss is `''` (hence the
  `coalesce`).
