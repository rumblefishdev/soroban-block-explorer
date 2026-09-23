---
id: '0580'
title: 'REFACTOR: key transaction_hash_index by an 8-byte hash prefix — the full hash is checked in transactions'
type: REFACTOR
status: active
related_adr: ['0059']
related_tasks: ['0538', '0396', '0579']
tags: ['clickhouse', 'storage', 'effort-medium', 'priority-high']
links:
  - crates/db-clickhouse/schema/init.sql
history:
  - date: 2026-09-23
    status: active
    who: karolkow
    note: >
      Batch 2 of the 0538 database-size list, chosen for the best saving per
      day of work (~120 GiB, estimate). Two PRs: the unused
      transaction_hash_dict removed first (task 0396), then the index itself.
---

# Key `transaction_hash_index` by an 8-byte hash prefix

## Summary

`transaction_hash_index` (174.96 GiB, 5.14 bn rows) maps a transaction hash —
outer or fee-bump inner — to its ledger. The 32-byte hash is random, so it
compresses at ratio 1.0: 153.86 GiB of the table. Key the index by the first 8
bytes instead and check the full hash in `transactions`, which the readers
already do. Saves ~120 GiB (_estimate_: 5.14 bn × ~11.5 B ≈ 55 GiB against
175).

## Context

Survey: [0538 notes](../0538_EPIC_canonical-transaction-and-event-location/notes/R-whole-database-survey-2026-09-23.md).
Measured 2026-09-23, read-only:

- **Writer:** the indexer, one insert per ledger; rows for every outer hash and
  every fee-bump inner hash (`stage.rs` "transactions + transaction_hash_index").
- **Readers:** `search/queries.rs` (search by hash) and
  `transactions/queries.rs` `lookup_hash_ledger` (transaction page) — both
  `WHERE hash = unhex(?) LIMIT 1`, then `transactions` in that ledger
  (`hash OR inner_tx_hash` on the page, `hash` in search).
- **Nobody else:** `system.query_log`, 30 days — `ingestion_writer`,
  `api_reader`, `dev_read`, `default`; no stellar-prices-api user.
- **`transaction_hash_dict`** is `NOT_LOADED`, 0 elements: removed first, in
  task 0396 (PR 1).

## Target shape

```sql
CREATE TABLE transaction_hash_index (
    hash_prefix     UInt64,   -- reinterpretAsUInt64(substring(hash, 1, 8))
    ledger_sequence Int64 CODEC(T64, ZSTD(1))
) ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (hash_prefix, ledger_sequence);
```

The reader takes **every** ledger with the prefix, never `LIMIT 1`: two hashes
sharing 8 bytes (~0.7 expected over 5 bn keys) must not turn into a false
"not found". `transactions` then decides by the full hash. The prefix is the
little-endian `u64` of the first 8 bytes on both sides (Rust
`u64::from_le_bytes`, SQL `reinterpretAsUInt64`), pinned by a test against a
real ClickHouse.

## Implementation Plan

1. **Trial** — one partition locally: prefix + ledger, codec variants for
   `ledger_sequence`, B/row; prefix collisions in the partition.
2. **PR 1 (task 0396)** — `transaction_hash_dict` removed; `DROP DICTIONARY`
   on production by the operator. No window.
3. **PR 2** — DDL, `TransactionHashIndexRow`, both readers, tests, docs, fill
   and gate SQL; merged right before the window.
4. **Window** — as 0575: staging filled from the old index in ClickHouse
   (`INSERT … SELECT`, no join), pause, tail, deploy, `EXCHANGE TABLES`,
   resume, checks, drop the old index.

## Acceptance Criteria

- [x] Trial recorded: B/row, codec, collisions — [notes/R-trial-partition-128.md](notes/R-trial-partition-128.md): 36.24 → 10.22 B/row, `T64, ZSTD(1)` on the ledger, 0 collisions in 156.9 M rows
- [ ] `transaction_hash_dict` gone (task 0396)
- [ ] Search and the transaction page find a transaction by outer and by inner
      hash on production after the swap
- [ ] Index re-measured; saving reported
- [ ] stellar-prices-api check recorded before the window (done: not a reader)
- [ ] **Docs updated** — `database-schema/**`, endpoint queries 03 and 22,
      `docs/backfills.md`, `docs/deployment.md`

## Progress

- **PR 1 (task 0396):** [#488](https://github.com/rumblefishdev/soroban-block-explorer/pull/488),
  draft. CI found a statement-count unit test (`init_sql_parses_into_statements`,
  40 → 39) the local run had skipped; fixed in `8b2ffd37`.
- **PR 2:** [#489](https://github.com/rumblefishdev/soroban-block-explorer/pull/489),
  draft, stacked on PR 1; commits `bcef1a4a` (refactor), `1b8ad65d` (re-key),
  `13fab680` (search by inner hash).
  1. `refactor`: `search/queries.rs` inline tests and decode smoke moved to
     `search/queries/{tests,decode_smoke}.rs` (1,332 → 989 lines);
     `lookup_hash_ledger` moved to `transactions/queries/hash_lookup.rs`.
  2. The change: DDL, `TransactionHashIndexRow::new`, both readers take every
     candidate ledger (`lookup_hash_ledgers`; search loops per ledger), tests,
     docs (schema overview §4.3, pilot, canonical SQL 03 / 22 and README,
     endpoint runner, SCF demo query, `backfills.md`, `deployment.md`).
  - Verified: `api` 617 tests, `db-clickhouse` all but the PR 1 count test;
    clippy clean; `api-types:generate` no diff; `persist_e2e` (Rust prefix =
    SQL prefix, bytes `01..08`), `smoke` (two ledgers under one prefix survive
    `OPTIMIZE FINAL`), `g9`, `claimable_balance_holdings` on a fresh
    ClickHouse 26.3.
- **Window runbook:** [`fill_hash_prefix.sql`](notes/fill_hash_prefix.sql),
  [`gate_hash_prefix.sql`](notes/gate_hash_prefix.sql),
  [`fill_hash_prefix.zsh`](notes/fill_hash_prefix.zsh). Loop dry-run with
  stubbed `chw` / `chq` (10 fills, 80 gate queries for `129:64550000` plus a
  range); fill statement and gate old side read-only on production,
  64,000,000–64,012,500: 5,983,289 rows = 5,983,289 distinct keys.

## Design Decisions

### Emerged

1. **The ledger is in the sort key.** Found by a local test: with
   `ORDER BY hash_prefix` alone the ReplacingMergeTree collapses two hashes
   that share a prefix in different ledgers into one row, keeping one ledger —
   the other transaction would answer "not found". `(hash_prefix,
ledger_sequence)` keeps both; two sharing it in one ledger collapse
   harmlessly, the ledger is the answer either way. Pinned by the `smoke` test.
2. **Readers take every candidate, newest ledger first**, and stop at the
   first whose `transactions` row carries the full hash — a loop over what is
   nearly always one ledger, instead of a multi-ledger `IN` query.
3. **Search finds a fee-bump by its inner hash** (decision karolkow,
   2026-09-23, in this PR as its own commit). The index always mapped the
   inner hash and the transaction page matched `hash OR inner_tx_hash`, but
   search checked `t.hash` only, so an inner hash found nothing. Production,
   ledger 64,578,112: the old condition 0 rows, the new 1.
