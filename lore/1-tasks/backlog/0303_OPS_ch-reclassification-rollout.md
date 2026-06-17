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
3. **Reclassify** — `nft-reclassify`: promote Nft pending→hot, drop SAC/Fungible.
4. **Assets backfill** — insert the type-3 (Soroban-fungible) `assets` rows.
5. **Step 6 — TRUNCATE decision** for the orphan pending residue (coordinate 0294).
6. **Instrumentation + RTT probe + E15/E16/E17 smoke + docs** (ADR 0046, runbooks
   0217/0221, clickhouse-pilot).

## Acceptance Criteria

- [ ] Rebuild + reclassify run on prod; `promoted_nfts` > 0 (~11k token rows)
- [ ] Fungible `assets` backfilled; `/v1/contracts` + `/nfts` serve real data
- [ ] E15 / E16 / E17 smoke green
- [ ] Prereqs 0294 / 0296 done first (or a re-run explicitly accepted)

## Depends on

- **0283** (code) merged + deployed
- **0294**, **0296** (prereqs — run order matters)
- Feeds **0301** / **0231** (enrichment drain)
