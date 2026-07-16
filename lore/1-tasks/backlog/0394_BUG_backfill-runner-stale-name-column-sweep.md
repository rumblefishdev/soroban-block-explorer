---
id: '0394'
title: 'backfill-runner + enrichment-runner: sweep remaining stale `name` column references (0304 drop)'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0388', '0392', '0304', '0359']
tags: ['effort-small', 'priority-high', 'clickhouse', 'backfill-runner']
links: []
history:
  - date: 2026-07-16
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0388 — that task fixed the `name` drop in ONE site
      (repair_tier1). Code-review of 0392 PR #341 (which fixed the live-indexer
      copy) surfaced the un-swept siblings still on develop. Same failure class,
      three more call sites.
---

# backfill-runner + enrichment-runner: sweep remaining stale `name` references

## Summary

Task **0304** dropped the `name` column from `soroban_contracts` and `assets`.
Three separate PRs each fixed one copy of the resulting broken SQL — **0388**
(`repair_tier1`), **0392 / PR #341** (live-indexer `fetch_prior_contract_rows`).
Three call sites in `backfill-runner` and `backfill-enrichment-runner` still
reference the dropped column and were missed by all of them. This task sweeps
the remainder so no maintenance pass aborts on `Code 47 UNKNOWN_IDENTIFIER` /
`NO_SUCH_COLUMN_IN_TABLE`.

## Context

The `name` drop was verified live on prod: `repair-tier1`'s deployer
reconstruction aborted on `unknown column name` (0388), and PR #341's CloudWatch
evidence shows `Code 47` from a SELECT touching only `soroban_contracts`. So the
`soroban_contracts.name` sites below are **broken on prod today**; the
`assets.name` sites are latent (the prod `ALTER … DROP COLUMN name` on `assets`
still batches with 0310's deploy-drain per init.sql) but already break on any
fresh init.sql schema, including the CH-gated e2e tests.

Relevance to **0392**: `contract-type-rebuild` is the "contract-type-rebuild-
equivalent" that 0392 Step 1's continuous reconcile is meant to trigger on — it
cannot be a reconcile building block while it aborts on the first row.

## Call sites (verified on `origin/develop`)

Production SQL — `soroban_contracts.name` (broken on prod now):

- `crates/backfill-runner/src/contract_type_rebuild.rs:203` — `build_staging`
  SELECTs `sc.name`; staging is created `LIKE soroban_contracts` (8 cols), so a
  9-col SELECT double-fails (unknown identifier + column count).
- `crates/backfill-runner/src/wasm_upgrade_backfill.rs:222` — same `sc.name` in
  `build_staging`. This is the pass PR #341's fail-open warn names as the 0320
  recovery path, so its being broken defeats that recovery contract.

Production SQL — `assets.name` (latent on prod, broken on fresh schema):

- `crates/backfill-runner/src/contract_type_rebuild.rs:271` — `backfill_assets`
  INSERTs into `assets(…, name, …)`.
- `crates/backfill-enrichment-runner/src/main.rs:777` — SEP-1 chunk-selection
  test seeds `INSERT INTO assets(…, name, …)`. (The nearby `asset_enrichment` /
  `nft_enrichment` inserts at 788/845/890 keep their own real `name` columns —
  do **not** touch those.)

CH-gated e2e seeds (fail at the seed INSERT against init.sql, so the tests that
would catch the above cannot even run):

- `crates/backfill-runner/src/contract_type_rebuild.rs:361`
- `crates/backfill-runner/src/wasm_upgrade_backfill.rs:307`

## Implementation

- Drop `name` from each SELECT / INSERT column list above (mirror the fix
  already applied in `repair_tier1.rs` and `persist.rs::fetch_prior_contract_rows`).
- For the `build_staging` sites, confirm the staging table shape matches the
  post-0304 8-column `soroban_contracts` so the `INSERT … SELECT` column counts
  line up.
- Repair the three e2e seed fixtures (same edit already made in
  `db-clickhouse/tests/{smoke,metadata_e2e}.rs` under PR #341).
- Run each CH-gated e2e against a local docker ClickHouse to confirm green.

## Acceptance Criteria

- [ ] `contract-type-rebuild`, `wasm-upgrade-backfill`, and the SEP-1
      enrichment chunk path run without `Code 47` / `NO_SUCH_COLUMN` against a
      current-schema ClickHouse.
- [ ] All three CH-gated e2e tests seed and pass against `apply_init_sql`.
- [ ] No remaining `soroban_contracts.name` / `assets.name` reference in
      `backfill-runner` or `backfill-enrichment-runner` (grep clean).
- [ ] **Docs updated** — N/A (no architecture-shape change; restores documented
      behavior).
- [ ] **API types regenerated** — N/A (no `crates/api/**` change).
