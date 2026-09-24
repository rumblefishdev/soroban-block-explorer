---
id: '0580'
title: 'REFACTOR: key transaction_hash_index by an 8-byte hash prefix — the full hash is checked in transactions'
type: REFACTOR
status: done
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
  - date: 2026-09-24
    status: done
    who: karolkow
    note: >
      Shipped as a parallel change in four deploys, no swap window: #491
      (search by inner hash), #492 (new table + dual write, history filled
      and gated), #493 (readers), #497 (write stopped); the old index was
      dropped on production. Freed 175.3 GiB of disk; the new table holds
      50.33 GiB, a net saving of 124.8 GiB against the ~120 GiB estimate.
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
- [x] `transaction_hash_dict` gone (task 0396, #488; `DROP DICTIONARY` on production)
- [x] Search and the transaction page find a transaction by outer and by inner
      hash on production after the switch (step 2 below)
- [x] Index re-measured; saving reported (step 3 below: −124.8 GiB net)
- [x] stellar-prices-api check recorded before the change: not a reader
      (grants and `query_log`, Context above)
- [x] **Docs updated** — schema overview, pilot, backend overview, endpoint
      queries 03 and 22 and README, endpoint runner, SCF demo query,
      `docs/backfills.md`, `docs/deployment.md`, runbooks (#492, #493, #497)

## Progress

_Superseded by the parallel change below: #488 merged as task 0396, #489
closed; the plan's window never happened._

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
4. **Parallel change instead of a swap window** (decision karolkow,
   2026-09-24) — see below; now the rule in `docs/deployment.md`.
5. **Search reuses the transaction page's `lookup_hash_ledgers`** (#493
   review) — one query to change, not two.
6. **Rebuild recipe from `transactions`** in `docs/backfills.md`: the prefix
   index is derived from `transactions` alone; checked against production,
   545,555 = 545,555 keys on 1,000 ledgers.
7. **Dead `domain::TransactionHashIndex` removed** (#497, gardening).

## Replanned as a parallel change (2026-09-24)

**Decided (karolkow):** no swap window. The re-key ships as four ordinary
deploys — a new table under a new name, written beside the old one — after a
devil's-advocate pass found the design sound and the window the risk (task
0575's ingest stood 51 minutes). PR #489 (swap under the same name) closed as
superseded; its code carried over. The rule is now in `docs/deployment.md`
for every later table change.

| PR  | scope                                                             | state                                                                              |
| --- | ----------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| A   | search finds a fee-bump by its inner hash (+ the moves it needed) | [#491](https://github.com/rumblefishdev/soroban-block-explorer/pull/491), merged   |
| B   | `transaction_hash_prefix_index` + the indexer writes both         | [#492](https://github.com/rumblefishdev/soroban-block-explorer/pull/492), deployed |
| C   | both readers on the new table                                     | [#493](https://github.com/rumblefishdev/soroban-block-explorer/pull/493), deployed |
| D   | stop writing the old index, drop it                               | [#497](https://github.com/rumblefishdev/soroban-block-explorer/pull/497), deployed |

**Projections checked instead of a table (2026-09-24, local ClickHouse
26.3):** they do run on a ReplacingMergeTree with
`deduplicate_merge_projection_mode = 'rebuild'` (task 0395 had this already;
the "refused" claim was the default), and one keyed by the prefix is used by
the planner (8,192 of 20 M rows read). But a projection stores its source
column: an expression projection on the prefix kept the full hash (36.19
B/row), a `_part_offset` one 37.24 plus 25.06 for the inner hash; the leanest,
a `MATERIALIZED` prefix column plus a projection, comes to ~104 GiB
(_estimate_) against ~49 GiB for the table, and needs a rewrite of all of
`transactions`. The table stays.

## Step 1 deployed, history fill (2026-09-24)

- **#492 merged and deployed** (Compute 09:33:48 UTC). The first ledger the
  indexer dual-wrote is **64,591,331**; on 64,591,331–64,591,349 the old and
  new index hold the same keys, 9,075 = 7,129 transactions + 1,946 fee-bump
  inner hashes. Queue, DLQ and indexer errors 0.
- **Fill:** partitions 100–125 copied and gated per slice (11:41–12:21 local),
  then the `dev_read` hourly read quota (4 TiB) ran out on the gate of
  slice 63,350,000. Resume: `126:63350000 127 128`, then
  `64500000-64591331`.
- **Why the quota went — a lesson for any fill:** the loop slices by ledger,
  but `transaction_hash_index` is sorted by hash, so a ledger-range filter
  cannot use its key and every statement reads the whole partition — 10 fill
  and 40 gate statements per partition, ~160 GB each instead of the ~20
  estimated. The same loop was cheap in task 0575 because those tables lead
  with the ledger. **Slice along the source table's sort key.** Correctness
  was not affected; only reading cost.
- **Fill complete** (11:17 UTC): `126:63350000 127 128` and
  `64500000-64591331` all gated ok. `system.parts`, active rows per
  partition, old against new: equal in every partition 100–129 except 126
  (+23,791,762 in the new table — slice 63,350,000 was inserted before the
  quota stopped its gate and inserted again on resume; ReplacingMergeTree
  collapses the copies on merge). Size: old 175.14 GiB, new **50.32 GiB**.
- **Lookup, new against old** (production, `X-ClickHouse-Summary`):

  | hash                     | old read               | new read              | ledger |
  | ------------------------ | ---------------------- | --------------------- | ------ |
  | outer, ledger 64,000,000 | 197,409 rows / 6.38 MB | 16,886 rows / 0.20 MB | same   |
  | inner, ledger 57,000,000 | 8,694 rows / 0.29 MB   | 8,694 rows / 0.10 MB  | same   |

  Both ~5–7 ms, dominated by round trip. The inner hash of the 64,000,000
  fee-bump and the outer hash at 57,000,000 also resolve to the same ledger
  in both tables.

## Step 2 deployed, readers on the prefix index (2026-09-24)

- **#493 merged and deployed** (Compute, API Lambda 12:22:11 UTC, with #495;
  the indexer unchanged, still writing both tables). Checked through the
  deployed API (Vite dev proxy, dev API key): search and the transaction
  page find a fee-bump at the head (64,593,473) by outer and by inner hash,
  the same at 64,000,000 and the oldest filled range (50,600,000); an
  absent hash sharing no row answers 404 / no hit.
- `system.query_log`, `api_reader` since the deploy: 12 reads of
  `transaction_hash_prefix_index`, **0 of `transaction_hash_index`**, 0
  exceptions.
- Review of #493 before merge (standards + spec) found the branch without
  step 1's table in `init.sql` (merged develop in), stale docs, canonical
  SQL 03 matching the outer hash only, and search carrying its own copy of
  the lookup — all fixed in the PR. Collision behaviour checked on a local
  ClickHouse: two hashes sharing 8 bytes in two ledgers each resolve to
  their own transaction.

## Step 3 deployed, old index dropped (2026-09-24)

- **#497 merged and deployed** (indexer Lambda 13:25:10 UTC). `query_log`:
  the last write to `transaction_hash_index` at 13:25:10, none after;
  the prefix index kept up with the head, 0 ingest exceptions.
- **Dropped by the operator:** `DROP TABLE transaction_hash_index SETTINGS
max_table_size_to_drop = 0` (a table over 50 GB needs the override).
  ClickHouse deletes the files after `database_atomic_delay_before_drop_table_sec`
  (480 s).
- **Measured:** free disk 613.28 → **788.57 GiB (+175.3 GiB)**; the prefix
  index 50.33 GiB; net saving **124.8 GiB** (estimate ~120). Active data in
  `default`: 703.6 GiB, against 976.4 GiB in the 2026-09-23 survey (with task
  0575).

## Issues Encountered

- **Read quota during the fill.** Slicing a hash-sorted source by ledger
  read the whole partition per statement; the `dev_read` 4 TiB/h quota ran
  out at partition 126. Resumed after the reset; lesson in step 1 above.
- **One slice inserted twice** (quota stopped the gate after the insert);
  harmless — ReplacingMergeTree collapses it, readers use `DISTINCT`.
- **Docs gave a bare `DROP TABLE`**, which the server refuses above 50 GB;
  corrected in `docs/deployment.md` as a general rule of the parallel change.
- **Turnstile blocks post-deploy checks in the browser**; the deployed API
  was checked through the Vite dev proxy and its dev API key instead.

**Modified tests:** `init_sql_parses_into_statements` count 39 → 40 (#492)
and 42 → 41 (#497), intentional; the fee-bump staging test now asserts
prefix rows, with an inner hash that differs in its first 8 bytes (#497);
`decode_smoke` asserts a hash finds its transaction, verified red with the
prefix index empty (#493).
