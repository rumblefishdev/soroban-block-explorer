---
id: '0318'
title: 'FEATURE: /search CH read path — last PG-only module (504 on prod)'
type: FEATURE
status: active
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

- [ ] `/v1/search?q=` returns `200` on prod (CH path), no 504/hang.
- [ ] All entity kinds resolve (tx/account/ledger/contract/asset/LP) matching the
      PG behaviour (and the 0271 shape if landed first).
- [ ] CH queries are PK-prefix seeks, no full-table hash joins (no Code 241).
- [ ] `API_DATASOURCE_SEARCH=ch` flipped in IaC; staging smoke passed.
- [ ] **Docs / API types**: update per ADR 0032 if the contract changes; likely
      `N/A` for a pure read-path swap.
