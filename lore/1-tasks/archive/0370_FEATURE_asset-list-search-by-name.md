---
id: '0370'
title: 'FEATURE: asset-list search matches display name/symbol, not just asset_code (find Soroban type-3 tokens)'
type: FEATURE
status: completed
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
  - date: 2026-07-10
    status: completed
    who: stkrolikiewicz
    note: >
      Shipped in PR #326 (merged to develop 2026-07-10, commit 8d2205c6). 1 code
      file (crates/api/src/assets/queries.rs): broadened list filter[code] to
      also match joined soroban_contract_metadata name/symbol; extracted
      build_list_sql + 2 unit tests. Cargo.lock ethnum 1.5.2->1.5.3 to fix a CI
      E0512 (unrelated to feature). No api-types regen (0 diff). Verified via
      unit tests + prod before/after SQL + full local docker-CH demo; perf +6ms
      median on search only (0 on browse, identical rows_read). Follow-up 0371.
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

- [x] `GET /v1/assets?filter[code]=Solv` (and `deJTRSY`, `xSolvBTC`) returns the
      type-3 token(s); `filter[type]=soroban` + search works — verified on prod
      (before/after SQL) AND via a full local API demo.
- [x] Browse path (no search term) query is byte-identical — the predicate is
      only added when a term is present.
- [x] Unit test for the predicate — added on `build_list_sql` (asserts the name/
      symbol predicates + the `?`/bind count). A live `fetch_list` CH test was
      not added (no CH unit harness in this module); the SQL-builder test is the
      data-free proxy.
- [x] **Docs updated** — N/A (no system-shape change: search-predicate widening,
      no schema / endpoint / DTO change).
- [x] **API types regenerated** — N/A (reused `filter[code]`; regen produced 0
      diff, confirmed locally).

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

## Implementation Notes

- **1 code file:** `crates/api/src/assets/queries.rs`. Extracted `build_list_sql`
  (pure fn), broadened `code_clause` to 3 predicates (`asset_code` + `m.name` +
  `m.symbol`), bind ×3, +2 unit tests (predicate shape + placeholder/bind count).
  `Cargo.lock`: `ethnum` 1.5.2→1.5.3.
- **Frontend:** unchanged — the Asset List search box already sends `filter[code]`.
- **Verified 3 ways:** unit tests + clippy; prod before/after SQL (type-3 tokens
  surface, `rows_read` identical); full local demo (docker `clickhouse-server`
  seeded with the real token rows + `cargo lambda watch` → real handler→CH→DTO
  path returned Solv BTC / xSolvBTC / deJTRSY).
- **Perf (prod measured):** +6 ms median on the _search_ path only (0 on
  browse/pagination), no extra I/O (identical `read_rows`).
- **Shipped:** PR #326, merged to develop (`8d2205c6`, 2026-07-10).

## Design Decisions

### From Plan

1. **Broaden `filter[code]`** to also match the joined display name/symbol — the
   core fix.
2. **Reuse `filter[code]`** (no new `filter[q]`) → no DTO/OpenAPI change → no
   `api-types` regen (Step 2 v1).

### Emerged

3. **Excluded `asset_enrichment.name` from the match.** The plan listed 4
   predicates (incl. `ae.name`). Prod before/after verification showed `ae.name`
   (classic SEP-1 names, ~360k rows) adds substring noise — "Opulent Insolvent"
   matches "solv" — for no gain, since classic assets are already findable by
   their non-empty code. Scoped to `m.name`/`m.symbol` (contract metadata,
   type-3 only); bind ×3, not ×4.
4. **Extracted `build_list_sql` as a pure fn.** Not in the plan; needed to
   unit-test the predicate shape + placeholder/bind contract without a live CH.
5. **Bumped `ethnum` 1.5.2→1.5.3 (`Cargo.lock`).** Out of scope but required for
   green CI — the newer CI Rust toolchain broke `ethnum 1.5.2` (`E0512`,
   `mem::transmute(())` on the now-non-ZST `TryFromIntError`), failing both the
   Rust job and API-types freshness. Repo-wide transitive dep via stellar-xdr 26
   (also independently resolved on develop by 0368's stellar-xdr 27 bump, #325).

## Issues Encountered

- **`ethnum 1.5.2` E0512 on the CI toolchain** — both CI checks red, not the
  feature. Fixed by the 1.5.3 bump. The "API types freshness" red was _masked_ by
  this compile error (`extract_openapi` could not build); the regen itself
  produced 0 diff (spec unchanged).
- **Local e2e could not reach prod CH** — the box's sshd blocks port-forwarding
  (`administratively prohibited`), so no tunnel. Pivoted to a local seeded docker
  ClickHouse + `cargo lambda watch` with a throwaway mTLS→plain `main.rs` patch
  (reverted before commit).

## Future Work

- **0371** (low): search by project/brand name or issuer domain ("Centrifuge" →
  deJTRSY). Needs off-chain data — a curated directory or `stellar.toml`
  `ORG_NAME`; on-chain XDR yields at most a deployer-derived domain, not the
  brand. Already spawned as a backlog task.
- **Optional:** global-search `asset` bucket parity for type-3 (Step 3) — the
  `/v1/search` `contract` bucket already finds them by name, so deferred.
