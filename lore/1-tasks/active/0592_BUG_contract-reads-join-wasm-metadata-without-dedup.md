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

- [x] Both joins read one row per `wasm_hash` (FINAL, or an equivalent
      dedup); the comment states the real engine and why
- [x] A ClickHouse-gated test inserts the same `wasm_hash` twice (two parts,
      no merge) and asserts one contract row with the latest metadata; it
      fails on the pre-fix SQL (under `enable_analyzer=0` — see Issues)
- [x] Both queries run against the local docker ClickHouse (`CH_URL` set) and
      read-only against production (`chq`, same SQL with a real contract id)
      — local done (CH 26.3.21.7). Production, 2026-09-28: old and new SQL of
      both queries on 5 contracts (3 random, 2 with `upgradeable` metadata),
      output identical byte for byte in 10 of 10 runs, no exception; cost
      identical (30 069 rows / 30.26 and 29.63 MiB read per call, old and new)
- [x] **Docs updated** — N/A: no change to the system's shape
- [x] **API types regenerated** — N/A: only SQL strings and comments inside
      function bodies changed, no DTO or doc comment

## Implementation Notes

- `crates/api/src/contracts/queries.rs`: `fetch_contract` and
  `fetch_wasm_interface` join
  `(SELECT wasm_hash, metadata FROM wasm_interface_metadata FINAL) wim`
  instead of the bare table. `sc.contract_id` gets `AS contract_id` in both.
  Pitfall 1 of the comment is rewritten (real engine, why the subquery, where
  the old `ILLEGAL_FINAL` came from); pitfalls 2 and 3 kept, 2 extended to the
  new alias; stale test path `queries_ch_tests.rs` → `queries/ch_tests.rs`.
- `crates/db-clickhouse/schema/init.sql`: comment above
  `wasm_interface_metadata` no longer says reads stay FINAL-free (comment only).
- `crates/api/src/contracts/queries/ch_tests.rs`: new
  `contract_reads_dedup_wasm_metadata_written_twice` — own throwaway DB,
  `SYSTEM STOP MERGES` on the table, two INSERTs of one hash (asserts 2 active
  parts, 2 physical rows), then both real query functions under
  `enable_analyzer` 1 and 0; expects the second write (`upgradeable = true`,
  first function `second_write`).
- Proof on local docker CH 26.3.21.7: pre-fix `fetch_contract` join →
  `enable_analyzer=0: ... left: None, right: Some(true)`; pre-fix interface
  join only → `left: Some("first_write"), right: Some("second_write")`; fixed
  SQL → both tests `ok`.

## Issues Encountered

- **The pre-fix SQL already dedups under the new analyzer.** On CH 26.3 with
  `enable_analyzer=1` (the default; no profile in `users.d` overrides it),
  `FROM soroban_contracts sc FINAL LEFT JOIN wasm_interface_metadata wim`
  returns one row per hash: the analyzer carries the leading table's `FINAL`
  over to a joined Replacing table (it skips a plain MergeTree silently).
  Stayed true with join swap, runtime filters and lazy materialisation each
  disabled, and with `partial_merge` / `full_sorting_merge` joins. So the
  defect is not live on production today; it is a dependency on analyzer
  behaviour. This is also what dropped task 0332 in 2026-07 ("effectively
  FINAL via sc-FINAL propagation"). The old analyzer does not propagate, so
  the test runs both; only the `=0` run fails on the pre-fix SQL.
- **`wim FINAL` in the join is not enough.** Under `enable_analyzer=0` FINAL
  on a joined table is ignored (both copies came back); under `=1` it only
  repeats what propagation already does. The subquery dedups under both.
- **Old analyzer and unaliased columns.** Under `enable_analyzer=0` the
  unaliased `sc.contract_id` comes back named `sc.contract_id` and the row
  deserialiser fails (same mechanism as pitfall 2). Aliased in both queries.
- **Root cause of the 0327 `ILLEGAL_FINAL` note.** `init.sql` switched the
  table to RMT on 2026-06-22 (c315721d0); production was swapped 2026-06-24
  (task 0310). The 0327 fix (d258c93bf, 2026-06-26) was "verified locally" —
  against a ClickHouse whose table predated the switch, because
  `CREATE TABLE IF NOT EXISTS` never changes an existing engine. Reproduced:
  explicit `FINAL` on a plain MergeTree join → Code 181; on the RMT → fine.
- **Cost.** Synthetic 5k × 20 KB table, `cityHash64(wim.metadata)` read:
  pre-fix, `wim FINAL` and the FINAL subquery all read 10 003 rows /
  95.73 MiB. The subquery adds no read cost. (A first benchmark with
  `length(metadata)` read only the size subcolumn and suggested otherwise.)
- **Unrelated local failures.** With `CH_URL` set, 4 `decode_smoke` tests in
  `liquidity_pools` / `search` fail on the empty local DB (they expect real
  pool rows); they fail the same way without this change.

## Design Decisions

### From Plan

1. **Dedup with FINAL, not argMax**: no version column, and "last inserted"
   is what both the merge and FINAL keep.

### Emerged

2. **Subquery instead of `wim FINAL`**: the only form that dedups under both
   analyzers; same bytes read (measured).
3. **Test runs under `enable_analyzer` 1 and 0**: under the default analyzer
   the pre-fix SQL passes, so a default-only test could not fail before the
   fix.
4. **`sc.contract_id AS contract_id`** in both queries: required for the
   `=0` run to deserialise at all.
5. **`init.sql` comment corrected** (comment only): it said reads stay
   FINAL-free, which this change made false.

## Future Work (findings, not filed)

- Other readers of the table without dedup: `db-clickhouse/src/persist.rs`
  (`SELECT … FROM wasm_interface_metadata WHERE … IN (…)` into a HashMap) and
  `backfill-runner/src/contract_type_rebuild.rs` (`SELECT wasm_hash, metadata
FROM wasm_interface_metadata`). A duplicate there gives last-read-wins by
  iteration order; matters only if two copies classify differently.
- `db-clickhouse/src/persist/rows.rs:55` still calls the table "MergeTree".
