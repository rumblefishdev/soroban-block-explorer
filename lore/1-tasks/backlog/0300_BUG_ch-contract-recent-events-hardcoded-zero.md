---
id: '0300'
title: 'BUG: CH contract detail `recent_events` hardcoded to 0 (PG parity gap)'
type: BUG
status: backlog
related_adr: ['0047']
related_tasks: ['0243']
tags:
  [
    backend,
    clickhouse,
    api,
    parity,
    contracts,
    priority-medium,
    effort-small,
    phase-implementation,
  ]
links: []
history:
  - date: '2026-06-17'
    status: backlog
    who: karolkow
    note: >
      Found during the 0243 CH read-path review (parity audit). The CH
      `fetch_contract_stats` returns a hardcoded `0` for `recent_events`
      while the PG path computes the real windowed count — a silent
      user-visible regression once the contracts module flips to CH.
      Pre-existing on develop (stkrolikiewicz's CH contracts-stats), not
      introduced by 0243; spawned as its own task.
---

# BUG: CH contract detail `recent_events` hardcoded to 0

## Summary

`GET /v1/contracts/:id` `stats.recent_events` is **always `0`** on the
ClickHouse path, while the Postgres path returns the real count of events
in the stats window. Silent, user-visible wrong number once contracts is
served from CH.

## Context

- **PG** (`crates/api/src/contracts/queries.rs`, `fetch_contract_stats`):
  `recent_events = COALESCE(SUM(amount), 0)` over
  `soroban_events_appearances` in the window.
- **CH** (`crates/api/src/contracts/queries_ch.rs`, `fetch_contract_stats`,
  ~line 283): the query only computes `recent_invocations` +
  `recent_unique_callers`; the return tuple's 3rd element (`recent_events`)
  is **hardcoded `0`** (~line 316). The handler maps it straight to
  `ContractStats.recent_events` (DTO doc still describes the PG behaviour).
- Verified still present on `origin/develop` 2026-06-17. Pre-existing
  (not from task 0243); surfaced by the 0243 parity audit.

## Implementation

- In CH `fetch_contract_stats`, compute `recent_events` as `count()` over
  `soroban_events` for the contract in the SAME window (the existing
  `ledger_sequence` floor + `ledgers.closed_at` predicate used for
  `recent_invocations`). CH stores one row per event, so `count()` is
  equivalent to PG's `SUM(amount)` over the appearance-fold table.
- Replace the hardcoded `0` in the return tuple with the computed value.
- Add a CH-mode test (or an offline mapping test) asserting `recent_events`
  is populated, mirroring the PG expectation.

## Acceptance Criteria

- [ ] CH `recent_events` returns the real windowed event count (no hardcoded
      `0`); matches the PG value for the same contract + window.
- [ ] DTO doc for `ContractStats.recent_events` accurate for both backends.
- [ ] Test covering the CH path.
- [ ] Verified before the contracts module's `API_DATASOURCE_CONTRACTS=ch`
      prod flip.
