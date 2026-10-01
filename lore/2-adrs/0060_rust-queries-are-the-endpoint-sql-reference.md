---
id: '0060'
title: 'The Rust queries and their ClickHouse tests are the endpoint SQL reference; the hand-kept SQL set is retired'
status: accepted
deciders: [karolkow]
related_tasks: ['0588', '0478', '0207']
related_adrs: ['0044', '0047', '0032']
tags: [docs, clickhouse, api, testing]
links: []
history:
  - date: '2026-09-28'
    status: accepted
    who: karolkow
    note: >
      Decided on review of 0478's PR, which built a CI gate to keep the
      endpoint-query SQL set parseable. Implemented in 0588: the set is
      deleted, the few notes worth keeping moved beside the Rust queries.
---

# ADR 0060: The Rust queries and their ClickHouse tests are the endpoint SQL reference; the hand-kept SQL set is retired

**Related:**

- [Task 0588: retire the endpoint-query SQL reference set](../1-tasks/active/0588_REFACTOR_retire-endpoint-query-sql-reference-set.md)
- [Task 0478: repair the Tier-1 query docs and gate them in CI (superseded)](../1-tasks/archive/0478_REFACTOR_tier1-gate-repair-and-ci.md)
- [Task 0207: the ClickHouse endpoint-query reference set](../1-tasks/archive/0207_FEATURE_clickhouse-endpoint-queries-reference-set.md)
- [Task 0480: which tests actually run](../1-tasks/backlog/0480_RESEARCH_test-coverage-and-ci-execution-audit.md)
- [ADR 0047: ClickHouse as the primary API datastore](./0047_clickhouse-primary-api-datastore.md)

---

## Context

`docs/architecture/database-schema/endpoint-queries-clickhouse/` held one
hand-written `.sql` file per public endpoint, a README with conventions, and a
runner script. Task 0207 wrote it in May 2026 as the reference read plan for the
ClickHouse pilot, before the API read from ClickHouse. ADR 0047 cites it as the
evidence that the query patterns work on ClickHouse.

Since then the Rust query functions in `crates/api/src/<module>/queries…` have
become the only SQL that runs, and the set turned into a copy of them kept by
hand. Measured on 2026-09-28:

- **The copy is expensive.** 92 of the 317 commits that touched `crates/api` in
  the previous 90 days also had to edit the set.
- **It drifts anyway.** Before task 0478, seven files did not parse. After it,
  `06` and `23` were still stale; `23` showed a `JOIN accounts FINAL` that task
  0354 had removed from the Rust. Several files carried banners saying their
  body was no longer the query the API runs.
- **A gate on the copy checks only the copy.** Task 0478 built a CI step that
  parse-checks every file. It could prove that the copies plan, never that they
  match the Rust.
- **The real reference already exists.** Every API module (accounts, assets,
  contracts, ledgers, liquidity_pools, network, nfts, search, transactions) has
  ClickHouse-backed tests (`decode_smoke`, `ch_tests`) that execute the Rust
  query itself against a server built from `crates/db-clickhouse/schema/init.sql`.
- **The executed SQL is recorded.** Production `system.query_log` holds the
  exact text of every query the API ran, readable with `chq`, at no maintenance
  cost.

## Decision

The Rust query functions are the endpoint SQL reference. Their ClickHouse tests
are the check that they run against the canonical schema. The hand-kept set is
deleted, not gated.

- A note about a query (an index choice, why `FINAL` is or is not used, a
  ClickHouse trap, a measured cost) lives as a comment beside the Rust function
  that runs the query.
- To see the SQL an endpoint executes, read its Rust query; for the exact text
  with bound values, read `system.query_log` with `chq`.
- No document maintains a second copy of endpoint SQL.

## Rationale

A reference is only worth keeping if something stops it from going wrong. The
Rust query cannot drift from itself, and its tests run it against the schema.
The copies had no such check, and the only check 0478 could add was a weaker
one: that each copy parses. Keeping them cost roughly one in three API commits
and still left them stale.

The knowledge in the file headers was the one real asset. Most of it had
already moved into the Rust comments as the queries were rewritten; the rest
moved there in task 0588, and stale or duplicated text was dropped.

## Alternatives Considered

### Alternative 1: Gate the copies in CI (task 0478)

**Description:** repair every file so it parses, and run the runner's
`--syntax-only` mode as a CI step against the canonical schema.

**Pros:**

- Catches a copy that no longer parses.
- Keeps a readable SQL file per endpoint.

**Cons:**

- Checks the copy, not the query the API runs; a copy that parses can still
  differ from the Rust, and the gate cannot see it.
- Keeps the per-commit maintenance cost and adds a CI step to maintain.

**Decision:** REJECTED — built, reviewed and closed unmerged (0478). It makes the
copy cheaper to keep correct-looking, not correct.

### Alternative 2: Generate the files from the Rust

**Description:** extract each query string from the Rust into `.sql` files, by
a build step or a test that writes them.

**Pros:**

- No hand maintenance; the files would always match the Rust.

**Cons:**

- Many queries are assembled at run time (`format!` fragments for filters,
  keysets, sort direction, inlined key lists), so there is no single string to
  extract without running the code path.
- The output would carry none of the notes that made the set useful, and the
  Rust file is already the readable form of the same text.

**Decision:** REJECTED — a generated copy adds a build artefact and tells a
reader nothing the Rust does not.

## Consequences

### Positive

- An API change edits one place.
- No document can claim a query shape the API no longer runs.
- The notes a reader needs sit next to the code they explain.

### Negative

- There is no single directory to browse every endpoint's SQL; the reader goes
  module by module (`crates/api/src/<module>/queries…`).
- The exact executed SQL needs production access (`chq` on `system.query_log`),
  or a local run of the API.
- **The ClickHouse tests of the `api` crate do not run in CI today.** They are
  gated on `CH_URL`, which no workflow sets, and the "ClickHouse e2e" step tests
  other crates only. Until that is wired (task 0480 tracks it), they run where a
  developer runs them against the local Docker ClickHouse. Three LP smokes also
  need real data, not just the schema. The copies were never a substitute for
  this: they checked nothing about the Rust either.
- The `compare-with-stellar-api` skill now starts from an endpoint or a Rust
  query instead of an SQL file, and the `run_endpoint_ch.sh` runner is gone.

---

## Delivery Checklist

Per [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md):

- [x] `docs/architecture/technical-design-general-overview.md` — N/A, does not reference the set
- [x] `docs/architecture/database-schema/database-schema-overview.md` updated — the "Canonical query references" section now points at the Rust queries
- [x] `docs/architecture/database-schema/clickhouse-pilot.md` updated — the "Read queries (reference set)" section records the retirement
- [x] `docs/architecture/backend/backend-overview.md` updated — links to the SQL files replaced by the Rust query modules
- [x] `docs/architecture/frontend/frontend-overview.md` — N/A, does not reference the set
- [x] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` — N/A, does not reference the set
- [x] `docs/architecture/infrastructure/infrastructure-overview.md` — N/A, does not reference the set
- [x] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` — N/A, does not reference the set
- [x] This ADR is linked from each updated doc at the relevant section

ADR 0047 still names the set as pilot validation; that was true when it was
written, and its body stays as it is. This ADR records the change.
