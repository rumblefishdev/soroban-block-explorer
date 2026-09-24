---
id: '0538'
title: 'EPIC: locate every transaction, operation and event by its canonical position — replace the surrogate transaction id project-wide'
type: EPIC
status: active
related_adr: ['0059']
related_tasks: ['0393', '0417', '0541', '0558', '0575']
tags:
  [
    'clickhouse',
    'storage',
    'performance',
    'epic',
    'phase-future',
    'effort-large',
    'priority-high',
  ]
links:
  - crates/db-clickhouse/schema/init.sql
history:
  - date: 2026-09-03
    status: backlog
    who: karolkow
    note: >
      Filed after a measurement made while shipping the net-settled column
      (0411/0393). The surrogate `transaction_id` was found to occupy ~267 GiB
      across seven tables at a compression ratio of 1.0, and the 32-byte
      transaction hash is stored twice for a further ~273 GiB. Together that is
      ~45% of a 1.16 TiB database spent on identity alone. The natural key
      `(ledger_sequence, application_order)` was verified unique and costs
      0.135 B/row against 8.03 B/row for the surrogate. Filed as RESEARCH, not
      REFACTOR: the saving is real but every candidate touches a sort key, so
      the question to answer first is which subset is worth the rewrite.
  - date: 2026-09-16
    status: backlog
    who: karolkow
    note: >
      Re-measured. `transaction_id` columns now 270.10 GiB = 26% of a 1.01 TiB
      database; free space 368.72 GiB. First concrete table decided: task 0541
      keys `soroban_events` by the canonical event location, which drops its
      50.48 GiB `transaction_id` for a reason beyond storage — event order and
      rpc-comparable ids.
  - date: 2026-09-16
    status: backlog
    who: karolkow
    note: >
      Turned from research into the programme (decision 209 A): the complete
      move of every table from the surrogate `transaction_id` to the canonical
      location, with 0541 as its first table. Scope widened by a second defect
      found while checking reads — lists order rows inside a ledger by the
      surrogate, i.e. by hash, not by execution order.
  - date: 2026-09-23
    status: active
    who: karolkow
    note: >
      Promoted. Steps 1 and 3 are done: ADR 0059 names the positions, and task
      0541 keyed `soroban_events` by the rpc event id (−242 GiB with the drops).
      Step 5 starts with task 0575 (`transaction_participants` +
      `operation_asset_appearances`, 170.13 GiB of `transaction_id`). The
      target shape is measured on production as `contract_transactions`:
      `application_order` behind a leading id costs 1.31 B/row against 8.03.
      Re-measured below.
  - date: 2026-09-23
    status: active
    who: karolkow
    note: >
      The rule was written only as one line of ADR 0059, while the `init.sql`
      header still presented `transactions.id` as the FK hub and ADR 0056
      rule 4 read as "new tables key on surrogates". Guard added (decision
      karolkow, 2026-09-23, option B): `tests/schema_conventions.rs` fails on
      a `transaction_id` column outside a shrinking allowlist; `init.sql`
      header, `CLAUDE.md` and ADR 0056 rule 4 now say the same thing.
---

# EPIC: canonical location for transactions, operations and events

## Programme (decided karolkow, 2026-09-16)

**Target:** one way to locate rows everywhere — the position Stellar itself
uses: transaction = `(ledger_sequence, application_order)` (1-based, the order
applied), operation = its index in the transaction (0-based), event = its
position within the operation (0-based), with stellar-rpc's sentinels for fee
events (task 0541 has the source-read definition). The surrogate
`transaction_id` (hash64 of the hash) disappears from every table; lookups by
hash keep using `transaction_hash_index`.

**Two defects, one cause:**

1. **Storage.** `transaction_id` columns are 270.10 GiB = 26% of the database,
   plus `transactions.id` 31.32 GiB (measured 2026-09-16, table below).
2. **Order.** Lists order rows inside one ledger by the surrogate — by hash,
   not by execution. Measured on ledger 64 454 000 for contract `CAS3J7GY…`:
   transactions at positions 195, 231, 9, 211, 221, 139 are listed in that
   order. Found by code reading in 11 list queries across 6 API modules:
   `transactions` (4 — contract- and operation-type-filtered lists),
   `assets` (2), `contracts` (2 — invocations, events), `accounts` (1),
   `liquidity_pools` (1), `nfts` (1). Every one pages on
   `(ledger_sequence, transaction_id)`.

**Steps, in order — each table its own deploy window:**

1. **ADR** — the convention: canonical location as identity and sort key;
   surrogates only where a measurement justifies one. Settle the name clash
   (`application_order` is the transaction position in `transactions` /
   `asset_transfers` / `soroban_event_ops`, the operation position in
   `operations_appearances` / `lp_operation_amounts`).
2. **Measure before migrating** (the research below): rebuild ONE partition of
   one presence table with the new key and measure its real size (position
   behind a leading `account_id` / `asset_id` compresses worse), and benchmark
   the two-column join on the hot list endpoints against today's.
3. **`soroban_events`** — task 0541 (decided): canonical key, rpc-format ids,
   `soroban_event_ops` dropped.
4. **NFT ownership location** — replace `nft_ownership.event_order` (a local
   ordinal per `(collection, token, ledger)`, so distinct pieces in one bulk
   move all commonly carry `0`) with the canonical transaction / operation /
   event position. That gives `asset_transfers`, `soroban_events` and
   `nft_ownership` one exact join key and removes the need to infer a sender's
   pieces from the previous-owner timeline.
5. **Presence tables by saving**: `operation_asset_appearances` (87.68 GiB),
   `transaction_participants` (81.46), `operations_appearances` (33.00),
   `soroban_invocations_appearances` (8.30), `operation_pools` (4.76),
   `lp_operation_amounts` (4.43). Each: new table, fill from the old one +
   `transactions` (no S3), coverage gate, `EXCHANGE TABLES` with the indexer
   stopped (`docs/backfills.md`), readers switched in the same window.
6. **Readers**: every list pages on the canonical position — execution order
   inside a ledger, cursor `(ledger_sequence, application_order[, op, event])`.
7. **`transactions.id`** dropped once nothing joins on it. The indexer joins
   by hash in memory too: eight extracted types carry a `transaction_hash`
   string, and staging resolves them through maps keyed by it
   (`persist/stage.rs` `tx_id_by_hash` / `app_order_by_hash`,
   `persist/value_flow.rs` `tx_by_hash` / `ops_by_hash`). Those joins move to
   the position with the tables. An event already carries it — since 0541 its
   rpc id holds the transaction's `application_order`, and a transfer comes
   only from an operation event (0541 review, 2026-09-21).
8. **Duplicate hash** (`transactions.hash` + `transaction_hash_index.hash`,
   ~275 GiB) — decided from the research question below, not assumed.

**Constraints:** free space 368.72 GiB of 1.72 TiB with backups on the same
volume — tables are rebuilt one at a time, largest last or after a cleanup;
never two copies of two tables at once. Every struct change ships with
`DEFAULT` and the DDL-before-writer order (ingest froze twice in 0548).

## Acceptance Criteria (programme)

- [ ] ADR adopted; `application_order` means one thing
- [ ] Partition-level measurement and join benchmark recorded before step 4
- [ ] No table carries `transaction_id`; `transactions.id` dropped
- [x] No new table can add `transaction_id`: `crates/db-clickhouse/tests/schema_conventions.rs`
      (allowlist of the 8 tables that still carry it; each migrated table
      removes its entry — the test fails on a stale one) — 2026-09-23
- [ ] `nft_ownership` carries the canonical event location; its per-token
      `event_order` is no longer used as event identity
- [ ] Every list returns rows in execution order inside a ledger — verified on
      ledger 64 454 000 for contract `CAS3J7GY…` and on one account, one asset
- [ ] Event ids on the wire match stellar-rpc `getEvents` (sampled)
- [ ] Database size re-measured after each table; saving reported per table
- [ ] **Docs updated** — `docs/architecture/database-schema/**`, API data
      contracts; **API types regenerated** where cursors change

---

# RESEARCH: identity columns that do not compress (original research, kept)

## Summary

One class of column dominates this database and carries no information beyond
identity: **hash-derived identifiers**. They are indistinguishable from random
noise, so they compress at ratio 1.0 — every row pays the full 8 or 32 bytes.
Measured on production, they account for roughly **45% of the 1.16 TiB**.

Stellar gives every transaction a natural, dense key —
`(ledger_sequence, application_order)` — that is verifiably unique and ~59×
cheaper. This task measures which tables would actually benefit from switching,
what the migration costs, and whether the same reasoning applies to other
columns.

## Context

The `net_settled` column (0393, shipped to the frontend in 0411) needed a
per-row cost model, which exposed the pattern by accident:

- adding the value column itself cost **292 MiB**;
- storing the same value _per account_ would have cost ~8 B/row **just to
  repeat the transaction id**, dwarfing the payload.

That prompted a database-wide measurement, and the surrogate turned out to be
the single largest structural cost in the schema.

## Measurements (production, 2026-09-03)

Full per-column figures in [notes/R-column-costs.md](notes/R-column-costs.md).
Headline numbers, all **measured**, not estimated:

| Column                                 | Size          | Ratio | B/row     |
| -------------------------------------- | ------------- | ----- | --------- |
| `transaction_id` × 7 tables            | **267 GiB**   | 1.0   | 8.03      |
| `transaction_hash_index.hash`          | **149.8 GiB** | 1.0   | 32.13     |
| `transactions.hash`                    | **123.4 GiB** | 1.0   | 32.13     |
| `transactions.id`                      | 30.9 GiB      | 1.0   | 8.03      |
| `ledger_sequence` (leading a sort key) | 0.25 GiB      | 130.5 | **0.061** |
| `application_order`                    | 0.31 GiB      | 26.9  | **0.074** |
| `account_id` (leading a sort key)      | 0.99 GiB      | 86.6  | **0.092** |

Two facts follow directly:

1. **Position in the sort key decides compression, not the column.**
   `ledger_sequence` costs 0.061 B/row when it leads the key and **2.82 B/row**
   when it sits second behind `account_id` — a 46× swing for the same data.
   Any redesign trades one column's compression for another's.
2. **The natural key is unique.** Verified on a 322,240-transaction sample:
   322,240 distinct `(ledger_sequence, application_order)` pairs, zero
   collisions. `application_order` maxes at 100, so it fits in two bytes.
   `ledger_sequence` is already present in every candidate table because it is
   the partition key — so the marginal cost is one narrow column, not two.

## Open questions this task must answer

1. **Which tables actually pay off?** The surrogate compresses at 1.0 in
   `operation_asset_appearances` and `transaction_participants`, but at ~1.57 in
   `soroban_events` and `operations_appearances`, where it repeats within a
   sort key. The saving is not uniform and the migration cost is.
2. **What does the read path lose?** Every join moves from one column to a
   pair. Measure whether two-column joins on the hot list endpoints stay within
   budget, or whether the CPU cost eats the storage win.
3. **Is the hash genuinely stored twice?** `transactions.hash` and
   `transaction_hash_index.hash` total ~273 GiB. Confirm the index is a
   deliberate reverse lookup and establish whether a cheaper structure (or a
   prefix) serves the same query.
4. **What else is identity-shaped?** `inner_tx_hash` (28.1 GiB),
   `source_id`/`destination_id` (23.5 + 21.4 GiB) — do they share the pattern,
   and does `LowCardinality` or a dictionary help where the value repeats?
5. **Non-schema quick wins.** ClickHouse's own `text_log.message` (24.5 GiB)
   and `query_log.ProfileEvents` (8.1 GiB) are server-side logs. A TTL is a
   configuration change, not a migration — the cheapest ~32 GiB available, and
   worth confirming separately because it needs no code at all. **Taken by task
   0563** (2026-09-17): the `system` database is 174.50 GiB, ≥ 89 GiB older than
   30 days; a config TTL alone renames the tables instead of trimming them, so
   the TTL goes on the live tables first.
6. **What is the migration actually worth?** Every candidate sits in a sort
   key, so this is a full rebuild per table plus a re-ingest, not an `ALTER`.
   Quantify against the alternative of applying the natural key **only to new
   tables** (the per-account delta table under discussion is the first
   candidate) and leaving the existing stock alone.

## Constraints

- **Free space is the live constraint.** Production has 458.87 GiB free of
  1.72 TiB, and `/backups/` shares that volume — every GiB counts twice.
- The engine is version-less `ReplacingMergeTree`; a sort-key change cannot be
  `ALTER`ed and forces a rebuild (the same wall 0393 hit; see its notes).
- No migration lands without a read-path benchmark first — the 0243/0386 quota
  outages were both read-shape regressions.

## Acceptance Criteria

- [ ] Per-table verdict: migrate / leave / new-tables-only, each with its
      measured saving and its measured read-path cost
- [ ] Two-column join benchmarked on the hot tx-list endpoints against today's
      single-column join
- [ ] Duplicate-hash question settled: what `transaction_hash_index` is for and
      whether a narrower structure serves it
- [ ] Log TTL quantified and handed over as a standalone config change
- [ ] Recommendation written as an ADR if a schema-wide convention is adopted
      (identity columns use the natural key; surrogates only where measured)

## Re-measured 2026-09-16 (production, `system.columns` / `system.parts`)

Database 1.01 TiB compressed; free 368.72 GiB of 1.72 TiB (was 458.87 GiB on
2026-09-03; `/backups` shares the volume).

| Table                             | `transaction_id` | B/row | Table total | Share             |
| --------------------------------- | ---------------- | ----- | ----------- | ----------------- |
| `operation_asset_appearances`     | 87.68 GiB        | 8.03  | 100.29 GiB  | 87%               |
| `transaction_participants`        | 81.46 GiB        | 8.03  | 111.06 GiB  | 73%               |
| `soroban_events`                  | 50.48 GiB        | 5.12  | 235.95 GiB  | 21%               |
| `operations_appearances`          | 33.00 GiB        | 5.09  | 101.86 GiB  | 32%               |
| `soroban_invocations_appearances` | 8.30 GiB         | 8.03  | 14.47 GiB   | 57%               |
| `operation_pools`                 | 4.76 GiB         | 8.03  | 6.93 GiB    | 69%               |
| `lp_operation_amounts`            | 4.43 GiB         | 4.87  | 11.52 GiB   | 38%               |
| **all**                           | **270.10 GiB**   |       |             | **26% of the DB** |

Plus `transactions.id` 31.32 GiB (8.03 B/row) and `transactions.hash`
125.26 GiB.

What the natural location costs where it already exists (sorted after the
ledger): `application_order` 0.23 B/row in `asset_transfers` and
`soroban_event_ops`, 0.074 in `transactions`; `op_index` / `event_pos_in_op`
0.12. Behind a leading `account_id` / `asset_id` it will compress worse
(fact 1 above) — the per-table estimate still has to be measured on one
rebuilt partition, not extrapolated.

The location is also the canonical identity Stellar uses (task 0541: rpc's
event id is `TOID(ledger, tx application order, op) + event in op`), so the
natural key is not only cheaper but the one outside tools speak.

**First table:** `soroban_events`, via task 0541 (decided 2026-09-16).

## Re-measured 2026-09-23 (production, after 0541)

Database 1011.88 GiB / 62.10 bn rows; free 482.58 GiB of 1.72 TiB.

| Table                             | `transaction_id` | B/row | rows     |
| --------------------------------- | ---------------- | ----- | -------- |
| `operation_asset_appearances`     | 88.06 GiB        | 8.03  | 11.77 bn |
| `transaction_participants`        | 82.07 GiB        | 8.03  | 10.97 bn |
| `operations_appearances`          | 33.24 GiB        | 5.08  | 7.02 bn  |
| `soroban_invocations_appearances` | 8.41 GiB         | 8.03  | 1.13 bn  |
| `operation_pools`                 | 4.77 GiB         | 8.03  | 0.64 bn  |
| `lp_operation_amounts`            | 4.45 GiB         | 4.82  | 0.99 bn  |
| **all**                           | **220.99 GiB**   |       |          |

Plus `transactions.id` 31.54 GiB; `transactions.hash` 126.13 GiB,
`transaction_hash_index.hash` 153.73 GiB (5.14 bn rows = 4.22 bn transactions

- 0.92 bn fee-bump inner hashes, so the index is not a plain copy).

**Step 2 answered by a table that already exists.** `contract_transactions`
(0541) has exactly the presence shape — `(contract_id, ledger_sequence,
application_order)` — on 2.99 bn rows: `application_order` 1.31 B/row (ratio
1.5), `ledger_sequence` 1.97 B/row. Its 203 parts are unmerged, so both are
upper bounds. Projected per table at 1.31 B/row (_estimate_):
`operation_asset_appearances` −74 GiB, `transaction_participants` −69 GiB,
`soroban_invocations_appearances` −7 GiB, `operation_pools` −4 GiB;
`operations_appearances` (position right after the ledger, ~0.23 B/row) −32 GiB.
The per-table trial partition stays, now to choose a codec for
`ledger_sequence` (2.83 B/row behind `account_id`), not to settle whether the
position pays.

**Next table:** `transaction_participants` + `operation_asset_appearances`,
task 0575 (decided karolkow, 2026-09-23).

**Trial partition, 0575 (2026-09-23).** Partition 128 of both tables rebuilt
locally, row counts equal to production: the position alone takes the row
from 11.19 / 9.09 B to 4.72 / 2.44 B; with `Delta, ZSTD(1)` on
`ledger_sequence` and `T64, ZSTD(1)` on `application_order` to 1.95 / 1.12 B.
Codecs matter as much as the position once the hash is gone — every later
table in this programme should try the same two.

**`soroban_invocations_appearances` — measured 2026-09-23, left for a later
step (decision karolkow, option C).** Its presence is fully covered by
`contract_transactions` (0 invocations missing on 1,000 ledgers), but
"invoked" is narrower than "touched" — for a SAC only 24% of
`contract_transactions` rows are invocations, so the Invocations tab and the
invocation counts cannot read `contract_transactions` as is. Of its 14.47 GiB:
`transaction_id` 8.42, `caller_id` 4.61 (the only value nothing else holds —
the Invocations tab's caller and "Unique callers"; the first invocation's
caller only), `caller_contract_id` 0.62 and `amount` 0.58 **read by nothing**
(`crates/`, `web/`), key columns 0.49. The transaction page's
`soroban_invocations` field is served but the frontend does not read it.
Options when this step comes: rekey to the position and drop the two dead
columns (~9.6 GiB, _estimate_), or fold an `invoked` flag and `caller_id` into
`contract_transactions` and drop the table (~10 GiB, but the table stops being
pure presence and the SAC Invocations tab reads ~4× the rows). Task 0575
already stopped reading it for the asset list.

## Step 5a done — task 0575 (2026-09-23)

`transaction_participants` and `operation_asset_appearances` keyed by
`(ledger_sequence, application_order)` with `Delta` / `T64` codecs, swapped in
one window (ingest paused 13:02–13:53 UTC) and the old tables dropped the same
day: 112.04 → 20.12 GiB and 100.76 → 12.39 GiB, +211.7 GiB free on the
volume. The account and asset lists now page in execution order inside a
ledger (checked on ledger 64,454,000 for one account and native XLM).
[Task 0575](../../archive/0575_REFACTOR_presence-tables-canonical-position/README.md).

`transaction_id` left on production after it (`system.parts_columns`):
`operations_appearances` 33.26 GiB, `soroban_invocations_appearances` 8.42,
`operation_pools` 4.78, `lp_operation_amounts` 4.46, `nft_ownership` +
`_pending` ~0; plus `transactions.id` 31.57 GiB.

**Before every later window:** check whether stellar-prices-api reads the
table — its users' grants (`users.d/services.xml`) and the users that queried
it in `system.query_log` (decision karolkow, 2026-09-23). For 0575: none.

## Database size — the whole list (decided karolkow, 2026-09-23)

This epic is also the umbrella for every schema-level space saving, not only
the transaction location (the same tables and the same windows). Survey:
[notes/R-whole-database-survey-2026-09-23.md](notes/R-whole-database-survey-2026-09-23.md).
Estimates until a trial measures them.

| candidate                                                                                                                   | saving     | window                                      | where                                       |
| --------------------------------------------------------------------------------------------------------------------------- | ---------- | ------------------------------------------- | ------------------------------------------- |
| `transaction_hash_index` keyed by an 8-byte hash prefix; the full hash checked in `transactions`                            | ~120 GiB   | yes                                         | step 8; task 0396 (`transaction_hash_dict`) |
| `operations_appearances` by position, `pool_ids` dropped, codecs                                                            | ~40–50 GiB | yes                                         | step 5; task 0372                           |
| `transactions.id` dropped                                                                                                   | 31.57 GiB  | yes                                         | step 7                                      |
| `soroban_invocations_appearances` folded into `contract_transactions` (below)                                               | ~8–9 GiB   | yes                                         | step 5                                      |
| `operation_pools`, `lp_operation_amounts` by position                                                                       | ~9 GiB     | yes                                         | step 5                                      |
| codecs on integer columns without one (`soroban_events`, `contract_transactions`, `transaction_hash_index.ledger_sequence`) | ~15–25 GiB | no (`MODIFY CODEC`, parts rewrite on merge) | —                                           |
| `transactions.idx_tx_hash_bloom` dropped                                                                                    | 4.93 GiB   | no (`DROP INDEX`)                           | —                                           |
| `soroban_events` payload as raw XDR instead of JSON text                                                                    | unknown    | yes                                         | tasks 0572, 0416; prices-api reads the JSON |
| account hash surrogates → dense ids (~99 GiB of columns)                                                                    | unknown    | everywhere                                  | research only                               |
| prices-api backup tables (`rollout_0286_bak_*`, `price_ohlcv_*_bak`)                                                        | 48.53 GiB  | —                                           | theirs to drop                              |

**`soroban_invocations_appearances` is a subset of `contract_transactions`.**
Same grain — one row per (contract, transaction); the invocation tree is
folded into `amount`. Ledgers 64,000,000–64,010,000: 1,978,709 invocation
pairs, 2,915,824 `contract_transactions` pairs, 0 invocation pairs missing
from it (68% of the pairs are invocations). What only the invocations table
carries: `caller_id`, `caller_contract_id`, the fold count. Its readers: the
contract list's and detail's invocation stats (count, unique callers), the
Invocations tab (driver + caller), the transaction page's invocations.
**Decided (karolkow, 2026-09-23): fold it into `contract_transactions`** —
`caller_id`, `caller_contract_id` and the fold count become columns there
(count 0 = touched by an event or an operation only), and the invocations
table goes. The 937,115 pairs of that range found only in
`contract_transactions` all come from classic transactions (SAC events of
classic payments and trades since protocol 23).

**`transactions.idx_tx_hash_bloom` added nothing — dropped 2026-09-23**
(task 0579). Both reads of
`transactions` by hash (`search/queries.rs:263`, `transactions/queries.rs:729`)
pin `ledger_sequence` first, and one ledger fits one granule. The detail read
matches `hash OR inner_tx_hash`, which is how an inner hash found through
`transaction_hash_index` resolves.
