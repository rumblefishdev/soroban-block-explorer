---
id: '0291'
title: 'Version-keyed cache (latest_ledger_sequence) instead of TTL — zero staleness after a new ledger'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0290']
tags:
  ['clickhouse', 'api', 'cache', 'freshness', 'phase-future', 'priority-medium']
links: []
history:
  - date: 2026-06-15
    status: backlog
    who: fmazur
    note: 'Spawned from the polling/fetching investigation (FETCHING_PLAN.md). The TTL cache serves data up to ~4s behind the DB after a new ledger; version-keying eliminates that window.'
---

# Version-keyed cache instead of TTL — zero staleness after a new ledger

## Summary

`/v1/network/stats` (and potentially the live lists) caches its result on a
**pure 4s TTL** (`crates/api/src/network/cache.rs`). Because cache refresh and
ledger arrival are not phase-locked, after a new ledger lands we serve a state
that is **up to ~TTL (4s) behind the DB**. Replace the TTL with
**version-keying on `latest_ledger_sequence`** + a cheap head check, so a new
ledger becomes visible on the **first** request after it is written, instead of
only after the window expires.

## Context

From the analysis in FETCHING_PLAN.md (discussion around 0290). Scenario:

```
t=0      cache written (ledger N-1), valid until t=4
t=4.25   miss → recompute (DB still N-1), valid until t=8.25
t=5      ledger N lands
t=5..8.25 HIT → returns N-1   ← N invisible for ~3.25s
```

- This is **bounded staleness (≤ TTL), not corruption** — the response is a
  consistent older snapshot. Documented / intentional (comment in
  `network/cache.rs`: the cache's job is to collapse fan-out, not to extend data
  lifetime).
- **No skipped ledgers** (TTL 4s < cadence ~5.8s → ≤1 ledger per window → the KPI
  steps +1). That property stays.
- Only a defect against a **"sub-second freshness" contract** — today's contract
  is `useLiveStatus`'s 20s threshold, so the badge does not lie. This task raises
  freshness below the TTL without breaking stampede protection.

Why not "reset the cache on a new ledger" via push: the cache is **per-Lambda,
in-process**; Lambda instances are not addressable — there is no way to
invalidate them from outside without a shared cache (Redis, deliberately deferred
— task 0180). Version-keying is self-correcting per instance, no new infra.

## Implementation

### Step 1: Cheap head read

- `SELECT max(sequence) FROM ledgers` — a single value over the primary key (not
  35M rows). Optionally a tiny short-TTL head cache (e.g. 1s) to dedup the head
  check across requests (tradeoff: ~1s head staleness vs a query per request).

### Step 2: Cache key = sequence

- Store the result (stats / list page) under the key `latest_ledger_sequence`.
- Per request: `if cached.seq == head → return cached; else → recompute under head`.
- **Keep stampede protection** (`moka try_get_with` / single-flight): concurrent
  misses on the same `seq` → one recompute, the rest wait.

### Step 3: Scope

- `/v1/network/stats` first (1:1 with the existing cache).
- Consider the same pattern on the **live lists** (`/transactions?limit=10`,
  `/ledgers?limit=10`) — relates to CH load from 0290 (but there the dominant
  cost is the query itself — see 0290, independent).

### Step 4: Head-freshness decision

- Live `max(sequence)` (zero head lag, a query per request) **vs** a 1s head
  cache (fewer queries, ≤1s head lag). Document the choice.

## Acceptance Criteria

- [ ] A new ledger is visible on the **first** request after it is written (no
      ≤4s window with the previous state) — verified.
- [ ] **Stampede protection preserved** — concurrent misses → one recompute.
- [ ] No skipped ledgers; the "current ledger" KPI steps +1.
- [ ] Tests: hit when `seq` unchanged, recompute when `seq` increases, dedup of
      concurrent misses, head-check cost (not the heavy query).
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md` §8.1
      (cache rationale changes from TTL to version-keying). Per
      [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — likely `N/A` (internal cache change, no DTO /
      route / openapi change). Confirm at PR time; regen if any `crates/api/**`
      contract-layer change. CI gate: `API types freshness`.

## Notes

- Version-keying is mostly a **freshness** win, not a load fix for 0290: the TTL
  already bounds heavy queries to ≤1/4s; version-keying lowers that to ≤1/ledger
  (~1/5.8s) plus a cheap head check. The load-bearing CH fix stays 0290
  (Statement A).
- Relation to 304: `ETag = latest_ledger_sequence` is the same version idea on
  the HTTP side; both can share the head source (see 0292).
