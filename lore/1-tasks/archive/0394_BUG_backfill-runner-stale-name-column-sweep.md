---
id: '0394'
title: 'backfill-runner + enrichment-runner: sweep remaining stale `name` column references (0304 drop)'
type: BUG
status: completed
related_adr: []
related_tasks: ['0388', '0392', '0304', '0359', '0379', '0406']
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
  - date: 2026-07-16
    status: active
    who: stkrolikiewicz
    note: >
      Activated to sweep the four remaining stale `name` sites plus two e2e
      seed fixtures.
  - date: 2026-07-17
    status: completed
    who: stkrolikiewicz
    note: >
      Closed. Sweep shipped in PR #342 (`116850d6`, merged 07-16). All four ACs met
      with evidence: grep verified clean across both crates (the surviving `name`
      references are the legitimate `nfts` / `soroban_contract_metadata` /
      `*_enrichment` columns this task excludes by design); `contract-type-rebuild`
      ran on **prod** in the 0379 Phase-3 drain 07-16 with the fixed binary
      (flipped_nft 105, flipped_fungible 3738, no Code 47).
      AC2 needed the CH-gated e2e, which **had never been run by anyone** — so they
      were run here against docker ClickHouse **26.3.12.3** (prod's major) with
      `init.sql` applied: **all 5 pass**. Execution was verified rather than
      assumed, because these tests pass silently when they skip: `system.query_log`
      carries their real statements (INSERT QueryStart+QueryFinish, the throwaway
      CREATE/DROP DATABASE), and a control run with the gate unset moved the log by
      zero. An early "1 passed in 0.53s" was NOT accepted as proof for exactly that
      reason.
      Two findings worth more than this task. **(1)** The suite uses two different
      gates — `CLICKHOUSE_URL` for backfill-runner, `#[ignore]` + `CLICKHOUSE_URL`
      for the enrichment three — so one command silently covers only part of it.
      **(2)** **CI has zero references to clickhouse in any workflow**, so all 25
      files' worth of CH-gated tests always skip, and the green "Rust (clippy,
      test)" on PR #342 said nothing about this task's ACs. That is the root cause
      of this whole 0304→0388→0392→0394 family: the tests that would have caught the
      stale column on the first PR already existed and never ran. Spawned **0406**.
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

- [x] `contract-type-rebuild`, `wasm-upgrade-backfill`, and the SEP-1
      enrichment chunk path run without `Code 47` / `NO_SUCH_COLUMN` against a
      current-schema ClickHouse. — **met, two independent ways.** All three paths
      ran green against a fresh `init.sql` schema on ClickHouse 26.3.12.3
      (2026-07-17, see below), and `contract-type-rebuild` additionally ran **on
      prod** in the 0379 Phase-3 drain 2026-07-16 with the fixed binary:
      flipped_nft 105, flipped_fungible 3738, assets_inserted 0 — no `Code 47`.
- [x] All three CH-gated e2e tests seed and pass against `apply_init_sql`.
      — **met 2026-07-17.** Run locally against docker ClickHouse **26.3.12.3**
      (same major as prod) with `init.sql` applied (31 tables). Results:

      | test | gate | result |
      | ---- | ---- | ------ |
      | `contract_type_rebuild::tests::rebuild_e2e_flips_other_and_backfills_assets` | `CLICKHOUSE_URL` | **ok** |
      | `wasm_upgrade_backfill::tests::backfill_e2e_corrects_stale_hash` | `CLICKHOUSE_URL` | **ok** |
      | `tests::select_sep1_chunk_skips_enriched_and_native` | `#[ignore]` + `CLICKHOUSE_URL` | **ok** |
      | `tests::select_sep1_chunk_sentinels_excludes_real_and_partial` | `#[ignore]` + `CLICKHOUSE_URL` | **ok** |
      | `tests::select_nft_chunk_skips_enriched` | `#[ignore]` + `CLICKHOUSE_URL` | **ok** |

      Execution was **verified, not assumed** — these tests pass silently when they
      skip, so `system.query_log` was used as the witness: it carries their actual
      statements (`INSERT INTO nfts … VALUES`, QueryStart + QueryFinish; 4 nft seeds,
      10 asset seeds) and the backfill-runner pair's throwaway `CREATE DATABASE` /
      `DROP DATABASE`. A control run with `CLICKHOUSE_URL` unset moved the log by 0.

- [x] No remaining `soroban_contracts.name` / `assets.name` reference in
      `backfill-runner` or `backfill-enrichment-runner` (grep clean).
      — **met, verified 2026-07-17**: zero hits for `sc.name` or a `name` column in
      an `assets(…)` / `soroban_contracts(…)` list across both crates. The surviving
      `name` references are the legitimate ones this task excludes by design
      (`nfts.name`, `soroban_contract_metadata.name`, `asset_enrichment.name`,
      `nft_enrichment.name`).
- [x] **Docs updated** — N/A (no architecture-shape change; restores documented
      behavior).
- [x] **API types regenerated** — N/A (no `crates/api/**` change).

## Notes on the e2e gating (found while closing)

The five tests use **two different gates**, which is why a single command misses
some of them:

- `backfill-runner`'s two read `CLICKHOUSE_URL` and **skip cleanly when unset** —
  they create and drop a throwaway database, so they are safe against any server.
- `backfill-enrichment-runner`'s three are additionally `#[ignore]`d and need
  `--ignored`. They seed the **real** `assets` / `nfts` / `*_enrichment` tables in
  the configured database and clean up with `ALTER TABLE … DELETE` — so point them
  at a throwaway server, never a shared one.

**CI never runs any of them**: no workflow provisions a ClickHouse service, so the
gate is always unset and all five are skipped. PR #342's green "Rust (clippy, test)"
therefore said nothing about this task's ACs. That is why they were run by hand here.
