---
id: '0318'
title: 'FEATURE: /search CH read path — last PG-only module (504 on prod)'
type: FEATURE
status: completed
related_adr: ['0047']
related_tasks: ['0243', '0271']
tags:
  [
    'api',
    'search',
    'clickhouse',
    'gradual-migration',
    'priority-high',
    'layer-api',
  ]
links:
  - crates/api/src/search/
history:
  - date: 2026-06-23
    status: backlog
    who: fmazur
    note: >
      Spawned from the 0243 flip verification. `/search?q=` has no CH read path
      and PG is disabled in prod (DATABASE_URL=disabled), so every call hangs
      and returns 504 after ~29s. Confirmed live: smoke + 504 timing. Search is
      one of the two modules (with NFTs) never migrated to CH.
  - date: 2026-06-24
    status: active
    who: fmazur
    note: >
      Promoted to active. Starting CH read-path implementation for /search.
  - date: 2026-06-24
    status: completed
    who: fmazur
    note: >
      Implemented CH read path (`search/queries_ch.rs`, ~6 buckets) +
      DataSource dispatch. 26 unit tests pass (+5 new), clippy/fmt clean,
      API types unchanged (pure read-path swap, gate green). Validated live
      against local CH (7.8M tx / 246k accounts): all entity kinds resolve,
      no Code 241. 5-agent /code-review max + a 2-agent re-review: found &
      fixed one real bug (Nullable `argMax(name)` → non-Option decode 500,
      latent because metadata was empty), reverted a LEFT-JOIN attempt that
      broke under `join_use_nulls=0`. Remaining: the IaC flag flip
      (`API_DATASOURCE_SEARCH=ch`) + staging/prod smoke — a deploy step of the
      [[0243]] migration, not new dev work.
---

# FEATURE: /search CH read path

## Summary

`GET /v1/search?q=` is the **last** handler module still on the sqlx/PG path
(with NFTs). PG was removed in prod (ADR 0047; `DATABASE_URL=disabled`), so the
endpoint **hangs ~29s and returns 504** — broken for users if the SPA search box
hits it. Give `search` a ClickHouse read path and flip
`API_DATASOURCE_SEARCH=ch`.

## Context

- Verified live (2026-06-23): `/v1/search?q=GAAA` → 504 after 29.2s (PG dial to
  the disabled host times out at the API GW 29s cap).
- Part of the 0243 per-module PG→CH migration; `search` + `nfts` are the only
  two modules never flipped (the other 7 are live on CH).
- **Relation to [[0271]]**: 0271 reworks the search shape (collapse
  `fetch_redirect` into broad + singleton-redirect, option C). Coordinate so the
  CH read path is written against the post-0271 shape rather than redone twice —
  decide order with the team.

## Implementation (outline)

- Add `crates/api/src/search/queries_ch.rs` (mirror the other modules' CH read
  paths): the broad multi-entity search (tx hash, account, ledger, contract,
  asset, LP) as CH queries — PK-prefix seeks, **no full-table hash joins** (see
  the 0317 events bug: a naive `JOIN transactions`/`accounts` builds the hash
  side from the whole table → CH Code 241).
- Wire `DataSource::for_module(Module::Search)` dispatch in the handler.
- Flip `API_DATASOURCE_SEARCH=ch` in `infra/src/lib/stacks/compute-stack.ts`
  once the CH path is validated.
- Until then, consider a fast-fail so search returns a clean error instead of a
  29s hang (avoid the PG dial timeout) — optional harm-reduction.

## Acceptance Criteria

- [ ] `/v1/search?q=` returns `200` on prod (CH path), no 504/hang. _(pending the
      flag flip + deploy; validated locally against local CH)_
- [x] All entity kinds resolve (tx/account/contract/asset/nft/pool) matching the
      PG behaviour (post-0271 shape). Validated live on local CH.
- [x] CH queries are PK-prefix seeks / bounded key-seeks, no full-table hash joins
      (no Code 241) — confirmed by review + measured read_rows.
- [ ] `API_DATASOURCE_SEARCH=ch` flipped in IaC; staging smoke passed. _(deploy
      step of the [[0243]] migration)_
- [x] **Docs / API types**: `docs/.../endpoint-queries-clickhouse/22_get_search.sql`
      updated per ADR 0032; API types `N/A` (pure read-path swap — `nx generate`
      produced no diff, freshness gate green).

## Implementation Notes

- **`crates/api/src/search/queries_ch.rs`** (new) — mirrors `queries::fetch_search`
  signature/return (`Vec<(String, SearchHit)>`), so the handler is
  backend-agnostic. Six per-entity buckets fired **concurrently**
  (`tokio::try_join!`) and **classification-gated** so only buckets that can match
  run (hash mode → tx+pool; strkey-prefix → account+contract-prefix+asset+nft;
  text → contract-name+asset+nft). This is the minimal-rows-scanned shape.
- **`handlers.rs`** — `DataSource::for_module(Module::Search)` dispatch (Pg arm
  byte-identical to before; Ch arm → `state.ch()`).
- **`mod.rs`** — `mod queries_ch;`.
- Files: 1 new (~840 lines incl. tests), 2 modified, 1 doc updated. 26 unit tests
  (+5 new: route_token×3, asset_type_name, CH_URL-gated decode_smoke).

## Issues Encountered

- **Nullable→non-Option decode 500 (FIXED).** `argMax(name, version)` over the
  `Nullable(String)` `soroban_contract_metadata.name` projects a Nullable wire
  column; decoding into `ContractNameRow.name: String` mismatches → 500. Latent in
  local testing because the metadata table was empty. Fix: `ifNull(argMax(...), '')`
  on both contract projections. Verified live after inserting a `name=NULL` row
  (now 200, empty label).
- **`account`/`contract` prefix + `FINAL` scaled with prefix breadth.** A 2-char
  prefix (`GA`) under `FINAL` read 98k/246k rows locally (~9M on prod 23M). Dropped
  `FINAL` + `ORDER BY pk LIMIT` (early-terminating, ~32k constant) + Rust
  adjacent-dedup. Measured 98k→32k.
- **tx-detail ledger lookup.** A scalar-subquery form de-optimised to a full
  `ledgers` scan (25k); a plain `INNER JOIN ledgers` builds a hash over all
  ledgers on prod. Fixed with a bounded `INNER JOIN (SELECT … WHERE sequence=L)`
  point-seek. A LEFT-JOIN variant (attempted for PG parity) regressed under the
  `api_reader` `join_use_nulls=0` RBAC (non-null default, not NULL) → reverted.

## Design Decisions

### From Plan

1. **Mirror the 0243 per-module CH pattern** — `queries_ch.rs` beside `queries.rs`,
   `DataSource::for_module` dispatch, same return shape.
2. **Written against the post-0271 broad shape** (six CTEs, no `fetch_redirect`),
   already landed on PG — no double work.
3. **No full-table hash joins** (0317 Code 241 lesson) — issuer/contract resolution
   via bloom-pruned `accounts WHERE id IN (page ids)` key-seeks.

### Emerged

4. **Per-bucket concurrent queries, not one UNION** — the PG UNION evaluates all
   six branches every call; firing only the satisfiable buckets (gated by the
   classifier's 3 modes) minimises rows scanned. Chosen over a literal UNION port.
5. **NFT name from `nft_enrichment`, contract name from `soroban_contract_metadata`**
   — both `nfts.name` and `soroban_contracts.name` are vestigial NULL on CH; the
   canonical SQL doc's `nfts.name`/`soroban_contracts.name` predicates would match
   nothing. Corrected the doc too.
6. **Skip small-table substring buckets in hash mode** — a 64-hex/56-char needle
   can't substring a ≤12-char asset code (provably empty) and a contract/NFT named
   after a tx hash isn't real intent. PG runs them (wasteful, same result).
7. **tx enrichment (`successful`+`last_activity_at`) kept for PG parity** via a
   bounded `transactions`+`ledgers` seek, though the canonical CH SQL had dropped
   it. `ledgers.closed_at` == PG `thi.created_at` (same instant).
8. **Account/contract dedup in Rust (keep-first)** after dropping `FINAL` — rare
   re-ingest dupes are contiguous; may very rarely yield <limit rows (acceptable
   for a dropdown; same as sibling CH list modules).

## Future Work

- **Flip `API_DATASOURCE_SEARCH=ch`** in `infra/.../compute-stack.ts` + staging/prod
  smoke (operator read-rows/memory check on the `assets` substring scan and the
  contract-name/nft metadata scans at prod table sizes). This is the final deploy
  step of the [[0243]] migration, not new dev work — no separate task.
- Optional, only if those bounded state tables grow large: `tokenbf_v1` skip index
  on `asset_code` / `soroban_contract_metadata.name` / `nft_enrichment.name`.
