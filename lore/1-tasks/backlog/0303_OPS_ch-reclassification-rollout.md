---
id: '0303'
title: 'OPS: ClickHouse contract-type reclassification rollout — prod rebuild run + validation'
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

## Prerequisites (run BEFORE the reclassify, to avoid re-runs)

- **0296** (NFT event-shape fix) + a raw-S3 re-parse — surfaces dropped NFT token
  rows (prod: **85 of 125** NFT-classified collections currently have ZERO rows).
  Reclassify AFTER, else it must re-run when 0296 lands.
- **0294** (SAC labeling) — mark the un-deployed-SAC orphans `is_sac=true` so the
  reclassify DROPS their ~51.5M false-positive pending rows instead of churning.

## Steps (from 0283 #9 + Implementation Plan 1–6)

1. **Step 0b — prod verification** (PARTLY DONE 2026-06-16 via `chq`): current
   **1 Nft / 2 Fungible**; would-be after rebuild **125 Nft / 4,118 Fungible**;
   **11,214 promotable NFT token rows across 40 collections** (~85/125 collections
   have 0 rows → the 0296 gap). Re-confirm at run time.
2. **Rebuild** — run `ch-maint contract-type-rebuild` (or backfill-runner
   equivalent) on prod CH: classify from `wasm_interface_metadata`, write verdicts
   (staging + EXCHANGE; **indexer stopped** during the swap — no code guard).

   **2b — SAC orphan relabel (task 0294 batch; run BEFORE step 3):** run
   `backfill-runner sac-orphan-relabel --dry-run` then for real. Crypto-confirms
   the ~5,607 un-deployed-SAC orphans and flips them `is_sac=true,
   contract_type=Token` so step 3 DROPS their ~51.5M false NFT-pending rows.
   MUST-FIX before running: the tool's `fetch_orphan_events` query OOMs on prod
   (3.73 GiB; `soroban_events` is built into the join and `signature` is not in
   its sort key) - rewrite to anchor on the ~5,607 orphan ids (materialize them,
   then `soroban_events WHERE contract_id IN (...)`, hitting the primary key).
   Even `--dry-run` runs this query.
3. **Reclassify** — `nft-reclassify`: promote Nft pending→hot, drop SAC/Fungible.
4. **Assets backfill** — insert the type-3 (Soroban-fungible) `assets` rows.
5. **Step 6 — TRUNCATE decision** for the orphan pending residue (coordinate 0294).
6. **Instrumentation + RTT probe + E15/E16/E17 smoke + docs** (ADR 0046, runbooks
   0217/0221, clickhouse-pilot).

## Acceptance Criteria

- [ ] `sac-orphan-relabel` prod OOM-query fixed + run; ~5,607 orphans flipped `is_sac=true`
- [ ] Rebuild + reclassify run on prod; `promoted_nfts` > 0 (~11k token rows)
- [ ] Fungible `assets` backfilled; `/v1/contracts` + `/nfts` serve real data
- [ ] E15 / E16 / E17 smoke green
- [ ] Prereqs 0294 / 0296 done first (or a re-run explicitly accepted)

## Depends on

- **0283** (code) merged + deployed
- **0294**, **0296** (prereqs — run order matters)
- Feeds **0301** / **0231** (enrichment drain)
