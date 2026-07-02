---
id: '0301'
title: 'ClickHouse enrichment rollout + finish: deploy, prod drain, NFT read-join, full smoke, cleanup'
type: FEATURE
status: completed
related_adr: ['0044', '0045', '0047', '0050']
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
  - date: '2026-06-17'
    status: backlog
    who: karolkow
    note: >
      Renumbered 0299 → 0301. The develop merge revealed 0299 was already
      taken there by 0299_REFACTOR_routes-consolidation-single-source (different
      task, allocated first on develop). Moved this task to the next free id
      (0300 was the prior max). Rewrote all refs (0231 README, ADR 0050, the
      enrichment code comments).
  - date: '2026-07-01'
    status: completed
    who: stkrolikiewicz
    note: >
      Archived as completed. The rollout landed: CH deploy and read-path flip
      across all 9 API modules (0243, 0284), NFT read-join, prod drain, and
      ADR-0032 docs sync all shipped. Residual code follow-ups (dead-column drop,
      async_insert) are carried by successors 0310/0322. Nothing remains uniquely
      owned by 0301.
---

# ClickHouse enrichment rollout + finish

## Summary

Take the lore-0231 enrichment write path (code-complete, committed, locally
verified) to **live in production** + finish the gated/post-deploy items. 0231
delivered the implementation; this is the rollout + the small follow-ups that
were blocked on deploy or on adjacent workstreams.

## Context

0231 ported SEP-1 asset + NFT `token_uri` enrichment PG→CH (side tables
`asset_enrichment` / `nft_enrichment`, ADR 0050), repointed the live worker,
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

### Constraint — live-RPC liveness ceiling (from 0283 research, 2026-06-16)

NFT `token_uri`/name/image are fetched from **live** Soroban RPC (the parser
stores no metadata at mint), so a contract whose ContractInstance is
archived/evicted (state-archival TTL — restorable but not live) returns nothing
and is **un-enrichable even after perfect classification**. (Correction, task
0340: `collection_name` is NOT part of the `token_uri()` JSON — it comes from a
separate contract-level SEP-50 `name()` RPC; still a live-RPC round-trip, so the
same liveness ceiling applies.) A network-wide
`getEvents` sample found **~66% of recent transfer/mint/burn emitters ABSENT**
from live state. Implications for this rollout:

- **Step 7 NULL-ratio MUST split** "un-enrichable-because-evicted (RPC absent)"
  from "not-yet-tried" — else a high NFT NULL ratio is ambiguous (job incomplete
  vs hitting the ceiling).
- Reachable target = "all real NFTs still **live** on mainnet", not literally all.
- The `''` sentinel on permanent-fail never auto-retries → a transient RPC outage
  at enrich time = permanent empty until `--retry-sentinels`; consider a
  restore-or-retry path.

(Cross-ref **0283**: classification is the upstream gate; this is the downstream ceiling.)

## Review-flagged gates (PR #261 review round, 2026-06-17)

- **HARD GATE — `ASSETS=ch` read flip is blocked on a completed + verified prod
  drain (Step 7), not merely on "0231 landed".** The read path
  (`api/src/assets/queries_ch.rs::ASSET_CH_SELECT`) was changed to read
  classic/SAC name+icon **only** from `asset_enrichment` — there is NO fallback
  to the indexer-owned `assets.name`/`assets.icon_url` (Option C). So flipping
  `API_DATASOURCE_ASSETS=ch` against an **empty** `asset_enrichment` shows blank
  names/icons for every classic/SAC asset site-wide. Sequence: drain → verify
  non-NULL coverage (a staging assertion that classic/SAC assets return non-NULL
  name+icon) → only then flip. Same gate applies to `NFTS=ch` once 4b lands.
- **Multi-writer `version` skew.** `version = now_ms()` is per-machine monotonic
  only. Do NOT run the drain / `--force-retry` concurrently with the live worker
  over overlapping keys — a lagging operator clock can regress a real value under
  an older sentinel (see `enrich_and_persist::now_ms` caveat). Either run the
  drain with the worker paused, or partition keys. Consider the
  `max(existing_version+1, now_ms)` read-back hardening if concurrent drains
  become routine.
- **`is_transient` coverage alarm.** The allow-list was widened (gateway 5xx /
  timeout / `-32000`), but permanent-by-default still means a provider outage can
  silently convert a chunk of the population to `''` sentinels. Add a CloudWatch
  alarm on the sentinel ratio (or a periodic `status` delta) so a
  transient-storm-turned-permanent is visible without manual polling; recover
  with `--retry-sentinels`.

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
