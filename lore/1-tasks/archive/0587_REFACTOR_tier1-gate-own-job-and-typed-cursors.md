---
id: '0587'
title: 'REFACTOR: Tier-1 endpoint-query gate in its own CI job, with typed cursor checks'
type: REFACTOR
status: completed
related_adr: []
related_tasks: ['0478', '0480']
tags: [ci, clickhouse, docs, effort-small, priority-low]
links: []
history:
  - date: '2026-09-28'
    status: backlog
    who: karolkow
    note: >
      Spawned from the review of 0478's PR. Two points were deferred there
      because they change CI structure and add checks, a separate review
      sitting from the SQL repair.
  - date: '2026-09-28'
    status: completed
    who: karolkow
    note: >
      Superseded by 0588, not implemented: the gate it extends was not merged,
      because the SQL reference set is being retired.
---

# REFACTOR: Tier-1 endpoint-query gate in its own CI job, with typed cursor checks

## Summary

Task 0478 made `run_endpoint_ch.sh all --syntax-only` a real gate and put it
in CI as one step at the end of the `rust` job. That placement costs a full
Rust build for a SQL-only change, and it runs the queries on tables the e2e
suites have already filled. The gate also checks cursor and filter
parameters only with `NULL`, which always type-checks. This task moves the
gate into its own job and adds a typed pass for the files that take a
cursor.

## Context

- **Placement** (`.github/workflows/ci.yml`, step "Endpoint queries parse
  (Tier 1)"). The `rust` filter includes the query directory, so a README- or
  SQL-only PR runs clippy, both test passes and the ClickHouse e2e (~9 min on
  the last measured run) to reach a 10-second step. The step runs after the
  e2e suites, which seed rows and swap tables, so CI and a local run (empty
  tables) check different data.
- **NULL-only parameters** (`run_endpoint_ch.sh`). Every cursor and filter is
  passed as `NULL` except `08`, which 0478 checks twice (unfiltered and with
  real values). `($3 IS NULL OR (col, id) < ($3, $4))` plans with `NULL` even
  when `col` and `$3` have incompatible types.

## Implementation Plan

### Step 1: its own job

A small job: checkout, start ClickHouse and apply the schema (the same
compose commands as the `rust` job), run the gate. Trigger it on `rust ||
endpoint_sql`, where `endpoint_sql` matches only the `.sql` and `.sh` files in
`docs/architecture/database-schema/endpoint-queries-clickhouse/`. Drop that
directory from the `rust` filter and remove the step from the `rust` job.
Keep the docs-only shortcut treating those files as code.

### Step 2: typed cursor pass

For every file whose runner arm passes a cursor or filter as `NULL` (at
filing: 02, 04, 05, 07, 10, 13, 14, 15, 17, 18, 23 — re-derive the list from
the runner), add a second `check` with typed example values, the way 0478
does for 08.

## Acceptance Criteria

- [ ] A SQL- or README-only change runs the gate without building Rust.
      Shown with the job graph of a real PR run.
- [ ] The gate runs on freshly created, empty tables in CI.
- [ ] Every file that takes a cursor or filter is also checked with typed
      values; a deliberate type mismatch in one keyset predicate turns the
      gate red (negative check recorded).
- [ ] **Docs updated** — the endpoint-queries README (Tier-1 row and §Tier 1
      gate) describes the new job and the typed pass.
- [ ] **API types regenerated** — N/A: no `crates/api/**` change.

## Notes

Task 0480 wakes up the skipped ClickHouse tests in CI. This task touches the
same workflow file, so land one before the other rather than in parallel.
