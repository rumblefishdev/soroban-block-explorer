---
id: '0538'
title: 'EPIC: locate every transaction, operation and event by its canonical position — replace the surrogate transaction id project-wide'
type: EPIC
status: backlog
related_adr: []
related_tasks: ['0393', '0417', '0541']
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
7. **`transactions.id`** dropped once nothing joins on it.
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
   worth confirming separately because it needs no code at all.
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
