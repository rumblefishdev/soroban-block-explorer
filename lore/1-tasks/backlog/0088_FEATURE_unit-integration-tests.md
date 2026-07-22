---
id: '0088'
title: 'Unit and integration tests: XDR parsing, API endpoints'
type: FEATURE
status: backlog
related_adr: ['0005']
related_tasks: ['0092']
tags: [priority-high, effort-large, layer-testing]
milestone: 3
links:
  - docs/architecture/technical-design-general-overview.md
history:
  - date: 2026-03-30
    status: backlog
    who: fmazur
    note: 'Task created — D3 scope coverage (task 0085)'
  - date: 2026-03-31
    status: backlog
    who: stkrolikiewicz
    note: 'Updated per ADR 0005: NestJS test patterns → cargo test + tokio::test'
  - date: 2026-04-01
    status: backlog
    who: fmazur
    note: 'Updated: removed Event Interpreter test references. Enrichment deferred.'
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Premise stale — the tests largely exist, 2026-07-22.**
      Counted in the tree: **890 Rust tests total, 322 in `xdr-parser`, 244 in
      `api`** — the two areas this task names. They accreted through a year of
      feature work rather than through this task, which has never been touched.
      Consequence: `priority-high` + `effort-large` is now misleading and outranks
      genuinely open work in every sort. What is actually missing is narrower and
      already owned elsewhere: CI never *runs* the ClickHouse-gated suite (0406)
      and the API integration tests still need rebuilding on CH fixtures (0360).
      Re-scope to "identify the remaining coverage gap" or close in favour of
      those two; do not start from the assumption that there are no tests.
---

# Unit and integration tests: XDR parsing, API endpoints

## Summary

Write unit tests for XDR parsing correctness and API endpoint responses. Write integration tests covering the end-to-end pipeline: Galexie → S3 → Indexer Lambda → PostgreSQL → API Lambda. D3 acceptance criteria require test coverage across both layers.

## Status: Backlog

**Current state:** Not started.

## Context

D3 (§7.4) requires "Unit and integration tests covering XDR parsing correctness and API endpoint responses." The effort breakdown (§7.1F) allocates: unit tests API — 8 days, unit tests XDR/ingestion — 7 days, integration tests e2e — 5 days (20 days total).

## Implementation Plan

### Step 1: Unit tests — XDR parsing + ingestion correctness (7 days)

Test XDR deserialization for LedgerCloseMeta, operations, soroban events/invocations, and ledger entry changes against known mainnet transaction hashes. Verify field extraction, edge cases, and error handling.

### Step 2: Unit tests — API endpoints (8 days)

Test all axum handler/query module layers using `cargo test` and `tokio::test`: request validation, response shape, pagination, error envelopes, filter parameters. Mock database layer with sqlx test fixtures.

### Step 3: Integration tests — end-to-end (5 days)

Test Galexie → S3 → Indexer → PostgreSQL → API pipeline with real database using `tokio::test` and `sqlx::test`. Ingest known testnet ledgers, query API endpoints, verify data consistency.

## Acceptance Criteria

- [ ] XDR parsing tests cover all 4 parsing tasks (0024–0027)
- [ ] API endpoint tests cover all feature modules (0045–0053)
- [ ] Integration tests verify ingestion → API data consistency
- [ ] All tests pass in CI pipeline
