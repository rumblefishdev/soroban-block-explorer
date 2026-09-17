---
id: '0541'
title: 'FEATURE: locate every Soroban event by its canonical identity — soroban_events keyed by (ledger, tx position, operation, event in operation)'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0453', '0457', '0540', '0182', '0538', '0374']
tags:
  ['clickhouse', 'indexer', 'xdr-parsing', 'effort-medium', 'priority-medium']
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/xdr-parser/src/event.rs
history:
  - date: 2026-09-04
    status: backlog
    who: karolkow
    note: >
      Filed from [[0540]]. `soroban_events` does not store which operation
      emitted an event, although the data exists across the whole ingested
      range — measured, not assumed: 1 265 of 1 265 archive transactions carry
      `TransactionMeta::V4` at protocols 20, 22 and 27, and all 1 770
      non-diagnostic token events carry an operation index. Three tasks already
      pay for the gap. Rides 0540's S3 pass.
  - date: 2026-09-06
    status: backlog
    who: karolkow
    note: >
      Parser (`event_pos_in_op`), `SorobanEventOpRow`, staging
      (`persist/value_flow.rs`), the `soroban_event_ops` DDL and the `--only`
      targeted write all landed on 0540's branch. Left here: the backfill run
      itself (0540 rollout step 6), coverage proof, and retiring 0453's
      read-time decode.
  - date: 2026-09-16
    status: backlog
    who: karolkow
    note: >
      Re-scoped. Stellar's event identity, read from stellar-rpc's source, is
      total for every row `soroban_events` stores, so the schema's two reasons
      for our own flat `event_index` do not hold. Decided (option A): the
      canonical location becomes the sort key of `soroban_events`; the side
      table is its source and is dropped afterwards. The "micro-backend
      removed" criterion was wrong and is replaced. First table of the
      natural-key programme in 0538.
  - date: 2026-09-16
    status: active
    who: karolkow
    note: >
      Promoted. First step: prove `soroban_event_ops` covers every
      non-diagnostic `soroban_events` row, partition by partition, before it
      becomes the source of the new sort key.
---

# soroban_event_ops

## Summary

Store which operation emitted each Soroban event. First as a **narrow side
table** written by 0540's S3 pass (the only additive write available), then
folded into two columns on `soroban_events` itself — see "Target shape" below
(decided 2026-09-07, reversing the "never as a column" stance this task was
filed with).

## Context

`xdr_parser` already computes the operation index: `extract_events` sets
`ExtractedEvent.op_index` from the CAP-67 V4 per-operation container.
`stage.rs` then drops it when building `SorobanEventRow`. The column simply
does not exist in ClickHouse.

Three consumers pay for that today:

| Task               | What it does instead                                                                                                                             |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| **0453**           | Built a read-time "micro-backend": decodes `op_index` from archive XDR on **every** transaction-detail render, exposed as `XdrEventDto.op_index` |
| **0457** (Effects) | Will need the same attribution, for every event and not only token verbs                                                                         |
| **0540**           | Needs an S3 pass rather than a ClickHouse-local transform                                                                                        |

## Why a side table, not a column (original reasoning — superseded by "Target shape")

The obvious fix — `ALTER TABLE soroban_events ADD COLUMN op_index` — looked
wrong here, for the reason [[0540]] hit first:

- `soroban_events` is **10.4 bn rows / 223 GiB**, a version-less
  `ReplacingMergeTree`. Filling a new column means re-inserting **whole rows**,
  which then compete with the existing ones on the same sort key. Which row
  survives a merge is not controlled. This is the documented failure that made
  the 0383 backfill unsafe to re-run after the `net_settled` column landed.
- Rebuilding the table and swapping it (`EXCHANGE TABLES`, the house pattern)
  needs **both copies on disk**: ~446 GiB against 459 GiB free.

A side table avoids both: nothing competes, nothing is rewritten.

```
soroban_event_ops(
    ledger_sequence    Int64,
    application_order  Int16,    -- tx position in the ledger → transactions.id → soroban_events
    event_index        Int16,
    op_index           Int16,    -- envelope position of the emitting operation
    event_pos_in_op    Int16     -- position within that operation's container
)
```

Keyed by the transaction's position, not its id (see "Why now" for the
measurement: the id was 92% of the row). The join to `soroban_events` goes
through `transactions` on `(ledger_sequence, application_order)`, the same hop
`asset_transfers` makes.

Together, `(op_index, event_pos_in_op)` is Stellar's **official** event identity
(TOID plus position within the operation), which 0540 measured as total for
token verbs. It is **not** total for `soroban_events` as a whole — tx-level (fee)
and diagnostic events have no operation — so those rows are simply absent from
this table rather than carrying a null. Absence is the honest encoding: the
question "which operation emitted the fee charge" has no answer.

## Why now

0540's S3 re-parse decodes every event of every ledger anyway. Writing this
table on the same pass costs the extra inserts and nothing else. Done
separately it costs a second ~1-day pass over ~1 TB of XDR.

**Size, measured 2026-09-07** on 39.5 M local events with the final DDL: the
first DDL keyed the table like `soroban_events` (`ledger_sequence,
transaction_id, event_index`) and cost **5.07 B/row — 4.66 of them the
`transaction_id`**, a random hash that does not compress; the two payload
columns cost 0.24. Re-keyed by the transaction's position
(`ledger_sequence, application_order, event_index`, the join going through
`transactions` as `asset_transfers` does) it is **0.63 B/row → ~3.6 GB** on
~5.7 bn rows, instead of ~29 GB. The same two columns inside `soroban_events`
would cost 0.24 B/row (ZSTD) — ~2.5 GB on 10.4 bn rows — which is the target
shape above.

`event_pos_in_op` needed a one-line parser change (the per-operation loop in
`event.rs` did not `enumerate()` the events within the operation) — landed on
0540's branch.

## Target shape — a column on `soroban_events`, the side table as the vehicle (decided 2026-09-07)

The deep review of 0540 asked the principled question: where does the
operation index of an event belong? Canonically **on the event**. stellar-rpc
identifies an event by the cursor `(ledger, tx, op, event)` and, since
Protocol 23, returns the operation index as an attribute of each event in
`getEvents` (per the RPC docs — verify the field name when implementing). A
separate table keyed like `soroban_events` is the right data in the wrong
place; it exists only because filling a column on a 10.4 bn-row table looked
like a rewrite.

It is not. The reasoning above ("re-inserting whole rows", "`EXCHANGE TABLES`
needs both copies") misses ClickHouse mutations: `ALTER TABLE … ADD COLUMN` is
metadata-only and instant, and `ALTER TABLE … UPDATE col = …` rewrites **only
the mutated column's files** — every other column of the part is hard-linked
into the new part. Filling two `Int16` columns over 10.4 bn rows costs one
narrow column-write on the box, no S3, no second copy of the table.

So the side table is kept for 0540's pass — it is what the S3 pass can write
additively today, and it is the **source** for the fold — and the target is:

1. `ALTER TABLE soroban_events ADD COLUMN op_index Nullable(Int16), ADD COLUMN
event_pos_in_op Nullable(Int16)` — instant; NULL = "not yet folded, or a
   tx-level / diagnostic event" (the two must be told apart by the fold's
   completion, not by the value).
2. Live indexer writes both from the deploy on (`SorobanEventRow` gains two
   fields — same deploy-window rule as any struct change, driver validates
   against `DESCRIBE`).
3. History: one mutation **per partition**, sourced from `soroban_event_ops`
   joined to `transactions` on `(ledger_sequence, application_order)` to
   recover `transaction_id`, loaded into a `Join`-engine table for that
   partition (~200 M rows, ~5 GB in memory — fits; 125 GB box), `WHERE` on
   the partition key so each mutation touches one partition's parts.
4. Coverage gate: per partition, `countIf(op_index IS NULL)` on
   `soroban_events` equals the partition's tx-level + diagnostic event count.
5. `DROP TABLE soroban_event_ops`; 0453/0457 read the columns.

Why not do the column now: the live path change and the ALTER need their own
deploy window, and 0540's window is already carrying `DROP COLUMN
net_settled` plus three new tables. One schema change per window (0310).

## Implementation Plan

1. **Parser** — capture the position within the operation (one `enumerate()`).
2. **Row + staging** — new row type; extend the targeted-write mode 0540 adds so
   one pass writes both new tables and touches nothing else.
3. **Table** — create by hand on prod (`init.sql` is fresh-install only).
4. **Backfill** — rides 0540's pass.
5. **Consumers** — retire 0453's read-time decode; hand 0457 the join.

## Acceptance Criteria

- [ ] `soroban_event_ops` created, keyed like `soroban_events`
- [ ] Written by the same pass as 0540 — no second re-parse
- [ ] `soroban_events` is **not** re-inserted; the two columns are added by
      `ALTER` and filled by per-partition mutations from the side table (see
      "Target shape"), then the side table is dropped
- [ ] Coverage proven against the source: for a sampled range, every per-op
      event in the archive meta has a row, and no row exists for a tx-level or
      diagnostic event
- [ ] ~~0453's transaction-detail render reads the table instead of decoding
      XDR, and the micro-backend is removed~~ — withdrawn 2026-09-16: the
      micro-backend also serves signatures, envelope/result/meta XDR, the
      operation list and the invocation tree; `op_index` is one field of many
- [ ] **Docs updated** — `docs/architecture/database-schema/**` and
      `xdr-parsing/**` per ADR 0032

## Canonical event identity — measured and decided (2026-09-16)

### What Stellar defines (stellar-rpc v23+, read from source)

`getEvents` returns `id` = `%019d-%010d` of `TOID(ledger, tx, op)` and the event
number (`go-stellar-sdk/protocols/rpc/cursor.go`; built in stellar-rpc
`internal/db/event.go`, `InsertEvents`). The same string is the paging cursor
and sorts chronologically inside a ledger.

| Component | Meaning                                     | Base  |
| --------- | ------------------------------------------- | ----- |
| ledger    | ledger sequence                             | —     |
| tx        | application order in the ledger             | **1** |
| op        | index into `TransactionMetaV4.operations[]` | **0** |
| event     | position **within the operation**           | **0** |

Transaction-level (fee) events are NOT id-less — they get sentinels by stage:
`BEFORE_ALL_TXS` → `(ledger, 0, 0, n)` counting across the ledger;
`AFTER_TX` → `(ledger, tx, 4095, n)` counting per tx;
`AFTER_ALL_TXS` → `(ledger, 1048575, 0, n)` counting across the ledger.
Diagnostic events get no id and are absent from `getEvents` since v23.
`transactionIndex` / `operationIndex` are returned per event since v23.0.0.
Before rpc v23 the id was `(ledger, tx, 0, index over the whole tx)` and is not
comparable with v23+ ids. SEP-35 / Horizon number operations from 1; rpc from 0.

### Where the project stands

|             | canonical location                                                          | ours                                                                           |
| ----------- | --------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| tables      | `asset_transfers`, `soroban_event_ops`, `transactions`, `transaction_memos` | `soroban_events`, `soroban_invocations_appearances`, the four presence indexes |
| transaction | `application_order` (1-based, = rpc tx)                                     | `transaction_id` = hash64 of the hash                                          |
| event       | `op_index`, `event_pos_in_op` (0-based, = rpc)                              | `event_index`: flat per tx, fee events first                                   |

Measured on production 2026-09-16: `transactions.application_order` runs 1..N;
`soroban_event_ops.op_index` / `event_pos_in_op` start at 0 — identical to rpc.
`soroban_event_ops`: 5.69 bn rows, 3.34 GiB, written live up to the ingest head,
**read by nothing**.

### Why the schema's defence of `event_index` does not hold

`init.sql` gives two reasons. (1) "Deterministic on replay" — true, and equally
true of the canonical identity, which is a function of the ledger meta alone
(the schema says so itself for `asset_transfers`). (2) "Not expressible for
fee and diagnostic events" — fee events have rpc ids (sentinels above, and the
parser already knows each one's `stage`); diagnostic events are not stored:
staging drops them (`stage.rs`, `is_diagnostic`). The comment is stale on both.

### What it costs today

- Contract events and contract invocations are ordered
  `(ledger, transaction_id, event_index)` (`api/src/contracts/queries.rs`), so
  events of different transactions in one ledger come out in hash order, not in
  execution order; inside a transaction the fee refund (settled after every
  transaction) is numbered before the operations.
- The event number on the transaction page cannot be matched to `getEvents`
  or any other tool.
- `soroban_events` is 235.95 GiB / 10.59 bn rows; its `transaction_id` column is
  **50.48 GiB (21%)**, `event_index` 4.69 GiB. Position columns sorted after the
  ledger cost 0.12–0.23 B/row (`asset_transfers`, `soroban_event_ops`).
- `application_order` names two things: the transaction's position in
  `transactions` / `asset_transfers` / `soroban_event_ops`, the operation's
  position in `operations_appearances` / `lp_operation_amounts`.

### Decision (karolkow, 2026-09-16) — option A

`soroban_events` is keyed by the canonical location:
`(contract_id, ledger_sequence, application_order, op_index, event_pos)` with
the rpc sentinels for fee events, and the API exposes the rpc-format id.

1. New table with the new key; filled per partition from `soroban_events` +
   `soroban_event_ops` + `transactions` (no S3); fee-event sentinels from the
   stage the parser already extracts (not stored today — the fill needs it).
2. Live writer switched in the same window; swap by `EXCHANGE TABLES`
   (indexer stopped, per `docs/backfills.md`).
3. Coverage gate per partition before the swap: row counts equal, every
   per-op event has a location, every fee event has a sentinel.
4. Readers: contract events and invocations in execution order; rpc ids on the
   wire; `#op-N` anchors possible; 0457 gets its attribution.
5. Drop `soroban_event_ops`; rewrite the stale `init.sql` comment; resolve the
   `application_order` double meaning.

Space: production free 368.72 GiB of 1.72 TiB (backups share the volume); the
new table is smaller than the 236 GiB it replaces — confirm per partition.

### Coverage of `soroban_event_ops` against `soroban_events` — measured (2026-09-16)

Exact row-by-row join over every ledger below 64,440,000 (50,457,424 onward),
in 20k-ledger slices (5k where the join exceeded the per-query memory cap):
`soroban_events` → `transactions` (`id` → `application_order`) → full outer
join with `soroban_event_ops` on `(ledger_sequence, application_order,
event_index)`.

| check                                                                             | result                                                     |
| --------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| events without their transaction                                                  | 0                                                          |
| events without an op row that are not a native-XLM fee event                      | 0                                                          |
| op rows without an event                                                          | 0                                                          |
| native-XLM fee events (index 0/1, `contract_id` of the native SAC) with an op row | 0                                                          |
| events without an op row                                                          | 4,974,589,984 — all native-XLM `fee`, `event_index` 0 or 1 |
| events with an op row                                                             | 5,595,366,384 distinct                                     |

So "has no op row" is exactly "tx-level fee event"; the fill can tell the two
apart by the join, never by the event name — 39 events named `fee` come from
another contract's own operation and do have op rows.

**Duplicates in the side table.** 85,970,362 extra rows, every one a byte-equal
copy (0 keys with differing `op_index` / `event_pos_in_op`): ledgers
64,128,000–64,317,019 written twice (582 keys three times), plus single
ledgers 55,077,289 and 59,697,154. `soroban_events` and `transactions` carry no
duplicates in those ranges. The fill deduplicates on the key; nothing to repair.

Not yet checked against the chain: this proves the two tables agree, both
written by the same parser. The rpc-id comparison (`getEvents` on a recent
range) belongs to the swap gate, and must also settle how the fee sentinel is
derived — the stage is not stored; one real fixture shows index 0 =
`BeforeAllTxs` charge, index 1 = `AfterAllTxs` refund — settled in "Fee event identity" below.

### Fee event identity — verified against the chain (2026-09-16)

The fee sentinel needs the event's stage, which is not stored. It is derivable
from what is stored — the event's position among the transaction's fee events
and the ledger — with no re-parse.

**Rule.**

| our row                                         | stage (from archive meta) | rpc id                                                     |
| ----------------------------------------------- | ------------------------- | ---------------------------------------------------------- |
| fee event, `event_index` 0 (charge)             | `BeforeAllTxs`, always    | tx 0, op 0, event = rank of the charge in the ledger       |
| fee event, `event_index` 1, ledger ≤ 58,762,517 | `AfterTx`                 | tx = `application_order`, op 4095, event 0                 |
| fee event, `event_index` 1, ledger ≥ 58,762,518 | `AfterAllTxs`             | tx 1048575, op 0, event = rank of the refund in the ledger |

Rank = 0-based position among the same-stage fee events of that ledger, by
`application_order` (stellar-rpc `internal/db/event.go`, `txEventIndices`:
one counter per stage per ledger, `afterTx` reset per transaction). Every
transaction is charged, so a charge's rank equals `application_order − 1`;
refunds exist only for some, so theirs does not.

**Evidence.**

- Live rpc (`getEvents`, v28.0.1, native SAC `fee` topic), ledgers 64,340,000 /
  64,370,000 / 64,400,000 / 64,430,000 / 64,450,000: 2,104 rpc events = 2,104
  of our rows, 0 mismatches on transaction and amount; 1,530 charges and 574
  refunds, every rpc event index equal to the rank rule.
- Archive meta decoded with the official CLI (`stellar xdr decode --type
LedgerCloseMetaBatch`), stage per transaction: protocols 20, 21, 22 (ledgers
  50,475,303 / 53,015,049 / 56,019,779 / 58,513,130) — refunds `after_tx`;
  protocols 23, 24 (58,816,920 / 60,016,783) — `after_all_txs`. At the upgrade:
  58,762,516 (p22) and 58,762,517 (header already p23, transactions applied
  under p22) `after_tx`; 58,762,518 `after_all_txs`, the first with
  `post_tx_apply_fee_processing`. No transaction carries more than one refund.
- The pre-23 id (`AfterTx`) is taken from stellar-rpc's source; no live rpc
  retains those ledgers, so it is not confirmed by an rpc answer.

**Official sources (checked 2026-09-16).**

- CAP-67 (`stellar-protocol/core/cap-0067.md`, "New Events for Representing
  Fees"): the charge is always `BEFORE_ALL_TXS`; a refund is emitted only when
  non-zero, `AFTER_TX` before protocol 23 and `AFTER_ALL_TXS` from 23. It also
  says future protocols may add more fee events following the same stage
  pattern — the reason the live writer must use the parsed stage, not this
  position rule.
- stellar-core `src/transactions/TransactionFrame.cpp`: the refund stage is
  chosen from the ledger header's version at apply time
  (`protocolVersionStartsFrom(... V_23)`), before that ledger's upgrades are
  applied — which is why the upgrade ledger 58,762,517 still carries `after_tx`.
- stellar-docs OpenRPC `getEvents`: `id` is "based on the TOID format" (SEP-35)
  plus a 10-digit event index. The docs do not describe the fee sentinels, and
  SEP-35 counts operations from 1 while rpc counts them from 0; the only full
  statement of the id is stellar-rpc's `internal/db/event.go`.

**Further checks.**

- Every event id, not only fees: 8 ledgers inside the rpc window (including 3
  with `system` events), 7,365 rpc events = 7,365 ids derived from our tables,
  same transaction for each. Per-operation events use `application_order`,
  `op_index` (0-based) and `event_pos_in_op` directly.
- Every transaction has exactly one charge: 4,181,443,104 charge events =
  4,181,443,104 transactions below ledger 64,440,000, equal in every partition.
  So a charge's counter is always `application_order − 1`; only refunds need a
  per-ledger rank.
- Archive meta across the whole ingested range: one refund-heavy ledger per
  64k-ledger archive partition plus the three upgrade ledgers — 248 ledgers,
  50,490,188–64,451,818, protocols 20–27 (30/45/71/15/25/26/17/19), 104,454
  transactions, 27,512 refunds. For every transaction: `application_order`
  equals its position in `tx_processing`; the tx-level events are only
  native-XLM `fee`, at most two; the charge is `before_all_txs`; the refund's
  stage follows the ledger rule; amounts and positions equal our rows.
  0 anomalies.

## Design — `soroban_events` keyed by the canonical location (2026-09-16)

### Table

```sql
CREATE TABLE soroban_events_staging_canonical
(
    contract_id        Int64,
    ledger_sequence    Int64,
    transaction_index  UInt32,  -- rpc id: 1..N; 0 = before all txs; 1048575 = after all txs
    operation_index    UInt16,  -- rpc id: 0-based; 4095 = after the transaction's operations
    event_index        UInt32,  -- rpc id: position in the operation; fee events: stage counter
    application_order  Int16,   -- the transaction the event belongs to (joins `transactions`)
    event_type         Int16,
    signature          LowCardinality(Nullable(String)),
    topics_xdr         String CODEC(ZSTD(3)),
    data_xdr           String CODEC(ZSTD(3))
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (contract_id, ledger_sequence, transaction_index, operation_index, event_index);
```

- Dropped: `transaction_id` (50.48 GiB) and the flat `event_index` (4.69 GiB).
- `application_order` stays although it repeats `transaction_index` for every
  operation event: a refund settled after all transactions carries the
  sentinel in its id, and only this column says which transaction it refunds.
  The transaction page filters on it for every row alike.
- The sort key within a contract is execution order, fee charges first and
  end-of-ledger refunds last — the contract event list pages on it directly.

| row                         | `transaction_index` | `operation_index` | `event_index`                   |
| --------------------------- | ------------------- | ----------------- | ------------------------------- |
| operation event             | `application_order` | `op_index`        | `event_pos_in_op`               |
| fee charge (position 0)     | 0                   | 0                 | `application_order − 1`         |
| refund, ledger ≤ 58,762,517 | `application_order` | 4095              | 0                               |
| refund, ledger ≥ 58,762,518 | 1048575             | 0                 | rank among the ledger's refunds |

### Fill — per 5k-ledger slice, in ClickHouse, no S3

```sql
INSERT INTO soroban_events_staging_canonical
SELECT
    e.contract_id,
    e.ledger_sequence,
    toUInt32(multiIf(o.has_op = 1, t.application_order,
                     e.event_index = 0, 0,
                     e.ledger_sequence >= 58762518, 1048575,
                     t.application_order)),
    toUInt16(multiIf(o.has_op = 1, o.op_index,
                     e.event_index = 0, 0,
                     e.ledger_sequence >= 58762518, 0,
                     4095)),
    toUInt32(multiIf(o.has_op = 1, o.event_pos_in_op,
                     e.event_index = 0, t.application_order - 1,
                     e.ledger_sequence >= 58762518, r.refund_rank,
                     0)),
    t.application_order,
    e.event_type, e.signature, e.topics_xdr, e.data_xdr
FROM soroban_events AS e
INNER JOIN (SELECT id, application_order FROM transactions
            WHERE ledger_sequence >= {A} AND ledger_sequence < {B}) AS t
    ON t.id = e.transaction_id
LEFT JOIN (SELECT ledger_sequence, application_order, event_index,
                  any(op_index) AS op_index, any(event_pos_in_op) AS event_pos_in_op,
                  toUInt8(1) AS has_op
           FROM soroban_event_ops
           WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
           GROUP BY ledger_sequence, application_order, event_index) AS o
    ON o.ledger_sequence = e.ledger_sequence
   AND o.application_order = t.application_order
   AND o.event_index = e.event_index
LEFT JOIN (SELECT f.ledger_sequence, f.transaction_id,
                  toUInt32(row_number() OVER (PARTITION BY f.ledger_sequence
                                              ORDER BY ft.application_order) - 1) AS refund_rank
           FROM soroban_events AS f
           INNER JOIN (SELECT id, application_order FROM transactions
                       WHERE ledger_sequence >= {A} AND ledger_sequence < {B}) AS ft
               ON ft.id = f.transaction_id
           WHERE f.ledger_sequence >= {A} AND f.ledger_sequence < {B}
             AND f.contract_id = -6164601581949826601   -- native SAC
             AND f.signature = 'fee' AND f.event_index = 1) AS r
    ON r.ledger_sequence = e.ledger_sequence AND r.transaction_id = e.transaction_id
WHERE e.ledger_sequence >= {A} AND e.ledger_sequence < {B};
```

"No op row" is the fee test, never the event name (coverage section above).
The `GROUP BY` collapses the side table's duplicate copies.

Dry run of this SELECT (read-only, 2026-09-16):

| slice                 | rows      | distinct new keys | distinct old keys | before all | after tx | after all |
| --------------------- | --------- | ----------------- | ----------------- | ---------- | -------- | --------- |
| 64,000,000–64,005,000 | 4,887,885 | 4,887,885         | 4,887,885         | 1,738,003  | 0        | 685,689   |
| 56,000,000–56,010,000 | 6,251,653 | 6,251,653         | 6,251,653         | 2,772,663  | 42,554   | 0         |
| 58,760,000–58,765,000 | 2,937,803 | 2,937,803         | 2,937,803         | 1,102,737  | 177,896  | 54,687    |

No key collisions; the boundary slice carries both refund kinds. The ids this
exact SQL produces for the 8 rpc-window ledgers: 7,365 of 7,365 equal to
`getEvents`, same transaction.

### Gates

1. Before each INSERT (read-only): distinct new keys = distinct old keys for
   the slice — a collision would be silently merged away by the engine.
2. After the fill, per partition: distinct rows equal to the old table;
   `transaction_index = 0` rows equal the partition's transactions; `4095`
   only below 58,762,518, `1048575` only from it.
3. Ids read back from the new table match `getEvents` on recent ledgers.

### Live writer

Staging computes the id from the parsed event, not from the rule above:
operation events from `op_index` / `event_pos_in_op`; tx-level events from
their `stage`, with per-ledger counters for `BeforeAllTxs` / `AfterAllTxs` and
a per-transaction counter for `AfterTx`, walked in application order. A future
protocol that adds stage events stays correct. A tx-level event without a
stage (V3 meta — absent from the archive and from live ingest) is a staging
error, never a guessed id. `soroban_event_ops` writes stop.

### Readers (same PR)

| reader                                       | change                                                                                                                                                                                 |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| contract events (`contracts/queries.rs`)     | order and cursor on the new key; transaction resolved by `(ledger_sequence, application_order)`; wire carries the rpc `id` instead of `transaction_id` (the frontend does not read it) |
| contract stats `recent_events`               | none (contract + ledger)                                                                                                                                                               |
| transactions filtered by contract (arm)      | arm yields `(ledger_sequence, application_order)`, mapped to the id through the `transactions` key until 0538 moves the driver to positions                                            |
| transaction detail `fetch_event_appearances` | filter `(ledger_sequence, application_order)`                                                                                                                                          |
| `balance_seed` (backfill-runner)             | reads `contract_id` / topics / data only; test fixtures' column lists                                                                                                                  |

API types regenerated; architecture docs (schema, pipeline, xdr parsing);
`init.sql` comment on the table rewritten; `docs/backfills.md` gets the fill.

### Space

Built alongside the old table: ~185 GiB by column arithmetic (236 GiB − 55 GiB
dropped + the new integer columns, estimate) against 368.72 GiB free.
Measured on the first filled partition before the rest.

### Decisions (karolkow, 2026-09-16)

- **Column names — rpc names** (`transaction_index`, `operation_index`,
  `event_index`). `event_index` changes meaning; every reader of the table is
  rewritten in the same change and a grep confirms no query keeps the old
  column.
- **Cutover — one change, one window** (a phased dual write was considered and
  rejected: three deploys, a second write path for the whole fill, and the old
  name occupied until the end). `EXCHANGE TABLES` keeps the name
  `soroban_events`.

### Rollout

1. Operator creates `soroban_events_staging_canonical` (DDL above). Nothing
   writes it.
2. Fill partition 127 first; measure size per column and run the gates. Stop
   there if the table is not smaller than the old partition.
3. Fill the remaining partitions while the indexer runs, one partition at a
   time, gate per partition.
4. One PR: live writer, the five readers, API types, docs, ADR for the naming.
   Merged, not deployed.
5. Window: stop the indexer → fill the tail up to the head → gates on the tail
   → `EXCHANGE TABLES soroban_events AND soroban_events_staging_canonical` →
   deploy Compute → start the indexer. Ingest paused ~30–60 min (estimate,
   nothing lost — the queue holds it); event reads fail for the minutes between
   the swap and the end of the deploy.
6. After production checks (ids against `getEvents`, contract event order):
   drop the old table (now under the staging name) and `soroban_event_ops`.
   Until then the old table is the rollback; ledgers indexed after the swap
   are only in the new one.

### Pre-fill gate on partition 127 (read-only, 2026-09-16)

100 slices of 5k ledgers (20k exceeds the read profile's memory cap for the
distinct counts; the INSERT itself carries no such aggregate): 452,275,397
rows = distinct new keys = distinct old keys = the partition's active row
count. `transaction_index = 0` rows 170,740,563 = the partition's
transactions; `operation_index = 4095` rows 0 (all ledgers ≥ 58,762,518);
`transaction_index = 1048575` rows 78,294,908.

## Pre-implementation review (2026-09-16) — rollout above is superseded

Stopped before any production write to check what the plan assumed. Verified
facts first, then what they change.

### Verified

- `default` is an `Atomic` database, so `EXCHANGE TABLES` is available. No
  materialized view, view or dictionary depends on `soroban_events` or
  `soroban_event_ops`; the co-located `prices` database does not read them.
- Last 14 days of `system.query_log`: only `ingestion_writer` (indexer),
  `api_reader`, the operator account and read-only development access touch
  the two tables.
- Operator writes (`chw`) run under the `admin` profile: 20 GB per query, no
  execution cap. Server memory cap 100 GiB, shared with the API and indexer.
- Disk: 368.32 GiB free; `/backups/` is the same volume. Weekly backup is
  `FREEZE` (hardlinks) → Borg → `UNFREEZE`, Sunday 03:30 UTC.
- Pausing the indexer by disabling its SQS trigger is undone by the next
  Compute deploy (`docs/deployment.md`); only `indexerLambdaConcurrency: 0`
  plus a deploy is durable. Failed deliveries move to the DLQ after 10 receives.

### What the plan missed

1. **The window as written is unsafe.** "Stop the indexer → … → deploy" with
   the quick pause lets the deploy re-enable the trigger; old indexer code
   inserting into the swapped table fails every ledger toward the DLQ. The
   pause must be the durable one, deployed from the code currently in
   production, and the new code deployed after the swap.
2. **Programme order skipped.** 0538 puts the naming ADR and a measured trial
   (size and read-path benchmark) before any migration. Column names in a
   DDL are the expensive thing to change later, and the table holds both
   `transaction_index` (rpc id, with sentinels) and `application_order` (the
   real position).
3. **Scope gaps.** The transaction page's archive-decoded events expose and
   display our flat `event_index` (`XdrEventDto`, `EventsSection.tsx`), so it
   would still number events differently from the contract page; its doc
   comment also claims pre-23 events carry no stage, which the archive
   refutes. `asset_transfers.event_index` exists only to join the old key and
   is orphaned by the change (dropping it needs the DEFAULT-first order).
   `account_reconciliation` and `redecode_diff` tests, the `--only
soroban_event_ops` backfill flag, `docs/backfills.md` and the merge scripts'
   table lists reference the old shape.
4. **Backups.** A `FREEZE` during the fill pins parts that merges would
   replace, and Borg uploads the new ~185 GiB table as new data — then the old
   one again under the staging name until it is dropped.
5. **Load.** Fill queries may take 20 GB each on the box serving the API; they
   run one at a time, outside the backup window, with the API latency watched.
6. **Rollback is asymmetric.** After the swap the new writer stops
   `soroban_events` (old shape) and `soroban_event_ops`; going back needs a
   re-ingest of every ledger since the swap into both.
7. **Live writer proof.** The id counters depend on walking transactions in
   application order; tests need real meta for a pre-23 refund, a post-23
   ledger with several refunds, and a runnable `getEvents` comparison kept as a
   check (ADR 0057).

### Revised order

1. ADR: canonical event identity and names (`application_order` = SEP-35
   transaction application order, the real position; rpc id components named
   as rpc).
2. Trial: create the staging table with the final DDL, fill partition 127,
   measure per-column size, benchmark the three read paths against the old
   table on that range. Go / no-go.
3. PR: writer, readers (contract events, transactions-by-contract arm,
   transaction detail and its archive-decoded ids, frontend numbering),
   `asset_transfers.event_index` out of the struct, `soroban_event_ops` writes
   removed, tests above, docs, API types. Merged, released only in step 6.
4. Fill the remaining partitions one at a time, gates and disk check after
   each, never across Sunday 03:30 UTC.
5. `asset_transfers.event_index` gets a DEFAULT (no pause needed).
6. Window: deploy `indexerLambdaConcurrency: 0` from the production code →
   fill the tail, gate → `EXCHANGE TABLES` → deploy the new code with
   concurrency 1 → `getEvents` check, contract event order, transaction page.
7. After the rollback horizon: drop the old table, `soroban_event_ops`, and
   `asset_transfers.event_index`.

### Decisions after the review (karolkow, 2026-09-17)

- **Transaction page numbering — same PR.** The archive-decoded events on the
  transaction page carry the rpc id, computed from the ledger meta (stage and
  per-ledger counters), and the frontend shows it; one numbering on every page
  from the first day.
- **`asset_transfers.event_index` — dropped in this change:** DEFAULT before
  the window, out of the writer struct in the PR, `DROP COLUMN` after the
  rollback horizon.
- **Rollback horizon — until the next backup.** Production checks on the day
  of the swap; the old table and `soroban_event_ops` are dropped before the
  following Sunday 03:30 UTC backup. Re-ingest stays the fallback after that.

### Naming found during the ADR draft

The same per-operation location already has names in `asset_transfers` and
`soroban_event_ops`: `application_order` (transaction, 1-based), `op_index`,
`event_pos_in_op` (0-based). `operations_appearances` and
`lp_operation_amounts` use `application_order` for the operation's position.
ClickHouse cannot rename a sort-key column ("Columns specified in the key
expression of the table … cannot be renamed", ALTER COLUMN docs), so aligning
`asset_transfers` would mean rebuilding it (43.96 GiB, 5.60 bn rows). Fee
events make the rpc id and the location differ, so both concepts exist
regardless of names.

**Decided (karolkow, 2026-09-17): stellar-rpc names everywhere** — ADR 0059
(proposed). stellar-rpc itself names a transaction's position
`applicationOrder` (`getTransaction` / `getTransactions`, 1-based) and uses
`transactionIndex` / `operationIndex` only on events, where fee events carry
the sentinel. So `application_order` stays in every table for the transaction;
the operation becomes `operation_index` and the event-in-operation
`event_index`; `transaction_index` exists only on event rows. The new
`soroban_events` DDL above already matches. `asset_transfers` renames
`op_index` / `event_pos_in_op` at its rebuild — task 0558 (backlog, not yet on
`develop`) plans a rebuild-and-swap of the same table for `token_id`; one
rebuild serves both. `operations_appearances` / `lp_operation_amounts` rename
their operation position in 0538.

Code work runs on `feat/0541_canonical-event-location` (worktree
`.claude/worktrees/feat-0541_canonical-event-location`, from `develop`
c765f4d8); lore and the ADR land on `develop`.

## Implementation and rollout plan (2026-09-17)

[`notes/S-implementation-and-rollout-plan.md`](notes/S-implementation-and-rollout-plan.md) — phases 1–5 (trial and benchmark, code tasks 2.1–2.8, partition fill, window runbook, cleanup), with the fill and gate SQL in [`notes/fill_insert.sql`](notes/fill_insert.sql) and [`notes/fill_gate.sql`](notes/fill_gate.sql). It supersedes the "Rollout" and "Revised order" lists above.

**Decided (karolkow, 2026-09-17): the transaction page shows the rpc id.** Its `#` column becomes `ID` with the full `getEvents` id, rows in execution order; the bare `event_index` (ledger-wide counter for fees, position in the operation otherwise) and a short `op N · M` form were rejected. Plan task 2.6–2.7.

## Phase 1 trial — partition 127 (2026-09-17)

Operator created `soroban_events_staging_canonical` and filled partition 127
(100 slices of 5k ledgers; 583 s of query time, median 5.6 s, peak 3.68 GiB per
statement).

**Correctness.** 452,275,397 rows = the old partition, in every one of the 100
slices; 452,275,397 distinct keys; `transaction_index = 0` rows 170,740,563 (=
the partition's transactions), `operation_index = 4095` 0, `transaction_index =
1048575` 78,294,908 — all equal to the pre-fill gate.

**Size.** 5.31 GiB against 7.19 GiB (−26%; still 6 parts, merges may shrink it
further). Per column (bytes/row, old → new): `topics_xdr` 8.516 → 8.332,
`data_xdr` 2.182 → 2.094, `transaction_id` 5.036 → gone, flat `event_index`
0.699 → gone; new `transaction_index` 0.634, `application_order` 0.460,
`event_index` 0.361, `operation_index` 0.150; `signature` 0.124 → 0.059. Whole
table by the same ratio ≈ 174 GiB (estimate).

**Read path** (median of 3, `system.query_log`; contracts: native XLM 271 M
events in the partition, `546855837558613593` 14 M, `5314455185855296541`
1,034):

| case                                                          | old                      | new                           |
| ------------------------------------------------------------- | ------------------------ | ----------------------------- |
| contract events, first page — XLM / mid / small               | 285 / 392 / 13 ms        | 119 / 193 / 9 ms              |
| contract events, cursor page — XLM / mid / small              | 290 / 354 / 15 ms        | 123 / 155 / 9 ms              |
| memory, first page — XLM / mid                                | 1.19 / 1.90 GiB          | 573 MiB / 1.10 GiB            |
| transaction resolve by id vs by position                      | 4 ms                     | 4 ms                          |
| transaction page event appearances                            | 81 ms, 24.7 M rows read  | 5 ms, 49 k rows read          |
| contract tx-list arm, `IN` over positions — XLM / mid / small | 315 / 29 / 3 ms          | **over 4 GB** / 4,331 / 57 ms |
| contract tx-list, today's full driver — XLM                   | **over 4 GB (6.04 GiB)** | —                             |
| contract tx-list arm, position window — XLM / mid             | —                        | 87 / 32 ms                    |

Verdict: the table and the contract-event and transaction-page reads pass with
margin. The `IN` mapping for the contract-filtered transaction list fails, and
the list is already broken for native XLM today; plan task 2.5 moves that list
to positions with bounded windows (measured 87 ms for native XLM).
