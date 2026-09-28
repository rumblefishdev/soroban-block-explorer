---
id: '0592'
title: 'BUG: contract reads join wasm_interface_metadata without deduplication — the comment calls it a plain MergeTree'
type: BUG
status: active
related_adr: []
related_tasks: ['0327', '0588']
tags: [api, clickhouse, contracts, effort-small, priority-low]
links: []
history:
  - date: '2026-09-28'
    status: active
    who: karolkow
    note: >
      Found while retiring the endpoint SQL set (0588): the retired SQL joined
      wasm_interface_metadata with FINAL, the Rust does not, and its comment
      says the table is a plain MergeTree. Production says otherwise.
---

# BUG: contract reads join `wasm_interface_metadata` without deduplication

## Summary

`crates/api/src/contracts/queries.rs` joins `wasm_interface_metadata` twice
(`fetch_contract` and the interface query) with no `FINAL` and no argMax. The
comment above `fetch_contract` says the table is a plain `MergeTree`, so
`FINAL` would be rejected with `ILLEGAL_FINAL`. It is not a plain MergeTree:
`init.sql` and production both have `ReplacingMergeTree ORDER BY wasm_hash`.
A wasm hash written twice before a merge would join twice. The contract row
would then come back duplicated or with an arbitrary copy of the metadata.

## Context — measured 2026-09-28 (production, read-only)

- `system.tables`: `engine_full = ReplacingMergeTree ORDER BY wasm_hash`,
  metadata modified 2026-06-24.
- Today: 5 133 physical rows = 5 133 distinct hashes, 3 parts. No duplicate
  exists now; this is a latent defect plus a false comment.
- The comment's `ILLEGAL_FINAL` claim (task 0327) cannot hold against this
  engine. Whatever broke then, `FINAL` is legal on the current table, but the
  new SQL must be proven against a real ClickHouse, not only compiled.

## Acceptance Criteria

- [ ] Both joins read one row per `wasm_hash` (FINAL, or an equivalent
      dedup); the comment states the real engine and why
- [ ] A ClickHouse-gated test inserts the same `wasm_hash` twice (two parts,
      no merge) and asserts one contract row with the latest metadata; it
      fails on the pre-fix SQL
- [ ] Both queries run against the local docker ClickHouse (`CH_URL` set) and
      read-only against production (`chq`, same SQL with a real contract id)
- [ ] **Docs updated** — N/A: no change to the system's shape
- [ ] **API types regenerated** — N/A unless a DTO/handler doc comment changes
