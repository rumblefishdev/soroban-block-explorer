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
   `transactions` (no S3), coverage gate. Since task 0580 (2026-09-24,
   decision karolkow) as a **parallel change** — new table under a new name,
   dual write, fill, readers, stop the old write, drop — four ordinary
   deploys and no window (`docs/deployment.md`); 0575's `EXCHANGE TABLES`
   window stopped ingest for 51 minutes.
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

- [ ] ADR adopted; `application_order` means one thing — ADR 0059 accepted and
      the schema agrees; three API operation DTOs still use it for the operation
      (closing checks in the note)
- [x] Partition-level measurement and join benchmark recorded before step 4
      — 0541 (events: first page 285 → 119 ms on XLM) and 0575 step 1 (trial
      partition + account / asset list benchmark, every list < 1 s, < 1 GiB)
- [x] No table carries `transaction_id`; `transactions.id` dropped (2026-09-29)
- [x] No new table can add `transaction_id`: `crates/db-clickhouse/tests/schema_conventions.rs`
      (allowlist of the 8 tables that still carry it; each migrated table
      removes its entry — the test fails on a stale one) — 2026-09-23
- [x] `nft_ownership` carries the canonical event location; its per-token
      `event_order` is no longer used as event identity
- [x] Every list returns rows in execution order inside a ledger — verified on
      ledger 64 454 000 for contract `CAS3J7GY…` and on one account, one asset
- [x] Event ids on the wire match stellar-rpc `getEvents` (sampled)
- [x] Database size re-measured after each table; saving reported per table
- [x] **Docs updated** — `docs/architecture/database-schema/**`, API data
      contracts; **API types regenerated** where cursors change

---

## Research acceptance criteria

- [x] Per-table verdict: migrate / leave / new-tables-only, each with its
      measured saving and its measured read-path cost
- [x] Two-column join benchmarked on the hot tx-list endpoints against today's
      single-column join
- [x] Duplicate-hash question settled: what `transaction_hash_index` is for and
      whether a narrower structure serves it — task 0580: an 8-byte prefix
      index replaced it, −124.8 GiB net (2026-09-24)
- [x] Log TTL quantified and handed over as a standalone config change (task 0563)
- [x] Recommendation written as an ADR if a schema-wide convention is adopted (ADR 0059)
      (identity columns use the natural key; surrogates only where measured)

The original research, every dated measurement, the per-step records and the
2026-09-30 closing checks are in
[notes/R-research-and-progress-log.md](notes/R-research-and-progress-log.md).
