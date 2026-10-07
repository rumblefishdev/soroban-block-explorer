---
title: 'Implementation plan and record'
type: generation
status: mature
spawns: []
tags: [xdr-parsing]
links: []
history:
  - date: 2026-10-06
    status: mature
    who: claude
    note: 'Moved out of README.md at completion (task file size limit).'
---

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
