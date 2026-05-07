---
id: '0201'
title: 'ledgers detail Cache-Control test flake on shared dev DB'
type: BUG
status: active
related_adr: []
related_tasks: ['0047']
tags: [priority-low, effort-small, layer-api, layer-tests, flake]
links:
  - crates/api/src/tests_integration.rs
  - crates/api/src/ledgers/handlers.rs
  - crates/api/src/ledgers/queries.rs
history:
  - date: '2026-05-07'
    status: active
    who: stkrolikiewicz
    note: >
      Discovered while running the full `cargo test -p api` suite during
      task 0192 verification. Test fails on `develop` too (reproduced via
      `git stash` on the 0192 branch), so it is unrelated to 0192's work.
      Spawned as a separate task per CLAUDE.md "Task-Gated Development"
      rule. Originally drafted as 0194 against a stale develop-merge-base
      that did not see the existing `0194_FEATURE_*` task; renumbered to
      0201 (next free ID after develop fast-forward) before push.
---

# `ledgers_detail_returns_header_and_cache_control_against_real_db` flakes on shared dev DB

## Summary

The test at
[crates/api/src/tests_integration.rs:1786](../../../crates/api/src/tests_integration.rs)
picks the two most-recent ledgers via
`SELECT sequence FROM ledgers ORDER BY closed_at DESC LIMIT 2`, treats
the first as the chain head, and asserts the `/v1/ledgers/{seq}` response
for that head carries `Cache-Control: public, max-age=10` (the SHORT TTL
branch).

On a dev DB shared with `persist_integration` test fixtures (which insert
synthetic ledgers at sequences `90_000_001`, `90_000_002`, `90_000_003`
with the **same** `closed_at = '2026-04-25 12:00:00+00'`), the
`ORDER BY closed_at DESC` clause is non-deterministic across the tied
rows: PostgreSQL may return any of the three. When the DB returns one
of the lower sequences (90_000_001 or 90_000_002), the handler's
`next_sequence` LATERAL lookup finds a successor in DB and emits the
LONG branch (`max-age=300`) — assertion fails.

The handler is correct: "head" is defined as `next_sequence IS NULL`,
not as the latest `closed_at`. The test's "pick the head" SELECT is the
bug; it should use `(closed_at DESC, sequence DESC)` to break ties
deterministically by chain position.

## Status: Active

**Current state:** root cause confirmed; fix is a one-line ORDER BY
clause change in the test.

## Context

The handler ([`crates/api/src/ledgers/handlers.rs:185-209`](../../../crates/api/src/ledgers/handlers.rs))
selects between two cache-TTL branches:

- `CACHE_CONTROL_SHORT` (`public, max-age=10`) when
  `body.next_sequence.is_none()` — the row has no successor in DB,
  i.e. is the chain head.
- `CACHE_CONTROL_LONG` (`public, max-age=300`) otherwise — the row is a
  closed (immutable) ledger.

`next_sequence` comes from a LATERAL lookup
([queries.rs:164-170](../../../crates/api/src/ledgers/queries.rs))
that selects `MIN(sequence)` strictly greater than the requested
sequence — pure sequence-based, no `closed_at` involvement.

The test introduced in commit `200452fc` (Karol, 2026-04-29, task 0047)
assumed `ORDER BY closed_at DESC LIMIT 1` would deterministically pick
the chain head. On a clean production-shape DB this holds because
Stellar ledger close times are monotonic with `sequence`. On a dev DB
that has been touched by `persist_integration` test fixtures, three
synthetic ledger rows share an identical `closed_at`, so the head pick
is undefined.

### Reproduction

```bash
DATABASE_URL=postgres://postgres:postgres@localhost:5435/soroban_block_explorer \
  cargo test -p api ledgers_detail_returns_header_and_cache_control \
  -- --nocapture
```

Failure on stashed clean develop state too — confirms the flake is
pre-existing and not introduced by task 0192.

```sql
SELECT sequence, closed_at FROM ledgers ORDER BY closed_at DESC LIMIT 5;
-- 90000001 | 2026-04-25 12:00:00+00
-- 90000003 | 2026-04-25 12:00:00+00
-- 90000002 | 2026-04-25 12:00:00+00
-- 62046000 | 2026-04-09 21:54:22+00
-- 62045999 | 2026-04-09 21:54:16+00
```

## Implementation Plan

### Step 1: Fix the test ORDER BY tie-breaker

Change the `head_seq` and `closed_seq` SELECT to:

```sql
SELECT sequence FROM ledgers ORDER BY closed_at DESC, sequence DESC LIMIT 2
```

This matches the list-endpoint canonical ordering
(`closed_at DESC, sequence DESC` per
[`LIST_PROJECTION` in ledgers/queries.rs](../../../crates/api/src/ledgers/queries.rs))
and deterministically picks the actual chain head even when fixture
ledgers share a `closed_at`.

The `closed_seq` (rows[1]) becomes the row immediately preceding the
head by sequence, which still satisfies the test's intent ("a closed
ledger that is not the head"). On a production DB the change is a
no-op because `closed_at` ties don't occur there.

### Step 2: Verify

Run the test against the shared dev DB; assert it now passes
deterministically across multiple runs.

## Acceptance Criteria

- [ ] Test passes against the shared dev DB (`sbe-fresh-postgres-1`)
      with `persist_integration` fixtures present.
- [ ] Test still passes against a clean DB.
- [ ] No production code change required (handler is correct).
- [ ] No follow-up task needed for `persist_integration` fixture
      cleanup — the test is the right place to fix this because the
      fixtures are intentionally distinct from chain ledgers and the
      test's tie-breaking responsibility is well-defined.

## Notes

- Not in scope of any architecture-affecting docs (no schema, API, or
  pipeline change). `docs/architecture/**` update not required per ADR 0032.
