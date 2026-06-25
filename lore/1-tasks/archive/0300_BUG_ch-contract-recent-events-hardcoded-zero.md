---
id: '0300'
title: 'BUG: CH contract detail `recent_events` hardcoded to 0 (PG parity gap)'
type: BUG
status: completed
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
  - date: '2026-06-25'
    status: active
    who: karolkow
    note: 'Promoted to active; starting implementation.'
  - date: '2026-06-25'
    status: active
    who: karolkow
    note: >
      Implemented: CH recent_events = count() over soroban_events in-window
      (replaces merge-stub 0). Switched uniqExact -> count() after measuring OOM
      on the 76M-event hot contract. Parity verified exact on live CH (no
      diagnostics in soroban_events). DTO doc + openapi regenerated; offline
      SQL-shape test added. AC 1-3 done; AC 4 (live now64() verify) blocked on
      ingest catch-up (galexie 0286, ~10d stale — escalated). Root cause traced
      to merge commit cdf67709, not a typo.
  - date: '2026-06-25'
    status: completed
    who: karolkow
    note: >
      Shipped in PR #283 (6 commits). Code: CH recent_events = count() over
      soroban_events; parity documented as a shared-source invariant (both
      tables fed by the same parser ExtractedEvent stream — verified global mix
      9.25B Contract + 4.7K System [all executable_update WASM upgrades] + 0
      Diagnostic); extracted stats_window_bounds() (list/detail share one
      window); fetch_contract_stats now returns the named ContractStats struct
      (PG + CH) instead of a positional 4-tuple, so the merge-stub bug class
      (silently plugging 0 into an unnamed slot) is now a compile error. 211/211
      api lib tests green + 2 offline SQL/window tests. Reviewed by 4 agents
      (/review, /simplify, devils-advocate red+blue, senior checklist) — all
      ship. AC 1-3 met; AC 4 (live now64() verify) deferred to follow-up,
      gated on 0286 ingest recovery. Docs/architecture: N/A (value-correctness,
      no shape change). Completed per /pr convention (task archived in the
      implementation PR).
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

- [x] CH `recent_events` returns the real windowed event count (no hardcoded
      `0`); `count()` over `soroban_events` in the same window. Parity logic
      validated on live CH (data-anchored window): events 1.27M vs invocations
      12.9K on a sample contract — coherent, non-zero.
- [x] DTO doc for `ContractStats.recent_events` accurate for both backends
      (PG `SUM(amount)` / CH `count()`; both non-diagnostic; not the `/events`
      full history).
- [x] Test covering the CH path — offline SQL-shape regression
      (`stats_sql_computes_recent_events_from_events_table`); no CH
      integration harness exists in the repo (PG-only `tests_integration.rs`).
- [ ] **(deferred to [[0328]], gated on [[0286]])** End-to-end verify against the live `now64()` window —
      currently impossible: CH ingest is ~10 days behind chain head (see
      Issues), so the wall-clock 7-day window is empty and the whole stats
      trio returns 0. Re-runnable once ingest catches up (gated on
      [[0286]] galexie disk-full). NOTE: the original "verified before the
      `API_DATASOURCE_CONTRACTS=ch` prod flip" wording is stale — the flip
      already shipped (`infra/.../compute-stack.ts`, `CONTRACTS: 'ch'`,
      `DATABASE_URL` disabled → PG path is dead code).

## Implementation Notes

- `crates/api/src/contracts/queries_ch.rs`: `StatsChRow` gains `recent_events`;
  SQL extracted to a testable `contract_stats_sql(days, ledger_floor)` builder;
  scalar subquery `SELECT count() FROM soroban_events se ...` in the same window
  (two `?` binds: events subquery, then outer invocations seek); return tuple
  3rd element now `row.recent_events` (was literal `0`).
- `crates/api/src/contracts/dto.rs`: `ContractStats.recent_events` doc corrected
  for both backends.
- `libs/api-types/{openapi.json,generated/types.gen.ts}`: regenerated (the DTO
  doc flows into the OpenAPI `description`; CI `API types freshness` gate).
- Test: `stats_sql_tests::stats_sql_computes_recent_events_from_events_table`
  (offline SQL-shape regression: asserts the events subquery exists, no literal,
  window parity, two binds).

## Design Decisions

### Emerged

1. **Root cause was a merge stub, not a typo.** The `0` entered in commit
   `cdf67709` "fix(merge): resolve compile errors after merging develop": the CH
   `fetch_contract_stats` returned a 3-tuple (events deferred, PR #237) while
   develop's handler expected the PG 4-tuple with `recent_events`; the merge fix
   plugged `0` to satisfy tuple arity. Events later shipped
   (`soroban_events`/`fetch_events`, task 0317) but the stub was never wired.

2. **`count()`, not `uniqExact`.** First implementation used
   `uniqExact((ledger_sequence, transaction_id, event_index))` for re-ingest
   dedup. It **OOMs** on the hottest contract (~76M events / 7d) — measured Code
   241 at the 3.73 GiB `read_only` cap. Switched to plain `count()`: measured
   ~99.5M rows / 1.39 GiB / 0.24 s / 89 MiB peak — safe. Dedup is unnecessary:
   `count() == count() FINAL` and `count() == uniqExact` on every sampled
   contract (1.2M–76M events), i.e. no re-ingest duplicates in practice.

3. **No `FINAL`.** `FINAL`-on-`soroban_events` is the documented OOM path
   (`fetch_events` header); the count needs no dedup (decision 2), so it is
   omitted. Mirrors the cheap-streaming intent of the `recent_invocations` path.

4. **Parity is exact, not approximate.** CH `soroban_events` holds only
   non-diagnostic contract events (parser drops diagnostics, ADR 0033; measured:
   recent slice is 100% `event_type = 1`), the same population PG's
   `soroban_events_appearances.amount` folds. So CH `count()` == PG `SUM(amount)`
   by construction.

5. **Window-staleness left as-is (out of scope).** The `now64()` window zeros
   the whole stats trio (and network `tps_60s`) when ingest lag exceeds the
   window. Inherited from `recent_invocations`, correct under healthy ingest —
   flagged for a separate staleness-aware-window task, not fixed here.

## Issues Encountered

- **Live `0` is currently NOT this bug.** CH ingest is ~10 days behind chain
  head (head `closed_at = 2026-06-15`; last `system.parts` write ~22 h before
  probe), so the wall-clock 7-day window is empty and _all three_ stats read 0
  regardless of the fix. Consistent with the unfixed galexie disk-full bug
  [[0286]] recurring (escalated separately). This blocks AC #4 and means the fix
  has no observable effect until ingest recovers.
- **No CH integration test harness.** `tests_integration.rs` is PG-only
  (`DATABASE_URL`); building a seeded-CH harness was out of proportion for a
  one-tuple-element fix, hence the offline SQL-shape test.
