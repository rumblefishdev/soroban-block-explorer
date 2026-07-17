---
id: '0405'
title: 'CI: actually run the ClickHouse-gated tests — 25 files of e2e that no pipeline has ever executed'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0394', '0388', '0392', '0304']
tags: [priority-high, effort-small, area-ci, clickhouse, robustness]
links:
  - .github/workflows/ci.yml
history:
  - date: 2026-07-17
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned while closing 0394, whose AC2 ("all three CH-gated e2e seed and pass")
      had to be satisfied by hand because nothing else can. Verified 2026-07-17:
      **zero references to clickhouse in any file under .github/workflows/**, while
      25 files under crates/ carry `CLICKHOUSE_URL`-gated tests. This is the root
      cause of the 0388 / 0394 / 0392-#341 family — the tests that would have caught
      the stale `name` column exist, and have never run.
---

# CI: actually run the ClickHouse-gated tests

## Summary

The repo has a substantial ClickHouse e2e suite that **no pipeline has ever
executed**. The tests are gated on `CLICKHOUSE_URL`; CI never sets it and never
provisions a server, so every one of them skips — silently and green.

## Context

Spawned from [0394](../archive/0394_BUG_backfill-runner-stale-name-column-sweep.md).
The pattern that task swept up is the cost of this gap, stated plainly:

- 0304 dropped the `name` column from `soroban_contracts` / `assets`.
- Broken SQL then reached **prod** and aborted real maintenance passes — 0388's
  `repair-tier1` deployer reconstruction died on `unknown column name`, and 0392's
  live indexer threw `Code 47` (20,494 failures in 7 days).
- Four separate PRs each fixed one copy. The e2e that would have caught all of
  them on the first PR **were already written** — they just never ran.

A green "Rust (clippy, test)" check is actively misleading here: it passes while
skipping every test that touches the database. PR #342's green CI said nothing at
all about the ACs it was supposed to satisfy.

## Implementation

- [ ] Add a ClickHouse service to the Rust CI job (image pinned to the prod major
      — **26.3**; prod ran 26.3.12.3 as of 2026-07-17) and set `CLICKHOUSE_URL`.
- [ ] Apply `crates/db-clickhouse/schema/init.sql` before the suite (it creates 31
      tables) — several tests seed real tables and assume the current schema.
- [ ] Handle the **two different gates** (0394 documented both): the
      `backfill-runner` tests read `CLICKHOUSE_URL` and self-skip; the
      `backfill-enrichment-runner` ones are additionally `#[ignore]`d and need
      `--ignored`. A job that forgets `--ignored` silently covers only part of the
      suite — the same silent-skip failure this task exists to end.
- [ ] Make a skip **visible**: if `CLICKHOUSE_URL` is set but a test skips anyway,
      that should fail rather than pass quietly.
- [ ] Confirm isolation before wiring it up: the enrichment tests seed the real
      `assets` / `nfts` / `*_enrichment` tables and clean up with
      `ALTER TABLE … DELETE`, so they need a throwaway server, never a shared one.
      The `backfill-runner` pair already creates and drops its own database.

## Acceptance Criteria

- [ ] CI provisions ClickHouse and runs the gated suite, including the `#[ignore]`d
      tests, on every PR touching `crates/**`.
- [ ] A deliberately broken column reference (e.g. re-introducing `sc.name`) makes
      CI **red** — verify by trying it, not by assuming.
- [ ] The run is visible in the job log: number of CH tests executed, not just
      "ok. N passed" with the CH ones filtered out.
- [ ] Docs updated — `N/A` (CI tooling; CLAUDE.md names this a legitimate N/A case).
- [ ] API types regenerated — N/A (no `crates/api/**` change).
