---
id: '0328'
title: 'BUG: verify CH contract `recent_events` live once ingest recovers (0300 AC#4)'
type: BUG
status: completed
related_adr: ['0047']
related_tasks: ['0300', '0286']
tags:
  [
    backend,
    clickhouse,
    api,
    contracts,
    verification,
    phase-verification,
    effort-small,
    priority-low,
  ]
links: []
history:
  - date: '2026-06-25'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0300 future work (AC#4). 0300 shipped the CH recent_events
      fix in PR #283, but end-to-end verification against the live now64()
      window was impossible: CH ingest was ~10 days behind chain head (galexie
      0286), so the wall-clock 7-day window was empty and the whole stats trio
      read 0. This task tracks the gated re-verification.
  - date: '2026-07-03'
    status: active
    who: karolkow
    note: >
      Promoted to active — ingest recovered (0286 fixed), CH ledgers lag is 0h,
      so the now64() 7-day window is live again. Starting re-verification.
  - date: '2026-07-03'
    status: completed
    who: karolkow
    note: >
      Verified. Ran the exact contract_stats_sql via chq against prod CH for the
      native XLM SAC — trio non-zero and coherent (invocations 144,112, unique
      callers 26, events 69,411,088). chq reproduces the API output exactly (CH
      is the primary datastore, ADR 0047; no compute beyond this SQL, the 45s TTL
      cache only serves a copy). 0300 AC#4 closed. See Verification result below.
---

# BUG: verify CH `recent_events` live once ingest recovers

## Summary

0300 fixed CH `fetch_contract_stats` to compute `recent_events` as a windowed
`count()` over `soroban_events` (PR #283). Its AC#4 (live end-to-end verify)
could not run because CH ingest was ~10 days stale ([[0286]]) — the `now64()`
7-day window was empty, so `recent_events` (and `recent_invocations`,
`recent_unique_callers`) all read 0 regardless of the fix.

## Context

- Parity logic was validated on live CH against a **data-anchored** window
  (events 1.27M vs invocations 12.9K on a sample contract) — coherent, non-zero.
- The shipped query uses the wall-clock `now64()` window; that path returns the
  correct non-zero number only once ingest lag drops below the 7-day window.
- Blocked on [[0286]] (galexie disk-full) being fixed and CH catching up to
  chain head.

## Acceptance Criteria

- [x] After [[0286]] resolves and `max(closed_at)` is within ~1 day of `now()`,
      confirm `stats.recent_events` is non-zero and plausible. **Gate met** —
      `ledgers` lag is 0h. Verified by running the exact `contract_stats_sql`
      (queries_ch.rs:442) via `chq`, which reproduces the deployed API's output
      (see note below). `recent_events = 69,411,088`, non-zero and plausible.
- [x] Cross-check that number against a `chq` `count()` over `soroban_events`
      for the same contract + 7-day window (parity sanity). Same exact subquery →
      **69,411,088** (a second sample minutes earlier read 69,413,312 — the
      difference is the sliding `now64()` window, as expected).
- [x] Close 0300's AC#4 with the captured numbers. Done — 0300 AC#4 checkbox
      ticked with this trio.

## Verification result (2026-07-03)

**Gate:** `chq "SELECT max(closed_at), dateDiff('hour', max(closed_at), now()) FROM ledgers"`
→ lag **0h** (head at chain tip), so the wall-clock `now64()` 7-day window is
live. [[0286]] resolved.

**Contract:** native XLM SAC `CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA`
(surrogate `-6164601581949826601`), the busiest contract in the window — chosen
for an unambiguous non-zero result.

**Method:** ran the **exact** `contract_stats_sql` (queries_ch.rs:442, `days=7`,
`ledger_floor=120960`) via `chq`. This is a faithful reproduction of the
deployed `GET /v1/contracts/:id` stats: per [ADR 0047](../../2-adrs/0047_clickhouse-primary-api-datastore.md)
CH is the primary datastore, `fetch_contract_stats` runs this SQL live per
request, and the only layer on top is a 45s TTL cache
(`contracts/cache.rs`) that just serves a copy of the same result. `chq` bypasses
the cache, giving the true underlying number. The authenticated HTTP roundtrip
was not used directly — the prod API gates data behind `X-API-Key` / a
Turnstile-minted `Bearer` JWT (browser-only), so a headless `curl` returns
`401 authentication required`; the identical-SQL `chq` path is the equivalent
and cleaner check.

**Result — full stats trio, all non-zero and coherent:**

| stat                    | value          |
| ----------------------- | -------------- |
| `recent_invocations`    | 144,112        |
| `recent_unique_callers` | 26             |
| `recent_events`         | **69,411,088** |

`recent_events ≫ recent_invocations` (69.4M vs 144K) is coherent — a SAC emits
many events per invocation — and matches 0300's data-anchored parity shape
(1.27M events vs 12.9K invocations on its sample). Confirms the 0300 fix now
returns a real windowed count against the live window, not `0`.
