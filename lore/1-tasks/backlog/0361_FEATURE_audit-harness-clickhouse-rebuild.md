---
id: '0361'
title: 'Rebuild audit-harness CH-native (data-correctness safety net for ClickHouse)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0244', '0175']
tags: ['clickhouse', 'audit', 'data-correctness', 'tooling']
links: []
history:
  - date: 2026-07-06
    status: backlog
    who: karolkow
    note: 'Spawned from 0244 (PG removal) — the PG-bound audit-harness was deleted; rebuild it CH-native.'
---

# Rebuild audit-harness CH-native (data-correctness safety net for ClickHouse)

## Summary

The old `crates/audit-harness` was the automated data-correctness safety net —
per-table SQL invariants across all rows plus DB↔Horizon and DB↔archive-XDR
diffs — the automated complement to the manual PR audits (0167/0172/0173). It was
`sqlx`/PostgreSQL-bound and became non-functional at the ClickHouse cutover, so
task 0244 deleted it (moved to `.trash/`) rather than carry PG-shaped dead code
through the remove-PG refactor. This task rebuilds it against ClickHouse from
scratch — a CH-native tool, not a mechanical port of the PG SQL.

## Status: Backlog

**Current state:** not started. Deferred deliberately from 0244 so the PG
teardown was not blocked. Until this lands there is **no automated correctness
net for the ClickHouse store** (there has been none since the cutover — this task
makes the gap explicit and tracked, it does not create it).

## Context

Why rebuild rather than port:

- CH has different idioms than PG — `FINAL` / `argMax(_, version)` over
  `ReplacingMergeTree`, no foreign keys, `intDiv(ledger_sequence, 500000)`
  partitions, `Int64` cityhash surrogates, `Decimal128(7)` amounts. The
  invariants themselves change; a line-by-line SQL port would be awkward and
  half-right.
- The three diff binaries (`horizon-diff`, `archive-diff`,
  `operations-order-diff`) read the DB via `sqlx` — swap for a
  `clickhouse::Client` (mTLS via `db_clickhouse::mtls` for prod, plain client
  for local).
- The XDR re-parse ground-truth path (`xdr-parser`, `stellar-xdr`, archive
  `.xdr.zst` fetch) is store-agnostic and carries over unchanged.

Reference: the deleted crate is preserved in git history and under
`.trash/` on the `refactor/0244_remove-postgres-sqlx-entirely` branch; the CH
schema authority is `crates/db-clickhouse/schema/init.sql` +
`docs/architecture/database-schema/clickhouse-pilot.md`.

## Implementation Plan

### Step 1: CH connection + shape

Stand up a `clickhouse::Client` (local + mTLS prod) and port the CLI surface
(`clap`) so the harness targets the CH store.

### Step 2: Phase-1 CH invariants

Rewrite the per-table invariant SELECTs as ClickHouse SQL, accounting for
`FINAL`/`argMax` version collapse, no-FK orphan checks via joins/anti-joins,
and partition-pruned scans. Keep the `(violations, sample)` contract and the
0/1/2 exit-code convention.

### Step 3: Phase-2 diff binaries on CH

Repoint `horizon-diff` (DB↔Horizon), `archive-diff` (DB↔archive-XDR ground
truth), and `operations-order-diff` (apply-order preservation) to read CH.

### Step 4: Wire into the correctness pipeline

Decide run cadence (manual `reports/` runs vs a scheduled job) and funnel
findings into the bug-task pipeline, same as the old harness.

## Acceptance Criteria

- [ ] `audit-harness` (or its successor) reads ClickHouse via `clickhouse::Client`, no `sqlx`
- [ ] Phase-1 invariants run against the live CH schema and return `0 violations` on a clean store
- [ ] `horizon-diff` + `archive-diff` + `operations-order-diff` run against CH
- [ ] A saved report is produced under `reports/` and green on current prod data
- [ ] **Docs updated** — `N/A` unless the rebuild changes the shape of the system
- [ ] **API types regenerated** — `N/A` (no `crates/api/**` change)

## Notes

- Spawned by task **0244** (remove Postgres/sqlx entirely), decision C on the
  audit-harness deferral (2026-07-06).
- Original crate: `crates/audit-harness` (task 0175 Phase 1+). Purpose and
  invariant catalogue are in its old `README.md` / `sql/` (git history +
  `.trash/`).
- Optional: consider whether the manual-audit skill `compare-with-stellar-api`
  (also PG-era) should be refreshed alongside.
