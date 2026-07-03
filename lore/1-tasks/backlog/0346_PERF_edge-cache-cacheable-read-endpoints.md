---
id: '0346'
title: 'PERF: enable API Gateway edge cache for cacheable read endpoints (netstats, txlist, lists)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0338']
tags: [priority-medium, effort-small, layer-infra, milestone-3, phase-launch]
milestone: 3
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Spawned from 0338 load-test analysis — cache the healthy-but-not-200ms endpoints (tier 3).'
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

- [ ] Cacheable endpoints served p50 <50 ms via cache (measured)
- [ ] Cache TTLs documented per endpoint; tip-sensitive endpoints not staled
- [ ] Docs updated (ADR 0032) for the cache config — or N/A noted
