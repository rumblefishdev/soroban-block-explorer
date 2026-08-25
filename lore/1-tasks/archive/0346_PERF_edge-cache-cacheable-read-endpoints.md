---
id: '0346'
title: 'PERF: enable API Gateway edge cache for cacheable read endpoints (netstats, txlist, lists)'
type: PERF
status: done
related_adr: []
related_tasks: ['0338', '0055', '0097', '0277', '0455']
tags: [priority-medium, effort-small, layer-infra, milestone-3, phase-launch]
milestone: 3
links:
  - docs/architecture/backend/api-gateway-cache-spec.md
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Spawned from 0338 load-test analysis — cache the healthy-but-not-200ms endpoints (tier 3).'
  - date: '2026-08-19'
    status: done
    who: karolkow
    note: >
      Closed as superseded. The API Gateway stage cache was decided against on
      2026-08-14 under task 0455 — status and return condition live in
      docs/architecture/backend/api-gateway-cache-spec.md. The mechanism this task
      picked is also no longer the preferred one: if origin pressure ever shows up,
      the lever is a Cloudflare cache rule honouring the existing origin headers.
---

# PERF: enable API Gateway edge cache for cacheable read endpoints

## Summary

The "healthy but not <200 ms" endpoints from the 0338 load test are read-mostly
and cacheable, but the API Gateway response cache is **OFF**. Enabling an edge
cache with a sensible TTL should bring their p50 to **<50 ms** and shield CH from
repeated identical scans.

## Context

Evidence: `crates/load-tests/out/2026-07-01T13-43-39Z/results.csv` — candidates
(dur p50 / read_rows): `netstats` 271 ms / 30k, `txlist` 286 ms / 2.5M,
`ldglist` 333 ms / 60k, `ctriface` 420 ms / 29k, `ctrlist` 681 ms / 1.2M,
`nftlist` 719 ms / 904k. API GW response cache is OFF in `infra/envs/production.json`
(noted in 0338). This is tier 3 — do it AFTER the CH full-scan fixes (0344, 0345),
which matter far more.

## Implementation

- Enable the API Gateway method/stage response cache for cacheable GET endpoints
  (`api-gateway-stack.ts` / `production.json`), with per-endpoint TTLs matched to
  data freshness (tip-sensitive endpoints get short TTLs or stay uncached).
- Confirm `X-Edge-Secret` / auth interplay doesn't bust the cache key unintentionally.
- Measure cache hit ratio + p50 improvement.

## Acceptance Criteria

- [ ] Cacheable endpoints served p50 <50 ms via cache (measured) — not done, the
      cache was never enabled
- [x] Cache TTLs documented per endpoint; tip-sensitive endpoints not staled —
      done ahead of this task by 0055 / 0284 / 0292, in the browser rather than at
      a gateway cache
- [x] Docs updated — `docs/architecture/backend/api-gateway-cache-spec.md` carries
      the NOT ADOPTED status and the return condition

## Outcome

Closed without enabling the cache. The decision went the other way on 2026-08-14
under task 0455, and is recorded at the top of
[`docs/architecture/backend/api-gateway-cache-spec.md`](../../../docs/architecture/backend/api-gateway-cache-spec.md):

> **Status: NOT ADOPTED — decision 2026-08-14 (task 0455).**

Two reasons the task is dead rather than deferred:

1. **The premise moved.** "Enable before launch" (task 0097) predates Cloudflare
   fronting the API (task 0277) and was overtaken by it. A dedicated Memcached
   cluster is a standing cost for a need nothing has measured.
2. **The mechanism is no longer the preferred one.** If origin pressure ever
   appears, the spec names a Cloudflare cache rule honouring the existing origin
   headers as the lever — zero AWS cost, shared edge, and the backend contract
   already exists for it. So this task would be reopened as a different task.

The return condition is written into the spec rather than left implicit:
measured origin pressure — ClickHouse read-quota incidents on cacheable
endpoints, or API latency the in-process layer cannot absorb.

## What caches today

Not nothing, which is part of why the gateway cache stayed off:

- Per-endpoint `Cache-Control` tiers in every user's **browser** — LIVE / SHORT
  10 s / MEDIUM 60 s / LONG 300 s / NO_STORE (`crates/api/src/common/cache_control.rs`,
  tasks 0055, 0284, 0292 which added the LIVE tier and ETag/304 conditional GET).
- **In-process moka caches** for contract detail (45 s) and network stats (60 s).
- Cloudflare proxies but does **not** cache — `cf-cache-status: DYNAMIC`, no cache
  rules in `infra/cloudflare/`.

## State of the machinery, if it is ever revisited

- `apiGatewayCacheEnabled: false` — `infra/envs/production.json`
- Wiring is real and one flag away: `cacheClusterEnabled: config.apiGatewayCacheEnabled`
  in `infra/src/lib/stacks/api-gateway-stack.ts`
- The dashboard row graphing `CacheHitCount` / `CacheMissCount` was removed in 0455
  as permanently empty, with "false and stays false by decision" recorded in
  `infra/src/lib/stacks/cloudwatch-stack.ts`
