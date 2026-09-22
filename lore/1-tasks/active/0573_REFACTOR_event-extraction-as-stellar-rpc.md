---
id: '0573'
title: 'REFACTOR: extract Soroban events the way stellar-rpc does — one pass, consensus and diagnostic apart'
type: REFACTOR
status: active
related_adr: ['0059']
related_tasks: ['0541', '0182', '0540', '0572']
tags: ['xdr-parsing', 'indexer', 'api', 'effort-medium', 'priority-medium']
links:
  - crates/xdr-parser/src/event.rs
  - crates/xdr-parser/src/types.rs
history:
  - date: 2026-09-22
    status: active
    who: karolkow
    note: >
      Filed and started after a review of `event.rs` following task 0541. The
      ids are right (3,298 of 3,298 against `getEvents`); the code that
      reaches them is not shaped like the source it copies.
---

# Extract Soroban events the way stellar-rpc does

## Summary

Rewrite `xdr-parser`'s event extraction to mirror its two upstream sources one
to one: stellar-go's `LedgerTransaction.GetTransactionEvents` for the model
(per transaction: transaction-level events with a stage, per-operation lists,
diagnostic events; V3 is one operation and no transaction-level events) and
stellar-rpc's `InsertEvents` for the numbering (one pass; `beforeAll` and
`afterAll` counted over the ledger, `afterTx` per transaction). Behaviour does
not change; a differential test proves it.

## Context

The ids task 0541 stores are right: `event_id_reconciliation` is green against
`getEvents`. The code that produces them has four structural problems:

1. **A flat model instead of the official one.** All three containers go into
   one `Vec<ExtractedEvent>`, and six fields describe where an event came from
   (`source`, `stage`, `op_index`, `event_pos_in_op`, `position_in_tx`,
   `event_id`); most combinations are meaningless and representable.
2. **Numbering happens apart from building.** `tx_level_event_ids` walks
   `v4.events`, `extract_events` walks them again, and `assign_event_ids`
   zips the two by order. stellar-rpc assigns the id in the loop that reads
   the event.
3. **Diagnostic events share the list with consensus events.** Seven files
   repeat `if source == Diagnostic { continue }` (the class of task
   0182's double count), the API splits them back apart, and `event_id` has to
   be an `Option`.
4. **V3 disagrees with the SDK.** V3 contract events are tagged `TxLevel`
   without a stage, so they get no id and staging refuses them; stellar-go
   treats them as operation 0. Dormant: the archive serves V4 for every
   protocol (task 0541 measured 1,265 of 1,265 transactions).

`position_in_tx` is no longer stored anywhere; it survives in log lines only.

## Implementation Plan

### Step 1: differential baseline

Before touching the code, dump the current output for real ledgers across
protocols (20, 22, 23 and 27, plus the `ledger_58816920` fixture): per event,
its id, where it came from, contract, and a hash of topics and data. Commit
the dump as a fixture.

### Step 2: the new model

```text
TxEvents { events: Vec<Event>, diagnostic: Vec<EventBody> }
Event    { id: EventId, origin: Origin, body: EventBody }
Origin   = Transaction(TransactionEventStage) | Operation(u16)
```

`LedgerEvents::new` counts stages only (no decoding); `transaction(i)` is
stellar-rpc's loop body for one transaction. `EventId` and its sentinels stay.
`extract_executable_update` moves to its own module.

### Step 3: consumers

Staging, `contract_transactions`, `asset_transfers`, the NFT and pool
extractors, the transaction page: diagnostic filters removed, operation and
stage read from `origin`, rejected transfers logged by rpc id.

### Step 4: prove it

The Step 1 dump matches byte for byte; a synthetic V3 meta gets
`(ledger, tx, 0, i)` like stellar-go; `event_ids_real_ledger` and
`event_id_reconciliation` pass.

## Acceptance Criteria

- [x] Differential dump of the old code equals the new code's, byte for byte —
      9,188 events of 4 ledgers; the frozen ids of 64,550,000 also equal
      `getEvents` (1,280 of 1,280, 2026-09-22)
- [x] One pass assigns every id; no function numbers events apart from
      building them
- [x] Diagnostic events cannot reach a consensus consumer: a separate list, no
      id, no `source` filter left in any consumer
- [x] V3 contract events are operation 0, as in stellar-go, with rpc ids
- [x] `position_in_tx`, `op_index`, `event_pos_in_op` and `source` gone from
      the event type
- [x] `event_id_reconciliation` green against production on the branch —
      5 ledgers (64,560,438–64,560,586), 5,830 ids equal on all three sides:
      `getEvents`, the table, the new parser on the archive (2026-09-22)
- [x] **Docs updated** — `xdr-parsing/xdr-parsing-overview.md` (containers,
      ids, the diagnostic type), `technical-design-general-overview.md`,
      `database-schema-overview.md`, `docs/backfills.md`,
      `docs/runbooks/live-tail-cutover.md`
- [x] **API types regenerated** — no diff: the transaction page's DTO is
      unchanged

## Implementation (2026-09-22)

Branch `refactor/0573_event-extraction`. `event.rs` 396 → 237 lines;
`extract_executable_update` moved verbatim, with its tests, to
`executable_update.rs`. 27 files changed in `crates/`, +713 −2,832 (the
bulk: `nft.rs`'s tests moved to `nft/tests.rs` and the one-off
`event_op_index_audit` example removed).

- `LedgerEvents::new` counts the stages of every transaction; `extract(i)` is
  stellar-rpc's loop body and returns `TxEvents { events, diagnostic }`.
- `ExtractedEvent` = `event_id` (never missing) + `origin`
  (`Transaction(stage)` | `Operation(u16)`) + the decoded body + transaction
  hash and close time. `ledger_sequence` is read from the id.
- `DiagnosticEvent` is its own type. The indexer takes `.events` only; the API
  maps both lists to the unchanged DTO.
- Removed consumer code: eight diagnostic filters in seven files, the staging
  error for a missing id, the API's `split_events`.

Verified: `cargo test` of `xdr-parser`, `db-clickhouse`, `indexer`, `api`,
`backfill-runner` green (the ClickHouse e2e tests against the local docker
server); `clippy -D warnings` clean.

**Tests changed:**

- `event/tests.rs` rewritten for the new API: 12 tests, including V3 as
  operation 0, the ledger counters when one transaction is asked for alone,
  an empty operation keeping its place, and the diagnostic mirror staying out
  of the consensus list.
- New: `tests/event_extraction_golden.rs` with four archive ledgers (protocols
  20, 23 on both sides of the refund-stage change, and 28).
- Removed, because the type now makes the state impossible: the staging
  "event without an id" error test, the three tests that a diagnostic event is
  skipped (wasm upgrade, pool registration, undeployed-SAC override), the
  asset-transfer diagnostic case, and `tx_event_stage_real_meta`'s test that
  `position_in_tx` numbers the refund before the operation.
- Fixtures that used `TxLevel` without a stage map to
  `Transaction(BeforeAllTxs)`, which keeps them out of `contract_transactions`
  as before; the share-token corpus keeps each event's place as its
  `event_index`, which is what its tie-break reads.

## Notes

Per-event copies of transaction context (`transaction_hash`, `created_at`)
stay for now; moving them to the caller touches the NFT and pool extractors
and is a separate step.
