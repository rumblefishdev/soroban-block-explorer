---
id: '0059'
title: 'Events are identified by the stellar-rpc event id; positions carry stellar-rpc names'
status: proposed
deciders: [karolkow]
related_tasks: ['0541', '0538', '0540', '0558', '0453']
related_adrs: ['0044', '0057']
tags: [clickhouse, schema, soroban-events, identity, naming, xdr-parsing]
links:
  - https://github.com/stellar/stellar-protocol/blob/master/core/cap-0067.md
  - https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0035.md
  - https://github.com/stellar/stellar-rpc/blob/main/cmd/stellar-rpc/internal/db/event.go
history:
  - date: '2026-09-17'
    status: proposed
    who: karolkow
    note: >
      Drafted in task 0541 before the soroban_events rebuild, after the
      identity was verified against getEvents and the archive meta. Amends the
      soroban_events key of ADR 0044; first convention of the 0538 programme.
---

# ADR 0059: Events are identified by the stellar-rpc event id; positions carry stellar-rpc names

**Related:**

- [Task 0541: canonical event location](../1-tasks/active/0541_FEATURE_canonical-event-location/README.md)
- [Task 0538: canonical transaction and event location (programme)](../1-tasks/backlog/0538_EPIC_canonical-transaction-and-event-location/README.md)
- [ADR 0044: ClickHouse store, full-content `soroban_events`](./0044_clickhouse-pilot-parallel-store.md)
- [ADR 0057: the network is the arbiter](./0057_network-is-the-arbiter-snapshot-reconciliation.md)

---

## Context

`soroban_events` is keyed `(contract_id, ledger_sequence, transaction_id,
event_index)`. `transaction_id` is a 64-bit hash of the transaction hash;
`event_index` is a counter we assign per transaction across fee, operation and
diagnostic events. Neither exists outside this project:

- events of one ledger come out in hash order, not in the order they executed;
- an event number on our pages cannot be matched to `getEvents` or any other
  tool;
- `transaction_id` costs 50.48 GiB of the table's 236 GiB.

Stellar defines the identity. stellar-rpc (v23+) gives every non-diagnostic
event an id `%019d-%010d`: a TOID (SEP-35 bit layout: ledger 32 bits,
transaction 20, operation 12) and an event number. The id is a function of the
ledger meta alone.

Stellar also names the positions, and the project does not follow it:

| position                           | stellar-rpc / SEP-35                                                                             | this project today                                                                                                                                                          |
| ---------------------------------- | ------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| transaction in ledger (1-based)    | `applicationOrder` (`getTransaction`, `getTransactions`); SEP-35 "transaction application order" | `application_order` (`transactions`, `transaction_memos`, `asset_transfers`, `soroban_event_ops`)                                                                           |
| operation in transaction (0-based) | `operationIndex` (`getEvents`)                                                                   | `op_index` (`asset_transfers`, `soroban_event_ops`); `application_order`, **1-based**, for the operation (`operations_appearances`, `lp_operation_amounts`, operation DTOs) |
| event in operation (0-based)       | event part of the `getEvents` id                                                                 | `event_pos_in_op` (`asset_transfers`, `soroban_event_ops`)                                                                                                                  |
| event's transaction part of the id | `transactionIndex` (`getEvents`) — a sentinel for fee events                                     | —                                                                                                                                                                           |
| event in transaction (our counter) | —                                                                                                | `event_index` (`soroban_events`, `asset_transfers`, transaction-page DTOs)                                                                                                  |

---

## Decision

1. **An event's identity is its stellar-rpc id.** Per-operation event:
   transaction = application order (1-based), operation = index in the
   envelope (0-based), event = position in the operation (0-based). Fee
   (transaction-level) events take stellar-rpc's sentinels by stage:

   | stage                           | transaction  | operation | event                                                   |
   | ------------------------------- | ------------ | --------- | ------------------------------------------------------- |
   | `BEFORE_ALL_TXS` (charge)       | 0            | 0         | counter over the ledger's charges, in application order |
   | `AFTER_TX` (refund, < p23)      | its position | 4095      | counter within the transaction                          |
   | `AFTER_ALL_TXS` (refund, ≥ p23) | 1048575      | 0         | counter over the ledger's refunds, in application order |

   Diagnostic events have no id and are not stored. The API exposes the id in
   the rpc string format.

2. **Positions carry stellar-rpc's names, with its meanings.**

   - `application_order` — the transaction's position in its ledger, 1-based
     (`applicationOrder`). Every table that locates a transaction. Never an
     operation's position.
   - `operation_index` — the operation's position in its transaction, 0-based
     (`operationIndex`).
   - `event_index` — the event's position in its operation, 0-based; for a fee
     event, its stage counter (the event part of the id).
   - `transaction_index` — only on event rows, as in `getEvents`: equal to
     `application_order` for a per-operation event, the sentinel for a fee
     event. Nothing joins a transaction on it; joins use `application_order`.

3. **`soroban_events` is keyed by the event id** —
   `(contract_id, ledger_sequence, transaction_index, operation_index, event_index)`
   — and also stores `application_order`, which says which transaction a fee
   event belongs to. Within a contract the key is execution
   order. `transaction_id` and the per-transaction counter are removed.

4. **Existing names move when their table is rebuilt**, because ClickHouse
   cannot rename a sort-key column: `asset_transfers` `op_index` →
   `operation_index` and `event_pos_in_op` → `event_index` (its rebuild, shared
   with task 0558, after its flat `event_index` is dropped by task 0541);
   `operations_appearances` / `lp_operation_amounts` operation
   `application_order` → `operation_index`, 0-based (programme 0538). New
   tables use the names from the start.

5. **Stage comes from the meta, never from a rule.** The live writer takes each
   fee event's stage from the parsed `TransactionEvent`; an event without one
   is a staging error. The position rule (charge = position 0; refund =
   position 1, `AFTER_TX` up to ledger 58,762,517 and `AFTER_ALL_TXS` from
   58,762,518) is used once, to fill history, where it is verified.

6. **Surrogate transaction ids are not used for new keys**; existing tables
   move to the location as the 0538 programme reaches them, one table at a
   time.

7. **`getEvents` is the arbiter** (ADR 0057): a runnable check compares ids
   read from our tables with `getEvents` on recent ledgers.

---

## Rationale

- The id is Stellar's, deterministic from the meta, and total for every row we
  store — the two reasons the schema gave for our own counter ("deterministic
  on replay", "not expressible for fee events") do not hold.
- Keying by it gives execution order for free and makes every event number
  checkable against the network.
- stellar-rpc already separates the two meanings: a transaction object carries
  `applicationOrder`, an event carries `transactionIndex` (sentinel for fees).
  Following it keeps a sentinel from ever being joined as a transaction
  position.
- One vocabulary, the network's, instead of names only this project knows.

Verified on production data (task 0541): 7,365 of 7,365 ids equal to
`getEvents` on 8 ledgers (the fill SQL itself); 248 archive ledgers across
protocols 20–27 with 0 stage anomalies; 4,181,443,104 charge events for
4,181,443,104 transactions; CAP-67 and stellar-core's `TransactionFrame.cpp`
state the stage rule.

---

## Alternatives Considered

### Alternative 1: keep `transaction_id` + our counter, add the location as columns

**Description:** `ALTER ADD COLUMN` op position and event position; order at
read time.

**Pros:** no rebuild.

**Cons:** keeps 50 GiB of hash; the table's own key stays in hash order; fee
events still have no checkable number.

**Decision:** REJECTED — fixes display only, not identity or storage.

### Alternative 2: key by location + stored stage, derive the id when reading

**Description:** `(contract_id, ledger_sequence, application_order, op_index,
event_pos_in_op)` plus `stage`.

**Pros:** no sentinels in the table.

**Cons:** fee events sort inside their transaction instead of before or after
all of them, so native-XLM lists are not in execution order; the refund
counter must be recomputed per request over the whole ledger.

**Decision:** REJECTED — the key would not be the order Stellar executes.

### Alternative 3: rpc names on the event id only, project names elsewhere

**Description:** `soroban_events` uses `transaction_index` /
`operation_index` / `event_index`; `asset_transfers` keeps `op_index` /
`event_pos_in_op`.

**Pros:** no rename rebuild of `asset_transfers`.

**Cons:** the same per-operation position under two names in the two tables
that join on it.

**Decision:** REJECTED — two vocabularies for one location (karolkow,
2026-09-17).

### Alternative 4: `transaction_index` as the transaction position everywhere

**Description:** rename `application_order` to `transaction_index` in every
table.

**Pros:** one word for "transaction" in positions.

**Cons:** not stellar-rpc's name for a transaction's position; on event rows
`transaction_index` carries fee sentinels, so a join on it silently drops fee
events; rebuilds `transactions` for a rename.

**Decision:** REJECTED — contradicts the network's own naming.

---

## Consequences

### Positive

- Contract events list in execution order; every event number matches
  `getEvents`.
- ~55 GiB less in `soroban_events`; `soroban_event_ops` and the flat
  `asset_transfers.event_index` go away.
- Position names match stellar-rpc in every table, eventually.

### Negative

- A full rebuild of `soroban_events` with a stop-the-indexer swap.
- Until their rebuilds, `asset_transfers`, `operations_appearances` and
  `lp_operation_amounts` keep the old names; joins spell out the mapping.
- `asset_transfers` needs a rebuild to rename key columns (planned together
  with 0558's).

---

## Delivery Checklist

Per [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md) — ticked in
the task 0541 PR:

- [ ] `docs/architecture/technical-design-general-overview.md` updated (or N/A)
- [ ] `docs/architecture/database-schema/database-schema-overview.md` updated (or N/A)
- [ ] `docs/architecture/backend/backend-overview.md` updated (or N/A)
- [ ] `docs/architecture/frontend/frontend-overview.md` updated (or N/A)
- [ ] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` updated (or N/A)
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md` updated (or N/A)
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` updated (or N/A)
- [ ] This ADR is linked from each updated doc at the relevant section

---

## References

- [CAP-67](https://github.com/stellar/stellar-protocol/blob/master/core/cap-0067.md) — "New Events for Representing Fees": charge and refund stages
- [SEP-35](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0035.md) — TOID bit layout, "transaction application order"
- [stellar-rpc `internal/db/event.go`](https://github.com/stellar/stellar-rpc/blob/main/cmd/stellar-rpc/internal/db/event.go) — id assignment and fee sentinels
- stellar-core `src/transactions/TransactionFrame.cpp` — refund stage chosen by protocol version
- [stellar-docs OpenRPC](https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getEvents) — `getEvents` `id` / `transactionIndex` / `operationIndex`; `getTransactions` `applicationOrder` ("the 1-based index of the transaction among all transactions included in the ledger")
- [ClickHouse ALTER COLUMN](https://clickhouse.com/docs/sql-reference/statements/alter/column) — key columns cannot be renamed
