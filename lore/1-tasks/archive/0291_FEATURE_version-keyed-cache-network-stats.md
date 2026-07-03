---
id: '0291'
title: 'Version-keyed cache (latest_ledger_sequence) instead of TTL — zero staleness after a new ledger'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0290', '0292', '0312']
tags:
  ['clickhouse', 'api', 'cache', 'freshness', 'phase-future', 'priority-medium']
links: []
history:
  - date: 2026-06-15
    status: backlog
    who: fmazur
    note: 'Spawned from the polling/fetching investigation (FETCHING_PLAN.md). The TTL cache serves data up to ~4s behind the DB after a new ledger; version-keying eliminates that window.'
  - date: 2026-06-19
    status: active
    who: fmazur
    note: 'Promoted to active to begin implementation.'
  - date: 2026-06-22
    status: completed
    who: fmazur
    note: >
      Shipped. cache.rs version-keyed on chain head (`()`→`i64`); shared
      `common/head.rs` head probe (PG `max(sequence)`, CH `ORDER BY sequence
      DESC LIMIT 1`); handler pins `fetch_stats(.., head)` to `WHERE sequence =
      head` so body==key; last-good fallback on head-read error. 8 files +
      api-types regen. Tests: full `cargo test -p api` 323 pass (+3 cache,
      incl. multi-thread single-flight). Verified E2E on a local CH (64k-ledger
      backfill): new ledger visible on the first request after write (AC1/AC3),
      EXPLAIN shows partition-prune + PK binary search (one granule). `max code
      review` (multi-agent) + 13-file leak scan clean. Deployed to prod
      (Compute stack) and verified. Scope kept to /v1/network/stats; live lists
      deferred. Spawned 0312 (CloudflareBootstrap orphan-secret cleanup).
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

- [x] A new ledger is visible on the **first** request after it is written (no
      ≤4s window) — **verified E2E**: against a local CH, inserting ledger
      N+1 flips `/v1/network/stats` to N+1 on the very next request (12 ms).
- [x] **Stampede protection preserved** — `try_get_with(head)`; covered by
      `concurrent_misses_on_same_head_collapse_to_one_compute` (multi-thread +
      barrier, load-bearing).
- [x] No skipped ledgers; the "current ledger" KPI steps +1 — head pin makes
      `latest_ledger_sequence == head` (verified E2E, monotonic step).
- [x] Tests: hit when `seq` unchanged, recompute when `seq` increases, dedup of
      concurrent misses — all committed. **Head-check cost** verified
      empirically (EXPLAIN `indexes=1` → partition-prune + PK binary search,
      `read_rows` 8.6k vs heavy 17k); an automated gated assertion was **not**
      added (see Future Work / [[0312]]-style note below — deferred).
- [x] **Docs updated** — `docs/architecture/backend/backend-overview.md` §8.1
      **and both** `01_get_network_stats.sql` reference queries (PG `$head` /
      CH `{head}` pin). Per
      [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [x] **API types regenerated** — turned out **NOT** `N/A`: the handler doc
      comment is the OpenAPI operation `description`, so the regen produced a
      description-only diff in `openapi.json` + `generated/*`, staged in the
      same commit. CI gate `API types freshness`: PASS.

## Notes

- Version-keying is mostly a **freshness** win, not a load fix for 0290: the TTL
  already bounds heavy queries to ≤1/4s; version-keying lowers that to ≤1/ledger
  (~1/5.8s) plus a cheap head check. The load-bearing CH fix stays 0290
  (Statement A).
- Relation to 304: `ETag = latest_ledger_sequence` is the same version idea on
  the HTTP side; both can share the head source (see 0292).

## Implementation Notes

Files (8 + api-types regen):

- `crates/api/src/common/head.rs` **(new)** — shared cheap head probe.
  `latest_sequence_pg` = `SELECT max(sequence)` (B-tree max over the PK);
  `latest_sequence_ch` = `SELECT sequence FROM ledgers ORDER BY sequence DESC
LIMIT 1` (read-in-order, one granule). Reused later by 0292 (ETag).
- `crates/api/src/common/mod.rs` — `pub mod head;`.
- `crates/api/src/network/cache.rs` — key `()` → `i64` (chain head); 60s
  backstop TTL + small `max_capacity` (bound only, not the freshness
  mechanism); rewrote module docs; new tests.
- `crates/api/src/network/handlers.rs` — cheap head read gates the cache;
  `try_get_with(head, …)`; last-good fallback on head-read error.
- `crates/api/src/network/queries.rs` + `queries_ch.rs` — `fetch_stats(.., head)`
  pins the latest-ledger row to `WHERE sequence = head`; CH TPS prune uses
  `head - 200` (drops the inner `max(sequence)` subquery → one fewer read).
- `crates/api/src/state.rs` — `network_last_good: Arc<RwLock<Option<Arc<NetworkStats>>>>`.
- Docs: `backend-overview.md` §8.1 + both `01_get_network_stats.sql`.
- `libs/api-types/{openapi.json,generated/*}` — description-only regen.

Verification: `cargo test -p api` (323 pass), clippy clean, `max` multi-agent
code review (8/9 findings actioned), 13-file leak scan clean, E2E on a local
CH backfill (one 64k partition), prod deploy via `make deploy-production-compute`

- smoke.

## Issues Encountered

- **CI `format:check --all` covers `lore/` + `docs/`.** The api-types regen
  triggered the TypeScript CI job, whose `nx format:check --all` step flagged 9
  prettier-dirty files that arrived via develop (a docs README + 8 lore task
  docs from 0293/0232/0310) — none touched by this task. Fixed by
  `prettier --write` on them (format-only commits). **Local `nx format:check`
  diverged from CI** (did not list the lore files); the reliable predictor is
  raw `prettier --check` over tracked files. Captured in memory.
- **CDK prod diff surfaced an unrelated orphan.** `make diff-production` showed
  a pending `OriginSecret` orphan in `Explorer-production-CloudflareBootstrap`
  — a committed-but-undeployed 0277 leftover, harmless (live edge-auth uses
  `EdgeSecret` in the Compute stack). Not deployed with 0291; spawned 0312.

## Broken/modified tests

- `network/cache.rs` tests rewritten for the `i64` key: the old
  `put_then_get_round_trips_within_ttl` (unit key `()`) became
  `put_then_get_round_trips_under_head_key`; added
  `hit_on_unchanged_head_recompute_on_advance` and
  `concurrent_misses_on_same_head_collapse_to_one_compute`. Intentional — the
  cache key type changed by design. Not a regression.

## Design Decisions

### From Plan

1. **Version-key on `latest_ledger_sequence`, keep `try_get_with` single-flight.**
   Core of the task — head becomes the cache key, stampede protection preserved.
2. **Live `max(sequence)` per request, not a 1s head cache** (Step 4 decision).
   Zero head lag = true "first request after write"; the probe is a single-row
   index read, so the per-request cost is negligible. Behind a small seam so a
   head cache can be swapped in later.
3. **Scope = `/v1/network/stats` only**; live lists deferred (their dominant
   cost is the query itself — 0290, independent).
4. **Shared head provider in `common/`** so 0292 (ETag) reuses one head source.

### Emerged

5. **Pin `fetch_stats(.., head)` to `WHERE sequence = head`** (not in the
   original plan). From the `max` review: keying by a separately-read head while
   the stats query re-derived its own "latest" caused a TOCTOU + a PG
   `max(sequence)`-vs-`closed_at DESC` divergence. Pinning makes
   `latest_ledger_sequence == cache key` always, and removes the double head
   read per miss. Changed the canonical SQL + both reference `.sql` docs.
6. **Last-good fallback on head-read error** (`AppState.network_last_good`).
   The per-request head read is a new hard DB dependency in front of the cache
   (a warm HIT used to need zero DB round-trips); on a transient head-read
   failure we serve the last good snapshot instead of 500, restoring "warm
   cache survives a DB/CH blip". Fails safe to 500 when no snapshot exists.
7. **CH head uses `ORDER BY sequence DESC LIMIT 1`, not `max(sequence)`.**
   `max()` over the sorting key is version/setting-dependent for index-only
   evaluation; the read-in-order form is a guaranteed one-granule read — matters
   because the head is now read on every request (CH `read_rows` quota).
8. **Backstop TTL kept at 60s** (reviewer flagged `tps_60s`/`generated_at` can
   freeze up to the backstop on a stalled head). Accepted as a documented
   trade-off rather than lowering it.
9. **Dropped the `HeadRow` struct** for the CH probe — bare scalar
   `fetch_optional::<i64>()` (mirrors the indexer's head probe).

## Future Work

- **Automated head-check-cost test** — the "head-check reads ≪ the heavy query"
  property is only verified manually (EXPLAIN / `read_rows`). A gated
  (`CLICKHOUSE_URL`) assertion like `smoke.rs` would lock it in. (Low priority,
  not yet a backlog task.)
- **0312** — deploy the CloudflareBootstrap slim-down (orphan the dead
  `OriginSecret`); unrelated 0277 leftover surfaced during the 0291 deploy.
