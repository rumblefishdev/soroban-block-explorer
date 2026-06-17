---
id: '0299'
title: 'ClickHouse enrichment rollout + finish: deploy, prod drain, NFT read-join, full smoke, cleanup'
type: FEATURE
status: backlog
related_adr: ['0044', '0045', '0047', '0048']
related_tasks: ['0231', '0243', '0282']
tags:
  [
    priority-medium,
    effort-medium,
    clickhouse,
    enrichment,
    rollout,
    post-merge,
    milestone-2,
  ]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0231. The enrichment WRITE path (PG→CH side tables, worker
      repoint, producer anti-join un-stub, batch runner, asset read-join 4a, local
      smoke) is code-complete + committed on feat/0231 — 0231 closed as
      "implementation delivered". This task carries everything that requires a
      DEPLOY or follows it, plus the small remaining code follow-ups (NFT read-join
      4b, column-drop writer-change 8, async_insert 9, ADR-0032 docs).
---

# ClickHouse enrichment rollout + finish

## Summary

Take the lore-0231 enrichment write path (code-complete, committed, locally
verified) to **live in production** + finish the gated/post-deploy items. 0231
delivered the implementation; this is the rollout + the small follow-ups that
were blocked on deploy or on adjacent workstreams.

## Context

0231 ported SEP-1 asset + NFT `token_uri` enrichment PG→CH (side tables
`asset_enrichment` / `nft_enrichment`, ADR 0048), repointed the live worker,
un-stubbed the indexer producer anti-join, and shipped the batch runner +
asset read-join (4a) + local smoke. **Blocked on deploy** (GitHub auth + the
worker's CDK env), so the live rollout was split out here.

## Remaining work

1. **Deploy** indexer + worker — needs the worker's CDK env
   (`MTLS_SECRET_NAME`, `CH_DOMAIN`) + GitHub auth. **Footgun to resolve at
   deploy**: the producer publishes to SQS every ledger batch but the worker is
   OFF in prod (`enrichmentWorkerLambdaConcurrency: 0`) — either enable the
   worker or gate the producer, else messages age out (recoverable via the drain,
   but wasteful). See 0231 cost assessment.
2. **Step 7 — prod drain**: run `backfill-enrichment-runner` against prod CH to
   backfill the existing ~1M+ assets/NFTs; report SEP-1/NFT NULL ratios + RPC
   quota; measure the read-join cost.
3. **Step 4b — NFT read-join (CODE)**: the `nfts` API module is still PG-only
   (no `queries_ch`). Add the CH read path + `DataSource` dispatch — the
   0243-style API PG→CH migration for nfts. `nft_enrichment` is
   `ReplacingMergeTree(version)`, so the join MUST collapse to one latest row
   per key — use the same versioned pattern as the asset read-join
   (`queries_ch.rs`: an `argMax(col, version) … GROUP BY <key>` sub-aggregate,
   or `nft_enrichment FINAL`) BEFORE the LEFT JOIN, else an un-merged duplicate
   multiplies NFT rows. Then `NULLIF(ne.col,'')` neutralises the `''` sentinel.
   Prerequisite for the NFT half of the full smoke.
4. **Step 10 — full prod smoke (live path)**: verify `SQS → worker → side-table
→ read-join → API` end-to-end on prod (NFT `token_uri` RPC round-trip + write;
   SEP-1 asset round-trip; clear-on-refresh; API serves the enriched value).
   Distinct from Step 7 (drain = data; smoke = the live machinery works).
5. **Step 8 — drop dead columns (gated, LAST)**: drop `assets.{name,icon_url}`
   - `nfts.{name,media_url,collection_name}` from CH. Requires the indexer +
     `backfill-runner` writer-change FIRST (stop emitting them: `AssetRow`/`NftRow`
   - `stage.rs` + `asset_aggregates.rs`/`repair_tier1.rs`) → redeploy → then the
     heavy `ALTER DROP` in a low-traffic window (operator runbook, like 0217).
     Could be coordinated through the CH maintenance window (task 0281).
6. **Step 9 — async_insert (perf)**: set `async_insert=1` +
   `wait_for_async_insert=1` on both enrichment CH clients (worker + runner) so
   the many tiny per-key INSERTs are server-batched. Cheap; could ride 0281.
7. **Docs (ADR 0032)**: update `docs/architecture/**` if any schema/endpoint
   shape changes during the above (the column drop does).

## Acceptance Criteria

- [ ] Worker + indexer deployed; producer↔worker flag coupling resolved
- [ ] Prod drain run; coverage + NULL ratios + read-join cost reported
- [ ] NFT read-join (4b) live — `nfts` API serves `nft_enrichment` (not PG)
- [ ] Full prod smoke green: live SQS→worker→read round-trip, NFT RPC + SEP-1,
      clear-on-refresh, API serves enriched value
- [ ] Dead columns dropped (after writer-change + redeploy), `soroban_contracts.name` kept
- [ ] `async_insert` enabled on both clients
- [ ] `docs/architecture/**` updated per ADR 0032 (or explicit N/A)

## Depends on

- **0231** ✅ (write path implemented + committed)
- Deploy access (GitHub auth + CDK env) — the headline blocker
- **0243**-style nfts API CH migration for Step 4b
- **0282** (NFT media-url quality) — independent follow-up, not blocking
- **0281** (CH maintenance window) — natural carrier for Steps 8 + 9
