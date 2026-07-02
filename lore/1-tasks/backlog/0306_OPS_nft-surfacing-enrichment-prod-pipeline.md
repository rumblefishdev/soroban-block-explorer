---
id: '0306'
title: 'OPS: NFT surfacing + enrichment prod pipeline — one ordered run (reparse → rebuild → reclassify → NFT enrich → asset enrich)'
type: OPS
status: backlog
related_adr: ['0046', '0050']
related_tasks: ['0283', '0231', '0296', '0294', '0303', '0301']
tags:
  [
    clickhouse,
    ops,
    nft,
    enrichment,
    contract-classification,
    prod-rollout,
    pre-launch,
    priority-high,
  ]
links:
  - docs/runbooks/0217_nfts_pending_migration_and_drain.md
  - docs/runbooks/0231_enrichment-backfill-local.md
history:
  - date: 2026-06-19
    status: backlog
    who: karolkow
    note: >
      Created to consolidate the operational rollout split across 0303
      (reclassification run) and 0301 (enrichment run), now that the
      implementation from 0283 / 0296 / 0231 has landed. Hardened by a
      red/blue/devil's-advocate review (code-grounded). Operator cleared the
      prereqs (prod schema present; 0283 + 0231 deployed; CLI run on-box →
      mTLS moot) and deferred 0294 SAC labeling: skipping it loses no real NFT
      (orphan false-positives stay quarantined in pending, droppable later).
      Runnable on-box.
---

# OPS: NFT surfacing + enrichment prod pipeline (one ordered run)

## Summary

Run, **in strict order**, the five ClickHouse maintenance + enrichment scripts
that take prod from "hot `nfts` empty, enrichment empty" to "NFT + asset
population surfaced and enriched". Enrichment is the finish line and depends on
every earlier step — it reads the hot `nfts` / `assets` tables that only fill
once reparse → rebuild → reclassify have run.

Consolidates the run-steps of 0303 (reclassify) and 0301 (enrichment) into one
self-contained runbook.

## Status: ready to run on-box — no hard blockers

Destructive, multi-step prod-mutation sequence. As of 2026-06-19 it is unblocked:

- **Run on the Hetzner CH box** (plain HTTP to `localhost:8123`) — the CLI
  runners build a plain client with no cert, so on-box bypasses the Caddy mTLS
  the read-only `chq` wrapper uses. mTLS is moot on-box.
- **Prod schema + side tables present**, **0283 + 0231 deployed** — confirmed by
  operator.
- **0294 SAC labeling — deferred, not a blocker.** ~4,310 orphans (un-deployed
  SACs mislabeled `is_sac=false`) hold ~51.5M false-positive `nfts_pending` rows
  (a SAC `transfer` amount mis-read as an NFT `token_id`). They sit at
  `contract_type = NULL`, so `nft-reclassify` neither promotes nor drops them —
  they stay quarantined in pending, which the API never reads. Real-NFT surfacing
  runs on `contract_type = 2` (a disjoint set), so skipping 0294 **loses no real
  NFT**. Drop the false-positives later via 0294 or a one-off `ALTER DELETE`.

The remaining items are run-day gates, not blockers.

### Run-day gates

- **G1 — indexer STOPPED for step 2's `EXCHANGE TABLES`** (whole-table swap; a
  concurrent write is lost or trips the `RebuildGuard` fence and aborts). Resume
  after the swap.
- **G2 — no in-flight mutations between steps.**
  `SELECT * FROM system.mutations WHERE is_done = 0` must be empty before step 3
  and before any re-run.
- **G3 — enrichment worker paused / keys partitioned during the drains.** Worker
  and drain both write `version = now_ms()`; concurrent writes over the same keys
  can regress a real value under an older sentinel. (Prod has the worker off
  while the producer publishes — enable the worker or gate the producer.)

## The pipeline (run top-to-bottom)

| #   | Script                    | Command                                                                                      | Source | What it does                                                                                                                                                                                                                                                                                                                                       |
| --- | ------------------------- | -------------------------------------------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **NFT Reparse**           | `backfill-runner --target clickhouse nft-reparse --start <genesis> --end <head> [--dry-run]` | 0296   | Re-parses `soroban_events` (already decoded in CH) through the 0296 parser; recovers the silently-dropped shapes (map / packed-vec / consecutive_mint) into `nfts_pending` + `nft_ownership_pending`. CH-direct, no raw-S3. Writes PENDING only. Idempotent.                                                                                       |
| 2   | **Contract Type Rebuild** | `backfill-runner --target clickhouse contract-type-rebuild [--dry-run]`                      | 0283   | Classifies every WASM from `wasm_interface_metadata`, rebuilds `soroban_contracts.contract_type` into staging and `EXCHANGE TABLES`-swaps it, then backfills the missing Soroban-fungible (`asset_type=3`) `assets` rows (`NOT EXISTS`-guarded). Idempotent. Indexer STOPPED during the swap (G1).                                                 |
| 3   | **NFT Reclassify**        | `backfill-runner --target clickhouse nft-reclassify [--dry-run]`                             | 0283   | Promotes `nfts_pending` → hot `nfts` for `contract_type = 2`; drops pending for `contract_type IN (0,3)`; drops legacy hot false-positives. `ALTER … DELETE` + `OPTIMIZE FINAL`. Reads the verdicts step 2 wrote.                                                                                                                                  |
| 4   | **NFT Enrichment**        | `backfill-enrichment-runner nft-metadata [--concurrency N] [--chunk-size N]`                 | 0231   | Drains hot `nfts` with no `nft_enrichment` row; fetches `token_uri()` via mainnet Soroban-RPC → `name` / `media_url`. **`collection_name` comes from the contract-level SEP-50 `name()` RPC, not `token_uri()` JSON** (task 0340) — new rows get it inline; pre-0340 rows need step 4a. Output stays dark to the API until 0301 step 4b — see ACs. |
| 4a  | **NFT Collection Name**   | `backfill-enrichment-runner nft-collection-name [--concurrency N]`                           | 0340   | Backfills `nft_enrichment.collection_name` for rows enriched before 0340 (real `name`/`media_url`, empty collection): one `name()` RPC per DISTINCT contract, re-INSERT preserving `name`/`media_url`. Skip if the cohort was enriched after 0340 landed.                                                                                          |
| 5   | **Asset Enrichment**      | `backfill-enrichment-runner sep1-assets [--concurrency N] [--chunk-size N]`                  | 0231   | Drains classic/SAC `assets` (`asset_type IN (1,2)`) with no `asset_enrichment` row; fetches issuer SEP-1 TOML → `icon_url` / `name`.                                                                                                                                                                                                               |

- `backfill-enrichment-runner` is CH-only (no `--target`); reads `CLICKHOUSE_URL`
  / `CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` / `CLICKHOUSE_DATABASE`. Both drains
  accept `--limit`, `--force-retry`, `--retry-sentinels` (re-attempt only `''`
  sentinel rows; exclusive with `--force-retry`), plus a `status` subcommand.
- Enrichment exits non-zero only on `db_failed > 0`; a high transient/sentinel
  count is a clean exit (the RPC-liveness ceiling, not a failure).

### Why this order

- `nft-reparse` fills the full pending population first — without it, the ~85 of
  125 NFT collections with 0 pending rows stay empty and reclassify surfaces only
  the ~40 with pre-existing rows.
- `contract-type-rebuild` writes the `Nft`/`Fungible` verdicts nothing else sets.
- `nft-reclassify` reads those verdicts, so it runs after rebuild.
- Enrichment (4, 5) reads the hot tables filled above; 4 and 5 are independent.

## Logging & evidence capture

The binaries do not persist a run log — wire it explicitly:

- **Always pass `--verbose`** (both runners gate tracing on it; `RUST_LOG` is
  ignored). Without it, per-batch progress and per-key `reason=` failures are
  suppressed.
- **Capture both streams** (detail → stderr, summary → stdout):
  ```bash
  <command> --verbose 2>&1 | tee docs/runbooks/artifacts/0306_<step>_<YYYYMMDD>.log
  ```
- **Record before/after `chq` reads** + the printed summary into the Run log
  below. Keep the artifact logs.

## Run procedure (per step: dry-run → status-before → real → status-after → cross-check → record)

> Every `backfill-runner` step has a count-only `--dry-run` — run it first,
> compare to the expected numbers, then run for real. All steps are idempotent,
> so a crashed step is safe to re-run by the same range/args. `chq` is read-only
> — mind the quotas (2B rows + 100 GB/h per server-hour).

**Step 0 — baseline census (`chq`).** Record: verdict breakdown
(`soroban_contracts FINAL`), would-be-Nft/Fungible, promotable token rows +
collections, orphan count + pending rows. Reference (2026-06-16 prod, partial):
current 1 Nft / 2 Fungible; after rebuild ~125 Nft / ~4,118 Fungible; ~11,214
promotable token rows across 40 collections (85/125 still 0-rows → the 0296 gap
step 1 closes); orphans 4,310 / ~51.5M pending; ~66% of recent NFT emitters
absent from live state (the enrichment ceiling).

**Step 1 — NFT Reparse.**

- Range MUST be genesis → head (a narrow range misses the historical tail).
  Chunk by range if needed (idempotent per range).
- The scan has no shape pre-filter — it sweeps the whole mint/transfer/burn
  firehose, so budget read-quota windows; the "~9 s local" figure does not apply.
- dry-run reports `events_scanned` + would-recover rows.
- Cross-check: distinct collections gaining pending rows ≈ 85; confirm a sample
  by re-parsing (some of the 85 may be genuinely inactive).

**Step 2 — Contract Type Rebuild.**

- Stop the indexer (G1); confirm `system.mutations` quiet (G2).
- dry-run: `flipped_nft ≈ 125`, `flipped_fungible ≈ 4,118`, `assets_inserted ≈`
  the type-3 gap. Real run → resume the indexer. Re-runnable if it fails.
- Cross-check: post-run verdict breakdown ≈ 125 Nft / 4,118 Fungible.

**Step 3 — NFT Reclassify.**

- Confirm `system.mutations` quiet (G2).
- dry-run: `promoted_nfts > 0` across ~125 collections.
- Cross-check (0294 deferred): `dropped_pending_*` reflects only real
  Fungible/Token drops, NOT the ~51.5M orphan rows — those stay in pending by
  design (quarantined, droppable later). A near-zero orphan drop is correct here.
- Real run → record promoted/dropped counts + before/after hot `nfts` totals.

**Step 4 — NFT Enrichment.**

- `status` (before) → `nft-metadata` drain → `status` (after).
- Split the NULL ratio into RPC-absent (archived/evicted instance) vs not-tried;
  a high NFT NULL ratio is the ~66% liveness ceiling, not an incomplete job.
- Output stays dark to the API until 0301 step 4b (CH read-join) lands and
  `NFTS=ch` flips — `nfts` API is still PG-only.

**Step 5 — Asset Enrichment.**

- Worker paused (G3). `status` before → `sep1-assets` drain → `status` after.
- Report SEP-1 NULL ratio; watch the sentinel ratio (recover with
  `--retry-sentinels`).

**Step 6 — validate, flip, docs.**

- Smoke E15/E16/E17 (`/nfts*`); `/v1/contracts` + `/v1/assets` serve real data.
- HARD GATE — flip `ASSETS=ch` only after a staging assertion that classic/SAC
  assets return non-NULL name+icon (the CH read has no fallback). Same for
  `NFTS=ch` once 4b lands.
- Do NOT TRUNCATE pending in this run (destructive; gated on the deploy-linkage
  fix — see 0283 step 6 / ADR 0046).
- Docs per ADR 0032: ADR 0046, runbooks 0217/0221, clickhouse-pilot.

## Run log (fill during execution)

| Step           | Date | dry-run summary                | before (chq)                    | after (chq)       | cross-check              | artifact                 |
| -------------- | ---- | ------------------------------ | ------------------------------- | ----------------- | ------------------------ | ------------------------ |
| 0 baseline     |      | —                              | verdicts / orphans / promotable | —                 | —                        |                          |
| 1 reparse      |      | events_scanned / would-recover | pending totals                  | pending totals    | ≈85 collections nonzero? | `0306_reparse_*.log`     |
| 2 rebuild      |      | flipped_nft/fungible/assets    | verdict breakdown               | verdict breakdown | ~125 / ~4,118?           | `0306_rebuild_*.log`     |
| 3 reclassify   |      | promoted/dropped               | hot nfts totals                 | hot nfts totals   | promoted > 0?            | `0306_reclassify_*.log`  |
| 4 nft-enrich   |      | status coverage                | status                          | status            | NULL split               | `0306_nftenrich_*.log`   |
| 5 asset-enrich |      | status coverage                | status                          | status            | SEP-1 NULL ratio         | `0306_assetenrich_*.log` |

## Acceptance Criteria

- [x] **Prereqs cleared:** run on-box (mTLS moot); schema + side tables present;
      0283 + 0231 deployed. 0294 deferred — not a run blocker.
- [ ] **Step 1 — NFT Reparse** run genesis→head; ~85 previously-zero-row
      collections gain pending rows (sample-confirmed).
- [ ] **Step 2 — Contract Type Rebuild** run (indexer stopped); ~125 Nft /
      ~4,118 Fungible verdicts written; type-3 `assets` backfilled.
- [ ] **Step 3 — NFT Reclassify** run; `promoted_nfts > 0` across ~125
      collections. (Orphan ~51.5M rows stay in pending — expected, 0294 deferred.)
- [ ] **Step 4 — NFT Enrichment** drain run; NULL ratio split + RPC quota reported.
- [ ] **Step 5 — Asset Enrichment** drain run; SEP-1 NULL ratio reported.
- [ ] **Every step logged** with `--verbose 2>&1 | tee`; before/after `chq`
      counts + summaries recorded in the Run log.
- [ ] `/v1/contracts` + `/v1/assets` serve real enriched data; `ASSETS=ch` flip
      only after the staging non-NULL assertion passes.
- [ ] **NFT surfacing caveat:** hot `nfts` + `nft_enrichment` populated; `/nfts`
      serving CH data is tracked in 0301 step 4b (not met by this run alone).
- [ ] **Docs updated** per ADR 0032 (ADR 0046 + runbooks 0217/0221 +
      clickhouse-pilot).
- [ ] **API types regenerated** — N/A (operational run; no `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**` change).

## Open questions

1. **`nft-reparse` chunking** — single genesis→head pass vs N range chunks under
   the read quota (idempotent either way; affects runtime + quota windows).
2. **Pull 0301 step 4b into this run?** If the NFT half must visibly surface, 4b
   (CH read-join) has to land — otherwise step 4's output sits dark.
3. **0294 cleanup timing** (deferred, not gating) — re-parse pass vs one-off
   audited SQL `UPDATE` to drop the quarantined orphan rows; schedule after the run.

## Relationship to 0303 / 0301

- **0303** (reclassification rollout): run-steps consolidated here → recommend
  marking superseded by 0306.
- **0301** (enrichment rollout): its deploy + drain = scripts 4–5 here. Its
  pure-code follow-ups stay in 0301 — notably **step 4b** (NFT CH read-join,
  which gates the `/nfts` surfacing above), plus the dead-column drop and
  `async_insert`.

## Notes

- All five scripts are idempotent (RMT / `NOT EXISTS` / EXCHANGE-after-drop /
  `ALTER DELETE`+`OPTIMIZE FINAL`); a crashed step is safe to re-run.
- No code-level ordering guard between scripts — "strict order" is the only
  guard. If `promoted_nfts == 0` at step 3, step 2 was skipped.
- Rehearse locally first: `docs/runbooks/0231_enrichment-backfill-local.md`
  (enrichment) + the full-scale `ch-snap` snapshot (reparse / rebuild /
  reclassify).
