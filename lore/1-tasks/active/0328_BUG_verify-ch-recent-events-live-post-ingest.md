---
id: '0328'
title: 'BUG: verify CH contract `recent_events` live once ingest recovers (0300 AC#4)'
type: BUG
status: active
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

- [ ] After [[0286]] resolves and `max(closed_at)` is within ~1 day of `now()`,
      hit deployed API `GET /v1/contracts/:id` for an active contract and confirm
      `stats.recent_events` is non-zero and plausible.
- [ ] Cross-check that API number against a `chq` `count()` over `soroban_events`
      for the same contract + 7-day window (parity sanity).
- [ ] Close 0300's AC#4 with the captured numbers.
