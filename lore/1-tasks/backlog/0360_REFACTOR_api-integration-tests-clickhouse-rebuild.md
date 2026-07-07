---
id: '0360'
title: 'REFACTOR: rebuild API integration tests on ClickHouse fixtures (was PG, dropped in 0244)'
type: REFACTOR
status: backlog
related_adr: ['0047']
related_tasks: ['0244']
tags: [layer-api, testing, clickhouse, cleanup]
links:
  - crates/api/src/common/ch.rs
  - .trash/api-pg-queries-0244/tests_integration.rs
history:
  - date: '2026-07-06'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0244 (Postgres removal). The API integration suite
      (crates/api/src/tests_integration.rs, 5448 lines: 115 DATABASE_URL-gated
      tests + ~98 Postgres-SQL seed/teardown fixtures) could not be ported
      mechanically — its fixtures INSERT into PG tables with PG semantics
      (partitions, sqlx query builders). Team decision: drop it in 0244 to
      unblock the PG removal, rebuild it on CH here. The original was moved to
      .trash/api-pg-queries-0244/tests_integration.rs (recover from there).
  - date: '2026-07-06'
    status: backlog
    who: karolkow
    note: >
      Renumbered 0357 → 0360. Id 0357 was triple-claimed on 2026-07-06; PERF
      launch-readpath task retains 0357 (already on origin/develop). This task
      had no inbound references. Was still untracked when renumbered.
---

# Rebuild API integration tests on ClickHouse fixtures

## Summary

Task 0244 removed Postgres from the API crate. The large
`tests_integration.rs` suite was Postgres-bound — not just reading from PG but
seeding it with PG-specific SQL fixtures — so it could not be ported
mechanically and was dropped (moved to `.trash/`). This task rebuilds that
integration coverage against ClickHouse so the endpoints regain their
end-to-end tests.

## Context

The old suite (`.trash/api-pg-queries-0244/tests_integration.rs`, 5448 lines)
had ~115 `DATABASE_URL`-gated tests. Roughly 98 of them relied on
`INSERT INTO` / `DELETE FROM` fixtures against PG tables (`liquidity_pools`,
`accounts`, `lp_positions`, `transactions`, …) using `sqlx` query builders and
PG partition semantics. ClickHouse writes differently (append-only /
`ReplacingMergeTree`, no PG-style partitions), so the fixtures need a genuine
rewrite, not a find-and-replace.

The small conditional-GET tests (ETag / 304 machinery, no fixtures) were
already ported in 0244 and now use `crate::common::ch::test_client_from_env()`
(a `CH_URL`-gated shared helper) plus
`AppState::for_tests(ch_client, runtime_enrichment)`. This task follows the
same client/gating pattern for the fixture-heavy tests.

## Implementation Plan

### Step 1: Recover + triage

Recover the original from `.trash/api-pg-queries-0244/tests_integration.rs`.
Split the tests into (a) no-fixture tests already covered by the ported
conditional-GET tests (drop as duplicates), (b) fixture-dependent tests worth
rebuilding, (c) obsolete tests (PG-shape-specific, no CH analog).

### Step 2: CH fixture helpers

Build seed/teardown helpers that INSERT into the CH tables the endpoints read
(`ledgers`, `transactions`, `soroban_*`, `liquidity_pool*`, `accounts`, …),
respecting `ReplacingMergeTree` version semantics and `FINAL`-read behaviour.
Gate on `CH_URL` (reuse `common::ch::test_client_from_env`).

### Step 3: Port the tests

Rebuild the group-(b) tests against the CH fixtures + a real
`AppState::for_tests(ch, ..)`.

### Step 4: Wire into CI (optional)

Decide whether the suite runs in CI against an ephemeral CH service, or stays
`CH_URL`-gated local-dev only (as it was `DATABASE_URL`-gated before).

## Acceptance Criteria

- [ ] Fixture-dependent API integration tests run green against a real CH
- [ ] `CH_URL`-gated: skip cleanly (no failure) when `CH_URL` is unset
- [ ] No residual `sqlx` / `PgPool` / `DATABASE_URL` in the rebuilt suite
- [ ] `.trash/` original removed once its useful tests are recovered
- [ ] **Docs updated** — N/A (test-only, no change to the described system shape)
- [ ] **API types regenerated** — N/A (test-only, no handler/DTO change)

## Notes

- Depends on nothing — 0244 already made the API CH-only. Can start anytime.
- The 3 already-ported conditional-GET tests (ledgers / network / transactions
  handlers `#[cfg(test)]` modules) are the reference pattern for client build +
  gating.
