---
id: '0303'
title: 'OPS: NFT surfacing rollout — 0296 deploy + raw-S3 re-parse + contract-type rebuild/reclassify + validation'
type: OPS
status: backlog
related_adr: ['0046']
related_tasks: ['0283', '0294', '0296', '0301', '0231']
tags:
  [
    clickhouse,
    ops,
    nft,
    contract-classification,
    prod-rollout,
    pre-launch,
    priority-high,
  ]
links: []
history:
  - date: 2026-06-16
    status: backlog
    who: karolkow
    note: >
      Spawned from 0283 (open-problem #9, operational). 0283 delivered the
      reclassification CODE (rebuild + live G1/G2/G9 verdicts, on branch
      fix/0283); this task is the prod RUN + validation, split out so 0283 can
      archive when the code merges — mirrors the 0231 → 0301 code/rollout split.
  - date: 2026-06-17
    status: backlog
    who: karolkow
    note: >
      Extended to OWN the full 0296 close-out: the 0296 parser deploy + raw-S3
      re-parse (previously listed only as a prereq) are now explicit owned steps
      (0c/0d), so this single task surfaces the complete NFT population end-to-end —
      not just the ~40 collections that already had Shape-A rows. 0296 parser code
      merged via PR #263.
  - date: 2026-06-18
    status: backlog
    who: karolkow
    note: >
      Scope sharpened after 0294 code merged. The 0294 SAC-orphan RELABEL (run
      the new `backfill-runner sac-orphan-relabel` batch tool) is now an explicit
      Step here (was only an implicit prereq). Carries the 0294 review's
      MUST-FIX: the tool's `fetch_orphan_events` query OOMs on prod (3.73 GiB,
      reproduced) - rewrite to anchor on the ~5,607 orphan ids before running
      (even `--dry-run` runs it). All deploy / rerun / relabel-run lives here,
      not in 0294 (code-only). NB 0306 (SAC-skeleton /v1/contracts de-pollution)
      is NOT a prereq - it is orthogonal to NFT reclassification.
---

# OPS: ClickHouse contract-type reclassification rollout

## Summary

Run the 0283 contract-type rebuild + NFT reclassification on **prod ClickHouse**
and validate. 0283 built the code (rebuild tool + live verdict path); this is the
operational rollout: execute the rebuild, promote NFTs pending→hot, backfill the
fungible `assets`, validate, smoke E15/E16/E17. The reclassification surfaces the
NFT population that **0301/0231 enrichment** then enriches — so this RUN is the
gate between classification and enrichment.

This task now also **owns the 0296 parser deploy + raw-S3 re-parse** (Steps 0c/0d) —
the step that materialises the silently-dropped NFT rows into pending — so it is the
single end-to-end close-out for NFT surfacing, not just the reclassify run.

## Prerequisites

- **0283** (classification code) merged + **deployed** — the rebuild tool + live
  G1/G2/G9 verdict path must be live before the rebuild run.
- **0294** (SAC labeling) — mark the un-deployed-SAC orphans `is_sac=true` so the
  reclassify DROPS their ~51.5M false-positive pending rows instead of churning.

> The 0296 parser deploy + its raw-S3 re-parse used to sit here as a prereq; they
> are now **owned steps (0c/0d) below**, so this task fully closes the 0296 NFT
> recovery end-to-end.

## Steps (deploy → re-parse → rebuild → reclassify → validate)

1. **Step 0b — prod verification** (PARTLY DONE 2026-06-16 via `chq`): current
   **1 Nft / 2 Fungible**; would-be after rebuild **125 Nft / 4,118 Fungible**;
   **11,214 promotable NFT token rows across 40 collections** (~85/125 collections
   have 0 rows → the 0296 gap, closed by 0c/0d). Re-confirm at run time.
2. **Step 0c — deploy the 0296 parser** (indexer Lambda, PR #263) to prod. Required
   so (a) live ingest stops silently dropping map / packed-vec / consecutive_mint
   NFT events, and (b) the re-parse below runs through the fixed parser. Smoke: a
   live ledger carrying an NFT `map{token_id}` / `consecutive_mint` event now
   produces `nfts_pending` rows.
3. **Step 0d — raw-S3 re-parse** of historical ledgers through the deployed parser →
   materialise the silently-dropped NFT rows (Shape map / packed-vec /
   consecutive_mint) into `nfts_pending` + `nft_ownership_pending`. This is the 0296
   backfill — it surfaces the ~85/125 zero-row collections. **MUST run BEFORE the
   rebuild+reclassify** so the reclassify promotes the FULL population in one pass
   (else it has to re-run). Scope/runbook: raw-S3 re-ingest (see runbook 0217 drain
   and the 0233 merged-backfill runbook); coordinate with the 0281 maintenance
   window if the volume is heavy.
4. **Rebuild** — run `ch-maint contract-type-rebuild` (or backfill-runner
   equivalent) on prod CH: classify from `wasm_interface_metadata`, write verdicts
   (staging + EXCHANGE; **indexer stopped** during the swap — no code guard).

   **4b — SAC orphan relabel (task 0294 batch; run BEFORE step 5):** run
   `backfill-runner sac-orphan-relabel --dry-run` then for real. Crypto-confirms
   the ~5,607 un-deployed-SAC orphans and flips them `is_sac=true,
   contract_type=Token` so step 5 (reclassify) DROPS their ~51.5M false
   NFT-pending rows. MUST-FIX before running: the tool's `fetch_orphan_events`
   query OOMs on prod (3.73 GiB; `soroban_events` is built into the join and
   `signature` is not in its sort key) - rewrite to anchor on the ~5,607 orphan
   ids (materialize them, then `soroban_events WHERE contract_id IN (...)`,
   hitting the primary key). Even `--dry-run` runs this query.
5. **Reclassify** — `nft-reclassify`: promote Nft pending→hot, drop SAC/Fungible.
6. **Assets backfill** — insert the type-3 (Soroban-fungible) `assets` rows.
7. **TRUNCATE decision** for the orphan pending residue (coordinate 0294).
8. **Instrumentation + RTT probe + E15/E16/E17 smoke + docs** (ADR 0046, runbooks
   0217/0221, clickhouse-pilot).

## Acceptance Criteria

- [ ] 0296 parser deployed (Step 0c); live NFT map / vec / consecutive_mint events
      produce `nfts_pending` rows (no silent drop)
- [ ] Raw-S3 re-parse run (Step 0d); the ~85/125 previously-zero-row collections now
      have pending rows (the 0296 backfill)
- [ ] `sac-orphan-relabel` prod OOM-query fixed + run; ~5,607 orphans flipped `is_sac=true`
- [ ] Rebuild + reclassify run on prod; `promoted_nfts` > 0 — **all ~125 would-be-NFT
      collections** surfaced to hot (not just the ~40 with pre-existing Shape-A rows)
- [ ] Fungible `assets` backfilled; `/v1/contracts` + `/nfts` serve real data
- [ ] E15 / E16 / E17 smoke green
- [ ] Prereqs 0283-deployed + 0294 done first

## Depends on

- **0283** (code) merged + deployed
- **0296** (parser code) merged (PR #263) — its **deploy + re-parse are owned here**
  (Steps 0c/0d), making 0303 the single end-to-end close-out for NFT surfacing
- **0294** (SAC labeling — run order matters)
- Feeds **0301** / **0231** (enrichment drain)
