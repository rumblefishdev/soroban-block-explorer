---
title: '0541 — implementation and rollout plan'
type: synthesis
status: developing
spawns: []
tags: [clickhouse, soroban-events, rollout, plan]
links:
  - ../../../../2-adrs/0059_canonical-event-identity-and-location-names.md
  - ./fill_insert.sql
  - ./fill_gate.sql
history:
  - date: 2026-09-17
    status: developing
    who: karolkow
    note: >
      Written after the pre-implementation review; phase 1 (trial) results
      fill in the fill-time, size and benchmark numbers.
---

# 0541 — implementation and rollout plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:executing-plans
> (or superpowers:subagent-driven-development) to run this plan task by task.
> Steps use checkbox (`- [ ]`) syntax. **Every production write, deploy and
> commit is the operator's act** — an agent stops at the step, hands over the
> exact command, and waits.

**Goal:** `soroban_events` keyed by the stellar-rpc event id, every reader and
page on that id, `transaction_id` / flat `event_index` / `soroban_event_ops`
gone — shipped in one PR and one cutover window.

**Architecture:** the parser assigns each event its rpc id once per ledger
(`xdr_parser::tx_level_event_ids` + `assign_event_ids`); staging writes it;
history is filled in ClickHouse from the old table + `soroban_event_ops` +
`transactions` into `soroban_events_staging_canonical`; a window swaps the
tables with `EXCHANGE TABLES` between two deploys.

**Tech stack:** Rust (xdr-parser, db-clickhouse, indexer, api, backfill-runner),
ClickHouse 26.3 (ReplacingMergeTree, Atomic DB), React (web), AWS CDK deploy.

**Spec:** [`../README.md`](../README.md) (design, gates, review, decisions) and
[ADR 0059](../../../../2-adrs/0059_canonical-event-identity-and-location-names.md).

## Global constraints

- Names (ADR 0059): `application_order` = transaction position (1-based);
  `operation_index` = operation (0-based); `event_index` = position in the
  operation or the fee stage counter; `transaction_index` only on event rows.
- Sentinels: `BeforeAllTxs` → transaction 0; `AfterAllTxs` → transaction
  1048575; `AfterTx` → operation 4095. rpc id string: `format!("{:019}-{:010}",
(ledger << 32) | (transaction << 12) | operation, event)`.
- History rule (fill only): charge = old `event_index` 0; refund = old
  `event_index` 1; refund stage `AfterTx` for ledger ≤ 58,762,517, `AfterAllTxs`
  from 58,762,518. Live code never uses this rule — it reads the stage.
- Native SAC surrogate `contract_id` = `-6164601581949826601`.
- Partition key `intDiv(ledger_sequence, 500000)`; fill slices of 5,000 ledgers.
- clickhouse-rs validates the row struct against `DESCRIBE`: a table column the
  struct lacks must have a DEFAULT before the struct ships; a table must exist
  before its writer deploys (0310, 0548).
- API change ⇒ `pnpm nx run @rumblefish/api-types:generate` in the same commit.
- Docs per ADR 0032 in the same PR.
- Worktree `.claude/worktrees/feat-0541_canonical-event-location` has a
  symlinked `node_modules`: before the first commit move the link to the main
  checkout's `.trash/` and run `pnpm install` inside the worktree.
- Commits only on the operator's explicit "commit"; issues as `Refs #N`; hooks
  never bypassed.
- File size and test placement (`CLAUDE.md`, task 0525): tests never inline and
  never beside the code. Rust `foo.rs` declares `#[cfg(test)] mod tests;` →
  `foo/tests.rs` (no `#[path]`); `mod.rs` uses `#[cfg(test)] #[path =
"tests/<name>_tests.rs"] mod tests;`. TS tests go to `__tests__/foo.test.ts(x)`
  importing `../foo.js`. A touched `foo_tests.rs` / `foo.test.tsx` sibling
  moves; a touched file over ~800 lines gets its tests extracted in the same
  PR. Touched here: `event.rs` (`event_tests.rs` → `event/tests.rs`),
  `value_flow.rs` (→ `value_flow/tests.rs`), `writer.rs` 824 lines
  (→ `writer/tests.rs`), `contracts/queries.rs` 1,327 lines (inline tests →
  `queries/tests.rs`; `queries_ch_tests.rs` → `queries/ch_tests.rs`),
  `transactions/queries.rs` 1,128 lines (inline → `queries/tests.rs`),
  `stellar_archive/mod.rs` (inline → `stellar_archive/tests/stellar_archive_tests.rs`),
  `common/extractors.rs` (inline → `extractors/tests.rs`), and the web tests of
  `EventsSection`, `OperationCard`, `resources` (→ `__tests__/`). `stage.rs`
  (3,285 lines) already has its tests outside; its five `stage_*_tests.rs`
  siblings are not touched and stay.

## Phases and gates

| phase | what                                   | blocks                   | who                                   |
| ----- | -------------------------------------- | ------------------------ | ------------------------------------- |
| 1     | Trial: partition 127, size + benchmark | go / no-go for 2–5       | operator writes, agent measures       |
| 2     | Code PR (tasks 2.1–2.8)                | merge needs phase 1 "go" | agent codes, operator reviews/commits |
| 3     | Fill partitions 100–128 (up to X)      | window needs all gates   | operator writes, agent gates          |
| 4     | Window                                 | —                        | operator, agent verifies              |
| 5     | Cleanup before the next Sunday backup  | —                        | operator                              |

---

## Phase 1 — trial on partition 127

### 1.1 Operator: create the table and fill partition 127

Run on a weekday, not across Sunday 03:30 UTC, from the repository root of a
checkout that has this folder:

```bash
chw "CREATE TABLE soroban_events_staging_canonical (contract_id Int64, ledger_sequence Int64, transaction_index UInt32, operation_index UInt16, event_index UInt32, application_order Int16, event_type Int16, signature LowCardinality(Nullable(String)), topics_xdr String CODEC(ZSTD(3)), data_xdr String CODEC(ZSTD(3))) ENGINE = ReplacingMergeTree PARTITION BY intDiv(ledger_sequence, 500000) ORDER BY (contract_id, ledger_sequence, transaction_index, operation_index, event_index)"
```

```bash
F=lore/1-tasks/active/0541_FEATURE_canonical-event-location/notes/fill_insert.sql; for a in $(seq -f '%.0f' 63500000 5000 63995000); do out=$(chw "$(sed -e "s/{A}/$a/g" -e "s/{B}/$((a+5000))/g" "$F")"); if printf '%s' "$out" | grep -q "DB::Exception"; then echo "FAILED at $a: $out"; break; fi; echo "ok $a"; done
```

### 1.2 Agent: size

```sql
SELECT table, column,
       formatReadableSize(sum(column_data_compressed_bytes)) AS compressed,
       round(sum(column_data_compressed_bytes) / sum(rows), 3)  AS bytes_per_row
FROM system.parts_columns
WHERE active AND partition = '127'
  AND table IN ('soroban_events', 'soroban_events_staging_canonical')
GROUP BY table, column ORDER BY table, sum(column_data_compressed_bytes) DESC
```

Pass: staging partition total < old partition total (7.19 GiB on
2026-09-16). Record per-column table in README.

### 1.3 Agent: correctness

- Rows: `SELECT count() FROM soroban_events_staging_canonical WHERE
intDiv(ledger_sequence, 500000) = 127` = 452,275,397 (pre-fill gate). If
  larger (a retried slice), operator runs `OPTIMIZE TABLE
soroban_events_staging_canonical PARTITION 127 FINAL` and the count is
  repeated.
- Distinct keys per 5k slice on the staging table equal the pre-fill gate's
  `new_keys` (same loop as the gate, reading the staging table).
- Fee rows: `countIf(transaction_index = 0)` = 170,740,563;
  `countIf(operation_index = 4095)` = 0; `countIf(transaction_index = 1048575)`
  = 78,294,908.

### 1.4 Agent: read-path benchmark

Pick contracts (read-only):

```sql
SELECT contract_id, count() AS n FROM soroban_events
WHERE intDiv(ledger_sequence, 500000) = 127
GROUP BY contract_id ORDER BY n DESC LIMIT 3
```

Use the top one (native SAC expected), the third, and one with ~1,000 events
(`HAVING n BETWEEN 900 AND 1100 LIMIT 1`). Every query carries
`SETTINGS log_comment = 'bench-0541-<case>-<old|new>'`; after the run read
`query_duration_ms, read_rows, memory_usage` from `system.query_log` by
`log_comment`. Each case runs 3 times, the median counts.

| case                 | old (production shape)                                                                                                                                                                                                                                                                                    | new                                                                                                                                                                                                                                                                                                                                                                                                             |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `events-first`       | `SELECT ledger_sequence, transaction_id, event_index, event_type, topics_xdr, data_xdr FROM soroban_events WHERE contract_id = {C} AND ledger_sequence <= 63999999 ORDER BY ledger_sequence DESC, transaction_id DESC, event_index DESC LIMIT 1 BY ledger_sequence, transaction_id, event_index LIMIT 21` | `SELECT ledger_sequence, transaction_index, operation_index, event_index, application_order, event_type, topics_xdr, data_xdr FROM soroban_events_staging_canonical WHERE contract_id = {C} AND ledger_sequence <= 63999999 ORDER BY ledger_sequence DESC, transaction_index DESC, operation_index DESC, event_index DESC LIMIT 1 BY ledger_sequence, transaction_index, operation_index, event_index LIMIT 21` |
| `events-cursor`      | same + `AND (ledger_sequence, transaction_id, event_index) < ({L}, {T}, {E})` from the middle of the partition                                                                                                                                                                                            | same + `AND (ledger_sequence, transaction_index, operation_index, event_index) < ({L}, {TI}, {OI}, {EI})` for the same event                                                                                                                                                                                                                                                                                    |
| `events-resolve`     | `SELECT id, lower(hex(hash)), successful FROM transactions WHERE ledger_sequence IN ({ledgers of the page}) AND id IN ({ids of the page}) LIMIT 1 BY id`                                                                                                                                                  | `SELECT ledger_sequence, application_order, lower(hex(hash)), successful FROM transactions WHERE (ledger_sequence, application_order) IN ({pairs of the page}) LIMIT 1 BY ledger_sequence, application_order`                                                                                                                                                                                                   |
| `tx-events`          | `SELECT contract_id, ledger_sequence FROM soroban_events FINAL WHERE transaction_id = {T} AND ledger_sequence = {L} AND intDiv(ledger_sequence, 500000) = 127 GROUP BY contract_id, ledger_sequence`                                                                                                      | `SELECT contract_id, ledger_sequence FROM soroban_events_staging_canonical WHERE ledger_sequence = {L} AND application_order = {A} AND intDiv(ledger_sequence, 500000) = 127 GROUP BY contract_id, ledger_sequence`                                                                                                                                                                                             |
| `tx-by-contract-arm` | `SELECT ledger_sequence, transaction_id FROM soroban_events WHERE contract_id = {C} AND intDiv(ledger_sequence, 500000) = 127`                                                                                                                                                                            | `SELECT t.ledger_sequence, t.id FROM transactions t WHERE intDiv(t.ledger_sequence, 500000) = 127 AND (t.ledger_sequence, t.application_order) IN (SELECT ledger_sequence, application_order FROM soroban_events_staging_canonical WHERE contract_id = {C} AND intDiv(ledger_sequence, 500000) = 127)`                                                                                                          |

Pass: for every case and contract, new `query_duration_ms` ≤ 1.2 × old,
`read_rows` ≤ old, `memory_usage` < 3.5 GiB (the `api_reader` cap is 4 GB).
**`tx-by-contract-arm` on the top contract is the known risk** (an `IN` set of
every native-XLM event in the partition). If it fails, task 2.5 changes: the
contract-filtered transaction list moves to positions in this PR (its driver
orders by `(ledger_sequence, application_order)` and resolves ids last), and
the benchmark is repeated for that shape before phase 2 merges.

### 1.5 Operator decision

Go → phase 2 may merge and phase 3 may start. No-go → `DROP TABLE
soroban_events_staging_canonical`, README records why, design revisited.

---

## Phase 2 — code (branch `feat/0541_canonical-event-location`)

### Task 2.1: rpc event id in the parser

**Files:**

- Modify: `crates/xdr-parser/src/types.rs` (`ExtractedEvent` + field; flat
  `event_index` renamed `position_in_tx`)
- Modify: `crates/xdr-parser/src/event.rs` (id functions; `event_id: None` in
  `extract_single_event`)
- Modify: `crates/xdr-parser/src/lib.rs` (re-exports)
- Modify: every `ExtractedEvent { … }` literal and `.event_index` read in
  `crates/xdr-parser/src/{nft.rs,asset_transfers.rs}`, their tests,
  `crates/db-clickhouse/src/persist/*` and `crates/api/src/runtime_enrichment/**`
  (compiler-guided rename)
- Move + test: `crates/xdr-parser/src/event_tests.rs` →
  `crates/xdr-parser/src/event/tests.rs` (`git mv`; `event.rs` ends with
  `#[cfg(test)] mod tests;`)

**Interfaces — produces:**

```rust
pub struct EventId { pub ledger_sequence: u32, pub transaction_index: u32,
                     pub operation_index: u16, pub event_index: u32 }
impl EventId {
    pub const BEFORE_ALL_TXS: u32;          // 0
    pub const AFTER_ALL_TXS: u32;           // 1_048_575
    pub const AFTER_TX_OPERATION: u16;      // 4_095
    pub fn toid(&self) -> u64;
    pub fn to_rpc_string(&self) -> String;
}
pub fn tx_level_event_ids(ledger_sequence: u32, tx_metas: &[&TransactionMeta]) -> Vec<Vec<EventId>>;
pub fn assign_event_ids(ledger_sequence: u32, application_order: u32,
                        tx_level: &[EventId], events: &mut [ExtractedEvent]);
// ExtractedEvent gains: pub event_id: Option<EventId>
// ExtractedEvent.event_index → position_in_tx (in-memory ordinal, never stored)
```

- [x] **Step 0:** `git mv crates/xdr-parser/src/event_tests.rs
crates/xdr-parser/src/event/tests.rs`, replace the `#[path]` declaration
      with `#[cfg(test)] mod tests;`, `cargo test -p xdr-parser` → still green.
- [x] **Step 1: failing tests** (append to `event/tests.rs`, reuse
      `make_contract_event` / `make_v4_meta`)

```rust
fn tx_event(stage: TransactionEventStage) -> TransactionEvent {
    TransactionEvent { stage, event: make_contract_event(0xAA, 1) }
}
fn op_with(n: u32) -> OperationMetaV2 {
    OperationMetaV2 {
        ext: ExtensionPoint::V0,
        changes: LedgerEntryChanges::default(),
        events: (0..n).map(|i| make_contract_event(0xB0, i)).collect::<Vec<_>>().try_into().unwrap(),
    }
}

#[test]
fn rpc_string_matches_getevents_format() {
    // Ledger 64,450,000 charge seen on mainnet getEvents: "0276810642227200000-0000000000".
    let id = EventId { ledger_sequence: 64_450_000, transaction_index: 0, operation_index: 0, event_index: 0 };
    assert_eq!(id.to_rpc_string(), "0276810642227200000-0000000000");
}

#[test]
fn fee_events_take_rpc_sentinels_and_ledger_counters() {
    use TransactionEventStage::*;
    // tx1: charge + end-of-ledger refund; tx2: charge only; tx3: charge + refund.
    let m1 = make_v4_meta(vec![tx_event(BeforeAllTxs), tx_event(AfterAllTxs)], vec![op_with(2)], vec![]);
    let m2 = make_v4_meta(vec![tx_event(BeforeAllTxs)], vec![], vec![]);
    let m3 = make_v4_meta(vec![tx_event(BeforeAllTxs), tx_event(AfterAllTxs)], vec![], vec![]);
    let ids = tx_level_event_ids(7, &[&m1, &m2, &m3]);
    let t = |tx, op, ev| EventId { ledger_sequence: 7, transaction_index: tx, operation_index: op, event_index: ev };
    assert_eq!(ids[0], vec![t(0, 0, 0), t(1_048_575, 0, 0)]);
    assert_eq!(ids[1], vec![t(0, 0, 1)]);
    assert_eq!(ids[2], vec![t(0, 0, 2), t(1_048_575, 0, 1)]);
}

#[test]
fn pre_23_refund_is_after_its_own_transaction() {
    use TransactionEventStage::*;
    let m1 = make_v4_meta(vec![tx_event(BeforeAllTxs), tx_event(AfterTx)], vec![], vec![]);
    let m2 = make_v4_meta(vec![tx_event(BeforeAllTxs), tx_event(AfterTx)], vec![], vec![]);
    let ids = tx_level_event_ids(9, &[&m1, &m2]);
    assert_eq!(ids[1][1], EventId { ledger_sequence: 9, transaction_index: 2, operation_index: 4_095, event_index: 0 });
}

#[test]
fn assign_sets_operation_events_and_leaves_diagnostics_without_id() {
    use TransactionEventStage::*;
    let diag = DiagnosticEvent { in_successful_contract_call: true, event: make_contract_event(0xDD, 9) };
    let meta = make_v4_meta(vec![tx_event(BeforeAllTxs)], vec![op_with(1), op_with(2)], vec![diag]);
    let tx_level = tx_level_event_ids(5, &[&meta]);
    let mut events = extract_events(&meta, "abc", 5, 0);
    assign_event_ids(5, 1, &tx_level[0], &mut events);
    let ids: Vec<_> = events.iter().map(|e| e.event_id).collect();
    let t = |tx, op, ev| Some(EventId { ledger_sequence: 5, transaction_index: tx, operation_index: op, event_index: ev });
    assert_eq!(ids, vec![t(0, 0, 0), t(1, 0, 0), t(1, 1, 0), t(1, 1, 1), None]);
}

#[test]
fn a_transaction_level_event_without_a_stage_gets_no_id() {
    // V3 meta carries no stage; staging refuses the row (ADR 0059 §5).
    let meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges::default(),
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: Some(SorobanTransactionMeta {
            ext: SorobanTransactionMetaExt::V0,
            events: vec![make_contract_event(0xAA, 1)].try_into().unwrap(),
            return_value: ScVal::Void,
            diagnostic_events: VecM::default(),
        }),
    });
    let mut events = extract_events(&meta, "abc", 5, 0);
    assert_eq!(events.len(), 1);
    assign_event_ids(5, 1, &tx_level_event_ids(5, &[&meta])[0], &mut events);
    assert_eq!(events[0].event_id, None);
}
```

- [x] **Step 2:** `cargo test -p xdr-parser event::tests` → fails to compile
      (`EventId` undefined).

- [x] **Step 3: implementation** in `event.rs` (below `extract_single_event`)

```rust
/// stellar-rpc's identity for a non-diagnostic event (ADR 0059): a TOID
/// (SEP-35 layout) plus an event number. Source: stellar-rpc
/// `internal/db/event.go`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId {
    pub ledger_sequence: u32,
    pub transaction_index: u32,
    pub operation_index: u16,
    pub event_index: u32,
}

impl EventId {
    /// Transaction part of a `BeforeAllTxs` event.
    pub const BEFORE_ALL_TXS: u32 = 0;
    /// Transaction part of an `AfterAllTxs` event: the largest 20-bit value.
    pub const AFTER_ALL_TXS: u32 = (1 << 20) - 1;
    /// Operation part of an `AfterTx` event: the largest 12-bit value.
    pub const AFTER_TX_OPERATION: u16 = (1 << 12) - 1;

    pub fn toid(&self) -> u64 {
        (u64::from(self.ledger_sequence) << 32)
            | (u64::from(self.transaction_index) << 12)
            | u64::from(self.operation_index)
    }

    /// The `id` string `getEvents` returns.
    pub fn to_rpc_string(&self) -> String {
        format!("{:019}-{:010}", self.toid(), self.event_index)
    }
}

/// Ids of every transaction-level event of a ledger, per transaction, in
/// `TransactionMetaV4.events` order. `tx_metas[i]` is application order
/// `i + 1`. Counters come from the meta itself, so a transaction whose events
/// were not extracted still advances them. Non-V4 metas have none.
pub fn tx_level_event_ids(ledger_sequence: u32, tx_metas: &[&TransactionMeta]) -> Vec<Vec<EventId>> {
    let mut before_all = 0u32;
    let mut after_all = 0u32;
    let mut out = Vec::with_capacity(tx_metas.len());
    for (i, meta) in tx_metas.iter().enumerate() {
        let application_order = u32::try_from(i + 1).expect("transactions per ledger fit u32");
        let mut ids = Vec::new();
        if let TransactionMeta::V4(v4) = meta {
            let mut after_tx = 0u32;
            for event in v4.events.iter() {
                let (transaction_index, operation_index, counter) = match event.stage {
                    TransactionEventStage::BeforeAllTxs => (EventId::BEFORE_ALL_TXS, 0, &mut before_all),
                    TransactionEventStage::AfterTx => (application_order, EventId::AFTER_TX_OPERATION, &mut after_tx),
                    TransactionEventStage::AfterAllTxs => (EventId::AFTER_ALL_TXS, 0, &mut after_all),
                };
                ids.push(EventId { ledger_sequence, transaction_index, operation_index, event_index: *counter });
                *counter += 1;
            }
        }
        out.push(ids);
    }
    out
}

/// Sets `event_id` on one transaction's `extract_events` output. `tx_level` is
/// that transaction's entry from [`tx_level_event_ids`]. Diagnostic events,
/// per-operation events without a position and transaction-level events
/// without a matching id keep `None`; staging refuses those rows.
pub fn assign_event_ids(
    ledger_sequence: u32,
    application_order: u32,
    tx_level: &[EventId],
    events: &mut [ExtractedEvent],
) {
    let mut tx_level = tx_level.iter().copied();
    for event in events.iter_mut() {
        event.event_id = match event.source {
            EventSource::Diagnostic => None,
            EventSource::TxLevel if event.stage.is_some() => tx_level.next(),
            EventSource::TxLevel => None,
            EventSource::PerOp => match (event.op_index, event.event_pos_in_op) {
                (Some(op), Some(pos)) => u16::try_from(op).ok().map(|operation_index| EventId {
                    ledger_sequence,
                    transaction_index: application_order,
                    operation_index,
                    event_index: pos,
                }),
                _ => None,
            },
        };
    }
}
```

`lib.rs`: `pub use event::{EventId, assign_event_ids, extract_events, tx_level_event_ids};`

- [x] **Step 4:** add `pub event_id: Option<EventId>` to `ExtractedEvent`
      (doc: "stellar-rpc id, set by `assign_event_ids`; `None` for diagnostic
      events"), `event_id: None` in `extract_single_event`, rename the flat
      field to `position_in_tx` (doc: "ordinal across all containers of the
      transaction; in memory only — never stored, never on the wire"), fix
      every literal/read the compiler lists.
- [x] **Step 5:** `cargo test -p xdr-parser` → all pass, including
      `tests/tx_event_stage_real_meta.rs`.
- [ ] **Step 6: commit** (on "commit"):
      `feat(lore-0541): assign the stellar-rpc event id in the parser`

### Task 2.2: indexer assigns ids for every ledger

**Files:** Modify `crates/indexer/src/handler/process.rs:175-230`.
**Consumes:** 2.1.

- [x] **Step 1: failing test** in a new `crates/indexer/src/handler/process/tests.rs`
      (`process.rs` gains `#[cfg(test)] mod tests;`), on a real ledger fixture:
      every non-diagnostic event in `ParseOutput.events` has
      `event_id.is_some()`, and the ids of one ledger are unique.

```rust
#[test]
fn every_consensus_event_of_a_real_ledger_has_a_unique_rpc_id() {
    let batch = include_bytes!("../../../tests/fixtures/ledger_58816920.xdr.zst");
    let xdr = zstd::decode_all(&batch[..]).expect("zstd");
    let batch = stellar_xdr::LedgerCloseMetaBatch::from_xdr(&xdr, stellar_xdr::Limits::none()).expect("batch");
    let meta = batch.ledger_close_metas[0].clone();
    let out = parse_ledger(&meta);
    let ids: Vec<_> = out.events.iter().flat_map(|(_, evs)| evs)
        .filter(|e| e.source != xdr_parser::EventSource::Diagnostic)
        .map(|e| e.event_id.expect("consensus event without id"))
        .collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len());
}
```

(Fixture `crates/indexer/tests/fixtures/ledger_58816920.xdr.zst`: the zstd
`LedgerCloseMetaBatch` of ledger 58,816,920 — protocol 23, 254 transactions,
157 end-of-ledger refunds — downloaded from
`s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/FC7E89FF--58816000-58879999/FC7E8667--58816920.xdr.zst`
(unsigned request, 131 KB). Add `zstd` / `stellar-xdr` to the indexer's
dev-dependencies only if they are not already direct dependencies.)

- [x] **Step 2:** run → fails (`event_id` is `None`).
- [x] **Step 3: implementation**

```rust
    let tx_level_ids = xdr_parser::tx_level_event_ids(ledger_sequence, &tx_metas);
    // … inside the per-transaction loop, replacing the extract_events line:
        if let Some(tm) = tx_meta {
            let mut events = xdr_parser::extract_events(tm, &ext_tx.hash, ledger_sequence, closed_at);
            let application_order = u32::try_from(tx_index + 1).expect("transactions per ledger fit u32");
            xdr_parser::assign_event_ids(
                ledger_sequence,
                application_order,
                tx_level_ids.get(tx_index).map_or(&[][..], Vec::as_slice),
                &mut events,
            );
```

- [x] **Step 4:** `cargo test -p indexer` → pass.
- [ ] **Step 5: commit:** `feat(lore-0541): give every parsed event its rpc id`

### Task 2.3: staging writes the new `soroban_events` and stops `soroban_event_ops`

**Files:**

- Modify: `crates/db-clickhouse/src/persist/rows.rs` (`SorobanEventRow`;
  delete `SorobanEventOpRow`; `AssetTransferRow` loses `event_index`)
- Modify: `crates/db-clickhouse/src/persist/stage.rs` (events block ~1880–1920;
  `app_order_by_hash` next to `tx_id_by_hash` ~1044; delete `event_op_rows`)
- Modify: `crates/db-clickhouse/src/persist/value_flow.rs` (delete the
  `event_ops` loop and field; drop `event_index` from `AssetTransferRow`)
- Modify: `crates/db-clickhouse/src/persist/writer.rs` (delete
  `soroban_event_ops` from `TARGETABLE`, the `event_ops` insert and its arms)
- Tests: `persist/tests_cross.rs`; `value_flow_tests.rs` →
  `value_flow/tests.rs` and `writer_tests.rs` → `writer/tests.rs` (`git mv`,
  `#[cfg(test)] mod tests;` in place of `#[path]`, first step, green before
  edits); `tests/lp_amounts_targeted_write_e2e.rs`, `tests/smoke.rs`
  **Consumes:** 2.1 (`ExtractedEvent.event_id`).

**Produces:**

```rust
pub struct SorobanEventRow {            // column order = DDL
    pub contract_id: i64,
    pub ledger_sequence: i64,
    pub transaction_index: u32,
    pub operation_index: u16,
    pub event_index: u32,
    pub application_order: i16,
    pub event_type: i16,
    pub signature: Option<String>,
    pub topics_xdr: String,
    pub data_xdr: String,
}
```

- [x] **Step 1: failing tests** in `tests_cross.rs`

```rust
#[test]
fn staged_events_carry_the_rpc_id_and_their_transaction() {
    // two transactions; tx1 charge + op event + end-of-ledger refund, tx2 charge
    let staged = stage_fixture_ledger_with_fee_events(); // builds ExtractedEvent with event_id set
    let rows: Vec<_> = staged.event_rows.iter()
        .map(|r| (r.transaction_index, r.operation_index, r.event_index, r.application_order))
        .collect();
    assert!(rows.contains(&(0, 0, 0, 1)));          // tx1 charge
    assert!(rows.contains(&(1, 0, 0, 1)));          // tx1 op event
    assert!(rows.contains(&(1_048_575, 0, 0, 1)));  // tx1 refund
    assert!(rows.contains(&(0, 0, 1, 2)));          // tx2 charge
}

#[test]
fn a_consensus_event_without_an_id_is_a_staging_error() {
    let err = stage_fixture_ledger_with_event_id(None).unwrap_err();
    assert!(err.to_string().contains("without a stellar-rpc id"));
}
```

(`stage_fixture_ledger_with_fee_events` / `…_with_event_id` follow the
builders already used by the event tests at `tests_cross.rs:660-730`.)

- [x] **Step 2:** `cargo test -p db-clickhouse tests_cross` → fails.
- [x] **Step 3: implementation** — in the transactions loop:
      `app_order_by_hash.insert(tx.hash.clone(), app_order);` — events block:

```rust
    for (tx_hash, evs) in events {
        let Some(&application_order) = app_order_by_hash.get(tx_hash) else {
            continue;
        };
        for ev in evs {
            if is_diagnostic(ev.source) {
                diagnostic_dropped += 1;
                continue;
            }
            let Some(contract_strkey) = &ev.contract_id else {
                contract_orphan_dropped += 1;
                continue;
            };
            let Some(id) = ev.event_id else {
                return Err(staging_err(&format!(
                    "event without a stellar-rpc id (tx {tx_hash}, source {:?}) — ADR 0059",
                    ev.source
                )));
            };
            let topics_xdr = serde_json::to_string(&ev.topics)
                .map_err(|e| staging_err(&format!("event topics serialize: {e}")))?;
            let data_xdr = serde_json::to_string(&ev.data)
                .map_err(|e| staging_err(&format!("event data serialize: {e}")))?;
            out.event_rows.push(SorobanEventRow {
                contract_id: ids::contract_id(contract_strkey),
                ledger_sequence: ledger_sequence_i64,
                transaction_index: id.transaction_index,
                operation_index: id.operation_index,
                event_index: id.event_index,
                application_order,
                event_type: ev.event_type as i16,
                signature: extract_event_signature(&ev.topics),
                topics_xdr,
                data_xdr,
            });
        }
    }
```

Delete `event_op_rows` (field, assignment at ~2543, `value_flow` loop and
struct field), `SorobanEventOpRow`, the writer's `event_ops` insert slot and
`"soroban_event_ops"` in `TARGETABLE`. `AssetTransferRow` and its builder lose
`event_index`; the staging error message in `value_flow.rs:118` names
`op_index`/`event_pos_in_op` instead.

- [x] **Step 4:** update the listed tests (drop `soroban_event_ops` from the
      targeted-write e2e and `TargetedTables::parse` cases; `smoke.rs`
      `INSERT INTO soroban_events (contract_id, ledger_sequence,
transaction_index, operation_index, event_index, application_order,
event_type, signature, topics_xdr, data_xdr)`).
- [x] **Step 5:** `cargo test -p db-clickhouse` (CH-gated tests against the
      repo docker ClickHouse, with the schema from task 2.8 applied) → pass.
- [ ] **Step 6: commit:** `feat(lore-0541): stage soroban_events by rpc event id`

### Task 2.4: contract events endpoint

**Files:** `crates/api/src/contracts/{dto.rs,queries.rs,handlers.rs}`.

**Produces (wire):** `EventItem { id: String, transaction_hash, ledger_sequence,
successful, created_at, event_type, topics, data }` (`transaction_id` removed);
cursor `EventCursor::ChEventId { ledger_sequence: i64, transaction_index: u32,
operation_index: u16, event_index: u32 }` (serde tag `ch_event_id` — a cursor
minted before the deploy no longer decodes and gets the existing 400
`invalid_cursor`).

- [x] **Step 0:** extract `contracts/queries.rs` tests (1,327 lines): the inline
      `mod tests { … }` → `crates/api/src/contracts/queries/tests.rs`,
      `queries_ch_tests.rs` → `queries/ch_tests.rs`; declarations
      `#[cfg(test)] mod tests;` / `#[cfg(test)] mod ch_tests;`; same for
      `common/extractors.rs` (inline → `common/extractors/tests.rs`).
      `cargo test -p api` → green before any change.
- [x] **Step 1: failing tests** (`contracts/queries/tests.rs`)

```rust
fn event_row(event_type: i16, topics_xdr: &str, data_xdr: &str) -> EventChRow {
    EventChRow {
        ledger_sequence: 64_450_000, transaction_index: 0, operation_index: 0, event_index: 0,
        event_type, topics_xdr: topics_xdr.into(), data_xdr: data_xdr.into(),
        transaction_hash: "deadbeef".into(), successful: true, created_at: 1_700_000_000_000,
    }
}

#[test]
fn map_event_row_exposes_the_rpc_id() {
    let ev = map_event_row(event_row(1, r#"[{"type":"sym","value":"fee"}]"#, r#"{"type":"i128","value":"100"}"#));
    assert_eq!(ev.item.id, "0276810642227200000-0000000000");
    assert_eq!((ev.transaction_index, ev.operation_index, ev.event_index), (0, 0, 0));
}

#[test]
fn events_page_orders_and_seeks_on_the_rpc_id() {
    let sql = events_page_sql(Some(&EventCursor::ChEventId {
        ledger_sequence: 1, transaction_index: 2, operation_index: 3, event_index: 4 }), Direction::Next);
    assert!(sql.contains("(se.ledger_sequence, se.transaction_index, se.operation_index, se.event_index) < (1, 2, 3, 4)"));
    assert!(sql.contains("ORDER BY se.ledger_sequence DESC, se.transaction_index DESC, se.operation_index DESC, se.event_index DESC"));
    assert!(!sql.contains("transaction_id"));
}
```

plus in `common/extractors/tests.rs`: a base64 cursor of the old
`{"src":"ch","ledger_sequence":1,"transaction_id":2,"event_index":3}` shape
returns 400 `invalid_cursor` for `Pagination<EventCursor>`.

- [x] **Step 2:** `cargo test -p api contracts` → fails.
- [x] **Step 3: implementation** — extract `events_page_sql(cursor, direction)
-> String` from `fetch_events` (so the test sees the real SQL):

```rust
fn events_page_sql(cursor: Option<&EventCursor>, direction: Direction) -> String {
    let (op, order) = keyset_sql_desc(direction);
    let cursor_clause = match cursor {
        Some(EventCursor::ChEventId { ledger_sequence, transaction_index, operation_index, event_index }) => format!(
            " AND (se.ledger_sequence, se.transaction_index, se.operation_index, se.event_index) {op} \
             ({ledger_sequence}, {transaction_index}, {operation_index}, {event_index})"
        ),
        None => String::new(),
    };
    format!(
        "SELECT se.ledger_sequence AS ledger_sequence, se.transaction_index AS transaction_index, \
                se.operation_index AS operation_index, se.event_index AS event_index, \
                se.application_order AS application_order, se.event_type AS event_type, \
                se.topics_xdr AS topics_xdr, se.data_xdr AS data_xdr \
         FROM soroban_events se \
         WHERE se.contract_id = ? AND se.ledger_sequence <= (SELECT max(sequence) FROM ledgers){cursor_clause} \
         ORDER BY se.ledger_sequence {order}, se.transaction_index {order}, se.operation_index {order}, se.event_index {order} \
         LIMIT 1 BY se.ledger_sequence, se.transaction_index, se.operation_index, se.event_index \
         LIMIT ?"
    )
}
```

Step 2 of `fetch_events` resolves transactions by position:

```rust
    let pairs = { let mut v: Vec<(i64, i16)> = raw.iter().map(|r| (r.ledger_sequence, r.application_order)).collect(); v.sort_unstable(); v.dedup(); v };
    let in_pairs = pairs.iter().map(|(l, a)| format!("({l},{a})")).collect::<Vec<_>>().join(",");
    // SELECT t.ledger_sequence, t.application_order, lower(hex(t.hash)) AS hash, t.successful, l.closed_at AS created_at
    // FROM transactions t INNER JOIN ledgers l ON l.sequence = t.ledger_sequence
    // WHERE (t.ledger_sequence, t.application_order) IN ({in_pairs})
    // LIMIT 1 BY t.ledger_sequence, t.application_order
```

`map_event_row` builds `id` with `xdr_parser::EventId { … }.to_rpc_string()`
(`ledger_sequence` narrowed with `u32::try_from`); `ChEvent` carries the three
id parts for the cursor; the handler encodes `EventCursor::ChEventId`;
`event_cursor_matches_source` matches the new variant. Update the doc comments
that describe `(ledger_sequence, transaction_id, event_index)`.

- [x] **Step 4:** `cargo test -p api` → pass.
- [x] **Step 5:** `pnpm nx run @rumblefish/api-types:generate`.
- [ ] **Step 6: commit:** `feat(lore-0541): page contract events on the rpc event id`

### Task 2.5: transaction queries that read `soroban_events`

> Superseded 2026-09-21 for the contract-filtered list: the bounded windows below
> were replaced by a seek on the `contract_transactions` presence index. The
> window's cap counted rows while the page counts transactions, so a contract
> with many events per transaction got a page that read as the end of its list.
> See [S-review-and-contract-transactions](S-review-and-contract-transactions.md).
> `fetch_event_appearances` stands as written.

**Phase 1 changed this task.** Mapping the events arm to transaction ids through
an `IN` set was measured unusable (native XLM: 6.64 GiB, over the 4 GB cap;
mid contract: 4,331 ms vs 29 ms). Today's full driver already fails for native
XLM (6.04 GiB). So the contract-filtered transaction list moves to positions in
this PR: each arm yields `(ledger_sequence, application_order)` inside a
bounded ledger window, the arms are merged in Rust, and the page is in
execution order. The operation-type and unfiltered lists (statements A, C) stay
as they are (programme 0538).

**Files:** `crates/api/src/transactions/queries.rs` (statement B ~450–530,
`fetch_event_appearances` ~965–1000), create
`crates/api/src/transactions/contract_positions.rs` (window queries + merge; its
tests in `contract_positions/tests.rs`), `transactions/dto.rs`
(`TxListCursor`), `transactions/handlers.rs` (cursor per statement, `:500`).

**Produces:**

```rust
// dto.rs — B's cursor; A/C keep `Ch`. Each statement rejects the other variant with 400.
pub enum TxListCursor {
    Ch { ledger_sequence: i64, tiebreak: i64 },
    ChPosition { ledger_sequence: i64, application_order: i16 },
}
// contract_positions.rs
pub struct ArmWindow { pub positions: Vec<(i64, i16)>, pub truncated: bool, pub last_ledger: Option<i64> }
pub fn merge_arm_windows(arms: &[ArmWindow], direction: Direction, take: usize) -> MergeResult;
pub enum MergeResult { Page(Vec<(i64, i16)>), NeedWiderWindow }
pub async fn contract_tx_positions(client: &clickhouse::Client, contract_id: i64, partition_expr: &str,
    head_max: &str, cursor: Option<(i64, i16)>, direction: Direction, take: usize)
    -> Result<Vec<(i64, i16)>, clickhouse::error::Error>;
```

**Merge rule (the correctness condition).** An arm fetches the first
`window` rows by ledger past the cursor, then **every** transaction position of
the ledgers that window reached. A truncated arm is complete only down to its
`last_ledger`, so the merged page may keep only positions on the complete side
of the most restrictive truncated arm (`max` of `last_ledger` for a descending
page, `min` for ascending). If fewer than `take` positions remain and some arm
was truncated, the window doubles and the arms are asked again (at most 6
rounds — `ponytail:` bounded; a contract needing more is logged and returns the
positions it has).

- [x] **Step 0:** extract the inline tests of `transactions/queries.rs`
      (1,128 lines) → `crates/api/src/transactions/queries/tests.rs`;
      `cargo test -p api transactions` → green.
- [x] **Step 1: failing tests** (`transactions/contract_positions/tests.rs`)

```rust
fn arm(p: &[(i64, i16)], truncated: bool) -> ArmWindow {
    ArmWindow { positions: p.to_vec(), truncated, last_ledger: p.iter().map(|x| x.0).min() }
}

#[test]
fn merges_in_execution_order_and_dedups() {
    let r = merge_arm_windows(&[arm(&[(10, 3), (10, 1)], false), arm(&[(10, 3), (9, 7)], false)], Direction::Next, 10);
    assert_eq!(r, MergeResult::Page(vec![(10, 3), (10, 1), (9, 7)])); // descending page
}

#[test]
fn a_truncated_arm_caps_the_page_at_its_last_complete_ledger() {
    // arm 1 reached only ledger 10; arm 2 has rows at ledger 8, which arm 1 may also have
    let r = merge_arm_windows(&[arm(&[(11, 2), (10, 5)], true), arm(&[(8, 1)], false)], Direction::Next, 2);
    assert_eq!(r, MergeResult::Page(vec![(11, 2), (10, 5)]));
}

#[test]
fn asks_for_a_wider_window_when_the_cap_leaves_too_few() {
    let r = merge_arm_windows(&[arm(&[(11, 2)], true), arm(&[(8, 1), (7, 1)], false)], Direction::Next, 3);
    assert_eq!(r, MergeResult::NeedWiderWindow);
}
```

plus SQL-shape tests in `queries/tests.rs`: no statement-B SQL names
`transaction_id` for `soroban_events`; the page seeks
`(t.ledger_sequence, t.application_order) IN (…)` and orders by
`t.ledger_sequence {order}, t.application_order {order}`;
`event_appearances_sql()` filters `se.ledger_sequence = ? AND
se.application_order = ?`; and in `dto` tests a `ChPosition` cursor round-trips
while a `Ch` cursor on statement B returns 400 `invalid_cursor`.

- [x] **Step 2:** `cargo test -p api transactions` → fails.
- [x] **Step 3: implementation**
  - `merge_arm_windows`: collect all positions, dedup, drop those beyond the
    cap (`max`/`min` of `last_ledger` over truncated arms), sort by
    `(ledger, application_order)` in the page direction; `Page` if `len ≥ take`
    or no arm was truncated, else `NeedWiderWindow`.
  - window queries per arm (`{p}` partition expression, `{cur}` = `NULL` or the
    cursor, `{op}`/`{order}` from `keyset_sql_desc`, `{w}` window): - events: `SELECT ledger_sequence, application_order FROM soroban_events
WHERE contract_id = {cid} AND {p} AND ledger_sequence <= {head_max} AND
({cur} IS NULL OR (ledger_sequence, application_order) {op} {cur})
AND ledger_sequence {op}= (SELECT … ORDER BY ledger_sequence {order} LIMIT
1 OFFSET {w}-1)` — implemented as two statements: the window's last ledger
    (`SELECT ledger_sequence … ORDER BY ledger_sequence {order} LIMIT 1
OFFSET {w}-1`, read-in-order on the key), then `SELECT DISTINCT
ledger_sequence, application_order … AND ledger_sequence BETWEEN …`. - invocations (`soroban_invocations_appearances`, key `(contract_id,
ledger_sequence, transaction_id)`) and operations
    (`operations_appearances`, contract filter not in its key — scans the
    partition as today): same two statements on `(ledger_sequence,
transaction_id)`, then ids → positions with one seek `SELECT
ledger_sequence, id, application_order FROM transactions WHERE
(ledger_sequence, id) IN (…)`; the cursor predicate on positions is
    applied after that mapping, and the window's first ledger is inclusive.
  - statement B page: `… FROM transactions t WHERE (t.ledger_sequence,
t.application_order) IN ({positions}) AND {src/op_type filters} ORDER BY
t.ledger_sequence {order}, t.application_order {order} LIMIT 1 BY
t.ledger_sequence, t.application_order LIMIT {lim_peek}`; the handler encodes
    `TxListCursor::ChPosition` for statement B.
  - `fetch_event_appearances(client, ledger_sequence: i64, application_order: i16)`:
    `FROM soroban_events se JOIN ledgers l … WHERE se.ledger_sequence = ? AND
se.application_order = ? AND intDiv(se.ledger_sequence, 500000) =
intDiv(?, 500000) GROUP BY se.contract_id, se.ledger_sequence` (no `FINAL`:
    the `GROUP BY` already collapses duplicates); the handler passes
    `tx.ledger_sequence, tx.application_order`.
- [x] **Step 4:** `cargo test -p api transactions` → pass.
- [x] **Step 5: measure before merge** — local API against production
      ClickHouse (read-only; the staging table has partition 127 only, so run
      against a build whose SQL names `soroban_events_staging_canonical` and
      ledgers of partition 127): first page and one cursor page of the
      contract-filtered list for native XLM, contract `546855837558613593` and
      `5314455185855296541`. Pass: each request < 1 s and < 1 GiB in
      `system.query_log`.
- [ ] **Step 6: commit:** `feat(lore-0541): page the contract transaction list on positions`

### Task 2.6: transaction page events from the archive carry the rpc id

**Files:** `crates/api/src/runtime_enrichment/stellar_archive/{extractors.rs,dto.rs,mod.rs}`.

**Produces (wire):** `XdrEventDto { event_type, contract_id, topics, data,
id: Option<String>, operation_index: Option<i16>, event_index: Option<u32>,
stage: Option<String> }` — `id`/`event_index` `None` for diagnostic events;
`op_index` renamed `operation_index`; the flat per-transaction number removed.
Delete the unused `extract_e14_heavy`, `E14HeavyEventFields` and their test
(`mod.rs:407`), the last producer of the flat number.

- [x] **Step 0:** `stellar_archive/mod.rs` inline tests →
      `stellar_archive/tests/stellar_archive_tests.rs` via
      `#[cfg(test)] #[path = "tests/stellar_archive_tests.rs"] mod tests;`
      (minus the deleted E14 test); `cargo test -p api stellar_archive` → green.
- [x] **Step 1: failing test** in a new `stellar_archive/extractors/tests.rs`
      (`extractors.rs` gains `#[cfg(test)] mod tests;`) using the real
      `tx_0a120260` meta (ledger 62,032,880, one KALE transfer): the refund's
      `id` has transaction part 1048575, the charge's has transaction 0, the
      transfer's transaction part equals its application order, and no
      diagnostic event has an `id`. (The fixture holds one transaction's meta,
      so the charge/refund counters are checked for their sentinel parts only;
      counters are covered by 2.1.) The same test asserts `contract_events`
      come back in execution order: sorted by `id` (fixed-width strings, so
      string order = numeric order) — charge, operation events, refund.
- [x] **Step 2:** run → fails.
- [x] **Step 3: implementation** — `extract_e3_heavy` computes
      `let tx_level = xdr_parser::tx_level_event_ids(ledger_seq, &tx_metas);`
      once, then `assign_event_ids(ledger_seq, (idx + 1) as u32,
&tx_level[idx], &mut events)` before `split_events`; `split_events` maps
      `id: e.event_id.map(|i| i.to_rpc_string())`, `event_index:
e.event_id.map(|i| i.event_index)`, `operation_index: e.op_index…`, then
      sorts the consensus list: `contract.sort_by(|a, b| a.id.cmp(&b.id))`
      (every consensus event has an id; the diagnostic list keeps its
      container order). Fix the
      `stage` doc comment: pre-23 refunds carry `after_tx` (archive meta,
      README "Fee event identity").
- [x] **Step 4:** `cargo test -p api stellar_archive` → pass;
      `pnpm nx run @rumblefish/api-types:generate`.
- [ ] **Step 5: commit:** `feat(lore-0541): number transaction-page events by rpc id`

### Task 2.7: frontend

**Files:** `web/src/pages/transaction-detail/sections/EventsSection.tsx`,
`op-card/OperationCard.tsx`, `op-card/resources.ts`,
`web/src/pages/contracts/ContractEvents.tsx`; tests moved first:
`sections/EventsSection.test.tsx` → `sections/__tests__/EventsSection.test.tsx`,
`op-card/OperationCard.test.tsx` and `op-card/resources.test.ts` →
`op-card/__tests__/` (imports become `../EventsSection.js` etc.).

**Display — decided (karolkow, 2026-09-17, decision 229 A):** the `#` column
becomes `ID` and shows the full rpc id exactly as `getEvents` returns it
(monospace, selectable); diagnostic rows show `—`; rows arrive in execution
order (task 2.6). Rejected: the bare `event_index` as `#` (a ledger-wide counter
for fees, a position in the operation otherwise — `135, 0, 1, 12` reads as an
error) and a short `op 1 · 0` form with the id in a tooltip (our own notation,
the canonical id hidden).

- [x] **Step 0:** `git mv` the three test files, fix their imports,
      `pnpm nx test @rumblefish/soroban-block-explorer-web` → green.

- [x] **Step 1: failing tests** (`sections/__tests__/EventsSection.test.tsx`;
      the file's `event()` factory changes to take the new fields)

```tsx
function event(
  id: string | null,
  topic0: string | null,
  extra: Partial<XdrEventDto> = {}
): XdrEventDto {
  return {
    event_type: 'contract',
    contract_id: null,
    topics: topic0 == null ? [] : [{ type: 'sym', value: topic0 }],
    data: { type: 'void' },
    id,
    event_index: null,
    operation_index: null,
    stage: null,
    ...extra,
  } as unknown as XdrEventDto;
}

it('identifies each consensus event by its rpc id and places it by operation or stage', async () => {
  const user = userEvent.setup();
  renderSection(
    [
      event('0276810642227200000-0000000135', 'fee', {
        stage: 'before_all_txs',
      }),
      event('0276810642227757056-0000000000', 'transfer', {
        operation_index: 0,
        event_index: 0,
      }),
      event('0276810646522163200-0000000000', 'fee', {
        stage: 'after_all_txs',
      }),
    ],
    []
  );
  await user.click(screen.getByText(/Show 3 events/));
  const rows = rowsOf(screen.getByRole('table'));
  expect(rows.map((r) => r.ID)).toEqual([
    '0276810642227200000-0000000135',
    '0276810642227757056-0000000000',
    '0276810646522163200-0000000000',
  ]);
  expect(rows.map((r) => r.Where)).toEqual([
    'before all txs',
    'op 1',
    'after all txs',
  ]);
});

it('gives diagnostic entries no id', async () => {
  const user = userEvent.setup();
  renderSection(
    [
      event('0276810642227757056-0000000000', 'transfer', {
        operation_index: 0,
        event_index: 0,
      }),
    ],
    [event(null, 'fn_call', { event_type: 'diagnostic' })]
  );
  await user.click(screen.getByText(/Show 1 diagnostic entr/));
  const table = screen.getAllByRole('table').at(-1) as HTMLElement;
  expect(rowsOf(table).map((r) => r.ID)).toEqual(['—']);
});
```

(Ids above: ledger 64,450,000; transaction 136 = TOID + 136·4096; the refund's
transaction part is 1,048,575. The existing tests of this file switch from
`r['#']` to `r.ID` and from `op_index` to `operation_index`.)

- [x] **Step 2:** `pnpm nx test @rumblefish/soroban-block-explorer-web` → fails.
- [x] **Step 3: implementation**
  - header `#` → `ID` (width auto); cell: `<Box component="span"
sx={{ fontFamily: 'monospace', whiteSpace: 'nowrap' }}>{event.id ?? '—'}</Box>`
  - row key: `key={event.id ?? 'diag-' + index}` (map callback takes `index`)
  - `whereLabel`: `event.operation_index` instead of `event.op_index`
  - `OperationCard`: `key={event.id ?? index}`
  - `resources.ts`: fallback name `'(unnamed)'`
  - `ContractEvents`: `rowKey={(row) => row.id}`
- [x] **Step 4:** tests + `pnpm nx typecheck @rumblefish/soroban-block-explorer-web` → pass.
- [ ] **Step 5: commit:** `feat(lore-0541): show rpc event numbers on the transaction page`

### Task 2.8: schema, backfill tooling, network check, docs

**Files:**

- `crates/db-clickhouse/schema/init.sql` — `soroban_events` DDL = README
  "Design — Table" (under the name `soroban_events`), comment rewritten (the
  "OURS, NOT STELLAR'S" block replaced by the ADR 0059 summary); delete the
  `soroban_event_ops` DDL and comment; `asset_transfers` loses `event_index`
  and the "carried only to join `soroban_events`" sentence.
- `crates/backfill-runner/src/main.rs:98` — `--only` example without
  `soroban_event_ops`.
- `crates/backfill-runner/tests/redecode_diff.rs` — drop the
  `soroban_event_ops.tsv` export and the `event_index` column of
  `asset_transfers`.
- `crates/backfill-runner/tests/account_reconciliation.rs:310-330` — fee SQL:
  `LIMIT 1 BY ledger_sequence, transaction_index, operation_index, event_index`
  and `(ledger_sequence, application_order) IN (SELECT t.ledger_sequence,
t.application_order FROM transactions t WHERE t.id IN (SELECT transaction_id
FROM transaction_participants …))`.
- Create `crates/backfill-runner/tests/event_id_reconciliation.rs` — pattern of
  `pool_reserves_reconciliation.rs` (skips offline; cert from
  `infra-hetzner/ca/out/$USER` or env): for 5 ledgers inside the rpc window,
  (a) `getEvents` ids vs ids read from `soroban_events` (id string built in SQL
  as in README "Fee event identity"), (b) same ledgers parsed from the public
  archive (`indexer::handler::process::parse_ledger`) vs `getEvents`. Fails on
  any difference.
- Docs: `docs/architecture/database-schema/database-schema-overview.md`,
  `clickhouse-pilot.md`, `endpoint-queries-clickhouse/{14_get_contracts_events.sql,
03_get_transactions_by_hash.sql,README.md}`,
  `docs/architecture/xdr-parsing/xdr-parsing-overview.md`,
  `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`,
  `docs/architecture/frontend/frontend-overview.md`, `docs/backfills.md`
  (fill procedure from README; remove `soroban_event_ops` from `--only`
  guidance), `scripts/merge-*.sh` table lists (`soroban_events` shape note).
- ADR 0059 → `accepted`, delivery checklist ticked.

- [x] **Step 1:** grep gate — `rg -n "transaction_id" -g '*.rs' crates | rg
soroban_events` and `rg -n "soroban_event_ops|event_pos_in_op|op_index"
crates/api crates/db-clickhouse/src/persist/stage.rs` return only
      intended hits (asset_transfers keeps `op_index`/`event_pos_in_op` until
      its rebuild).
- [x] **Step 2:** `cargo test --workspace`; `pnpm nx run-many -t test typecheck lint`;
      `pnpm nx run @rumblefish/api-types:check-generated`.
- [ ] **Step 3: commit:** `docs(lore-0541): schema, tooling and docs for the rpc event id`
- [ ] **Step 4:** PR via `/pr` — title `feat(lore-0541): key soroban_events by
the stellar-rpc event id`, body lists phase 1 results and links this
      plan; **merge only after phase 1 "go"; deploy only in phase 4**.

---

## Phase 3 — fill partitions 100 through the head's partition

> Updated 2026-09-21 (review): the head crossed into partition 129 after this
> plan was written, and the phase now fills a second table,
> `contract_transactions`, slice by slice after the rekey (step 3b). See
> [S-review-and-contract-transactions](S-review-and-contract-transactions.md).

**When:** after the PR is ready to release, as close to the window as the fill
time allows — both copies of the table share the disk from the first
partition until phase 5. Free space 360.71 GiB on 2026-09-17 and 348.50 GiB on
2026-09-20 (~4 GiB/day, estimate from two readings); the fill adds ~170 GiB
(estimate) and the server logs are not trimmed (task 0563 deferred). At that
rate the finished fill sits ~2 weeks above the stop line, so the window must
follow the fill within that. **Stop** before any partition if free space is
under 120 GiB.

**Before the first partition:** the operator creates `contract_transactions`
verbatim from `crates/db-clickhouse/schema/init.sql`, and fills partition 127's
slices with step 3b as the index's trial; the agent then measures the
contract-filtered list on it against the plan's gate (< 1 s, < 1 GiB).

Order: 100 → the head's partition (129 on 2026-09-20). Partition 127 is done in
phase 1. The head's partition is filled up to `X` = the highest multiple of
5,000 at least 50,000 ledgers below the head when it is reached; the rest is
the window's tail.

Per partition `P`:

1. **Agent, read-only — pre-fill gate:** [`fill_gate.sql`](./fill_gate.sql)
   for every 5k slice of `P` (`sed` the `{A}`/`{B}` placeholders, pipe to
   `chq`; checked on 64,000,000–64,005,000 against the 2026-09-16 dry run). Pass: `new_keys = old_keys` in every slice; `transaction_index = 0`
   rows = `SELECT count() FROM transactions WHERE intDiv(ledger_sequence,

500000) = P`; `operation_index = 4095`rows only in slices < 58,762,518,`transaction_index = 1048575` only in slices ≥ it.

2. **Agent — disk:** `SELECT formatReadableSize(free_space) FROM system.disks
WHERE name = 'default'` ≥ 120 GiB; the day is not Sunday; no backup running
   (`SELECT count() FROM system.processes WHERE query ILIKE '%FREEZE%'` = 0).
3. **Operator — fill:**

```bash
F=lore/1-tasks/active/0541_FEATURE_canonical-event-location/notes/fill_insert.sql; P=101; for a in $(seq -f '%.0f' $((P*500000)) 5000 $((P*500000+495000))); do out=$(chw "$(sed -e "s/{A}/$a/g" -e "s/{B}/$((a+5000))/g" "$F")"); if printf '%s' "$out" | grep -q "DB::Exception"; then echo "FAILED at $a: $out"; break; fi; echo "ok $a"; done
```

3b. **Operator — `contract_transactions` for the same partition**, after step 3
    (it reads the rekeyed rows):

```bash
F=lore/1-tasks/active/0541_FEATURE_canonical-event-location/notes/fill_contract_transactions.sql; P=101; for a in $(seq -f '%.0f' $((P*500000)) 5000 $((P*500000+495000))); do out=$(chw "$(sed -e "s/{A}/$a/g" -e "s/{B}/$((a+5000))/g" "$F")"); if printf '%s' "$out" | grep -q "DB::Exception"; then echo "FAILED at $a: $out"; break; fi; echo "ok $a"; done
```

4. **Agent — post-fill:** staging row count for `P` = sum of the gate's `n`;
   if larger, operator runs `chw "OPTIMIZE TABLE soroban_events_staging_canonical PARTITION $P FINAL"`
   and the count repeats. For `contract_transactions`: `uniqExact(contract_id,
   ledger_sequence, application_order)` of `P` = the fill's `SELECT DISTINCT`
   run as a count over `P` (a short count is a partial insert). API p95
   latency during the fill is read from the CloudWatch dashboard (read-only);
   if it rose > 2× the previous hour, the next partition waits.

Estimate from phase 1: partition 127 (452 M rows, 4.3% of the table) took
583 s of query time over 100 slices — median 5.6 s, max 10.7 s, peak 3.68 GiB
per statement. Scaled by rows, all 29 partitions ≈ 3.8 h of query time
(estimate); in practice one or two partitions per sitting.

Loops use `seq -f '%.0f'`: the operator's `seq` printed `6.35e+07` in phase 1.
ClickHouse read those as exact floats (all 100 slices matched), but integer
literals remove the doubt.

---

## Phase 4 — the window

### 4.0 Preconditions (all true before starting)

- Phase 3 complete up to `X` for both tables; every gate passed.
- `contract_transactions` exists — the new indexer writes it, and a missing
  table fails every insert: `chq "EXISTS TABLE default.contract_transactions"`
  returns `1`.
- PR merged to `develop`; release PR `develop → master` merged (not tagged, not
  deployed). Everything else in that release is known and wanted.
- `asset_transfers.event_index` has its default — verified by a read, not
  recalled: `chq "SELECT default_kind FROM system.columns WHERE database =
  'default' AND table = 'asset_transfers' AND name = 'event_index'"` returns
  `DEFAULT`. If not, the operator runs
  `chw "ALTER TABLE asset_transfers MODIFY COLUMN event_index DEFAULT 0"` (the
  running indexer still writes the column; the new one will not). The swap does
  not cover this table.
- The two query shapes of the client outside this repository that read
  `soroban_events.transaction_id` / `event_index` are handed to its owner, or
  its outage over the window is accepted explicitly.
- Two local checkouts ready and built once (`make -C infra` build step):
  `prod` at the last `production-*` tag, `new` at `origin/master`.
- Not Sunday; free disk ≥ 120 GiB; SQS ingest queue and DLQ empty
  (`aws sqs get-queue-attributes --queue-url "$(aws sqs get-queue-url --queue-name production-ledger-ingest --query QueueUrl --output text)" --attribute-names ApproximateNumberOfMessages`
  and the same for `production-ledger-processor-dlq` — read-only, agent may
  run).

### 4.1 Pause (operator)

In `prod`: `infra/envs/production.json` → `"indexerLambdaConcurrency": 0`
(local edit, not committed), then

```bash
make -C infra deploy-production-compute
```

Agent verifies: `aws lambda list-event-source-mappings --function-name production-soroban-explorer-indexer`
returns none; `chq "SELECT max(sequence) FROM ledgers"` unchanged over 2 min.
Record the head `H`.

### 4.2 Tail (operator, then agent)

Operator: fill loop from `X` to `H + 1` (last slice may be shorter; use
`seq X 5000 H` and `B = min(a + 5000, H + 1)`), then the
`contract_transactions` loop over the same slices. Agent: pre/post gates on the
tail slices of both tables; `getEvents` comparison on 5 tail ledgers reading
the **staging** table (they are inside the rpc window); totals: staging
distinct rows per partition 100 through the head's partition = old table's —
the head's partition included, since it is the one a short tail loop or an
off-by-one on `H` would land in.

### 4.3 New code, still paused (operator)

In `new`: `"indexerLambdaConcurrency": 0` (local edit), then

```bash
make -C infra deploy-production-compute
```

From the end of this deploy until 4.4, the contract events tab, the
transactions-by-contract filter and the transaction page's event appearances
return errors.

### 4.4 Swap (operator, immediately)

```bash
chw "EXCHANGE TABLES soroban_events AND soroban_events_staging_canonical"
```

Agent: `chq "SHOW CREATE TABLE soroban_events FORMAT TSVRaw"` shows
`transaction_index`; the contract events endpoint for the native SAC returns
200 through the SPA.

### 4.5 Resume (operator)

In `new`: revert the local edit (`"indexerLambdaConcurrency": 1`), then

```bash
make -C infra deploy-production-compute
make -C infra deploy-production-web
```

(`deploy-production-web`: follow `docs/deployment.md` — Turnstile site key and
`--skip-nx-cache`, verify `/auth/session` = 200.)

Agent verifies: ESM present; `max(sequence)` advancing; DLQ empty after 15 min;
new rows since `H` exist with `transaction_index` set;
`cargo test -p backfill-runner --test event_id_reconciliation` passes; ledger
64,454,000 for `CAS3J7GY…` lists transactions in application order; one
transaction page shows `op N` / stage and rpc numbers; over the first ledgers
the new indexer wrote, `fill_contract_transactions.sql` (run as a `SELECT`)
returns exactly the writer's `contract_transactions` rows — `EXCEPT` both ways
empty — the one check that the SQL fill and the Rust writer agree.

### 4.6 Rollback (only if 4.5 fails and cannot be fixed forward)

1. Operator: `chw "EXCHANGE TABLES soroban_events AND soroban_events_staging_canonical"`
   (old table back under its name).
2. Operator: in `prod` with concurrency 1: `make -C infra deploy-production-compute`.
3. Ledgers `H+1 … now` are missing from the old `soroban_events` and
   `soroban_event_ops`: re-ingest them with `backfill-runner run` over that
   range from the `prod` checkout, then `repair-tier1` (`docs/backfills.md`).
4. README records what failed.

---

## Phase 5 — cleanup (before the next Sunday 03:30 UTC)

Operator, after the agent confirms 4.5:

```bash
chw "DROP TABLE soroban_events_staging_canonical"
chw "DROP TABLE soroban_event_ops"
chw "ALTER TABLE asset_transfers DROP COLUMN event_index"
```

Agent: disk freed (~240 GiB expected); task README closes the acceptance
criteria; `lore-framework-tasks` completion checklist.

---

## Coordination with task 0558

0558 (backlog, not yet on `develop`, other session) plans a rebuild-and-swap of
`asset_transfers` for `token_id`. Per ADR 0059 §4 that rebuild also renames
`op_index` → `operation_index` and `event_pos_in_op` → `event_index`, and must
not reintroduce the flat `event_index` this plan drops. Independent of 0541's
schedule; if 0558 runs first, task 2.3 edits the renamed struct instead.

## Self-review

- Spec coverage: README design (table, fill, gates, writer, readers, space),
  review items 1–7 (window 4.1–4.5, ADR, scope 2.4–2.8, backups 3/4.0/5, load
  3.4, rollback 4.6, writer proof 2.1–2.2/2.8), decisions 215–224 — each has a
  task or step.
- Open numbers, filled by phase 1: per-partition fill time, new partition
  size, benchmark medians.
