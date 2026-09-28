---
id: '0424'
title: 'BUG: NFT ownership order is ambiguous within a ledger — current owner can be nondeterministic'
type: BUG
status: active
related_adr: ['0059']
related_tasks: ['0415', '0538', '0586']
tags:
  [
    'xdr-parser',
    'clickhouse',
    'indexer',
    'nft',
    'phase-future',
    'effort-medium',
    'priority-medium',
  ]
links:
  - crates/xdr-parser/src/state.rs
  - crates/api/src/nfts/queries.rs
history:
  - date: '2026-07-21'
    status: backlog
    who: karolkow
    note: >
      Found while auditing log-sourced facts (0415). Independent of the
      events-vs-ledger debate — this is our own ordering bug and must be fixed
      whichever source NFT ownership ends up reading.
  - date: '2026-09-28'
    status: active
    who: karolkow
    note: >
      Activated as epic 0538 step 4 (thread 292 A): the ownership rows move to
      the canonical event location, which removes the last two tables carrying
      `transaction_id`. Decided: history filled from `soroban_events` through
      the same Rust extraction (293 B2); `event_order` leaves the API for the
      position (294 A).
---

# BUG: NFT ownership order is ambiguous within a ledger

## Summary

`nft_ownership.event_order` is **our own counter**, not a chain ordering. It is
keyed `(contract_id, token_id, ledger_sequence)` and restarts at `0` for every
token in every ledger, so it does **not** encode which transaction within the
ledger came first. `nfts.current_owner_ledger` — the RMT version that decides the
current owner — is **ledger-only**. When one token has two or more ownership
events in the same ledger, the winner is decided by the merge, i.e.
**nondeterministically**, and the displayed current owner can be wrong.

## Context

`crates/xdr-parser/src/state.rs` (`extract_nft_ownership_events`):

```rust
let mut order_counter: HashMap<(String, String, u32), u16> = HashMap::new();
…
let key = (event.contract_id.clone(), token_id.clone(), event.ledger_sequence);
let counter = order_counter.entry(key).or_insert(0);
```

Consequences:

- Ordering by `(ledger_sequence, event_order)` is only meaningful **across**
  ledgers. Within a ledger it compares two counters that both start at `0`.
- `mint` + `transfer` in a single transaction (mint to treasury, then transfer to
  buyer) is a **mass pattern**, not an edge case — so the ambiguous case is common.
- Any analysis that folds this stream to a "current owner" inherits the ambiguity.
  This already produced a false positive during the 0415 audit: a consistency
  check read the ordering as authoritative and flagged 88 tokens as having a
  "transfer before mint", which the ordering data cannot actually establish.

**Measured on prod (2026-07-21):** **88 tokens** have more than one ownership
event inside a single ledger — that is the population at risk. This is NOT a
claim that all 88 currently display the wrong owner; it is the set where the
answer is not determined by the data.

```sql
SELECT count() FROM (
  SELECT contract_id, token_id, ledger_sequence, count() AS ev
  FROM nft_ownership GROUP BY contract_id, token_id, ledger_sequence
  HAVING ev > 1);
```

## Epic 0538 step 4 — the canonical location (2026-09-28)

**Measured (read-only):** `nft_ownership` 23,504 rows (398 KiB),
`nft_ownership_pending` 521; 24,025 rows over 11,454 ledgers and 139
contracts, from ledger 51,827,994. No storage gain — the step is what lets
`transactions.id` (31.6 GiB) go, and it fixes this task's order.

**Found in the code** (read-only map, 2026-09-28):

- The position exists upstream and is dropped: `ExtractedEvent.event_id`
  (`EventId {ledger, transaction_index, operation_index, event_index}`) is
  not copied by the `NftEvent` constructors (`xdr-parser/src/nft.rs`).
- NFT events are per-operation contract events, so every one has an rpc id
  in every protocol (only transaction-level events without a stage lack one).
- `consecutive_mint` expands one event into many rows with one id — the key
  keeps `token_id`.
- Readers: the transfers tab keys `(ledger_sequence, event_order)` and its
  `LIMIT 1 BY` collapses distinct same-ledger rows (every token counts from
  0); `accounts/balance_changes.rs` joins on `transaction_id` and infers the
  moved pieces from the owner timeline; `event_order` is on the wire
  (`NftTransferItem`) and in the frontend row key.
- Promotion `_pending` → live is `INSERT … SELECT *`
  (`backfill-runner/src/nft_reclassify.rs`): both tables change together.

**Decided (karolkow):**

- Work tracked here, not in a new task (292 A).
- History filled from `soroban_events`: the rows' contract events are read
  back (topics / data are the parser's own JSON, stored verbatim) and run
  through the same `detect_nft_events` → `extract_nft_ownership_events` as
  the indexer — no S3 (293 B2). Gate: whole rows against the old tables,
  position aside.
- `event_order` leaves the API; a transfer carries `application_order`,
  `operation_index`, `event_index` like the other lists (294 A).

**Plan — parallel change, as tasks 0372 / 0586:** new tables beside the old
ones (key `(contract_id, token_id, ledger_sequence, application_order,
operation_index, event_index)`, no `transaction_id`, no `event_order`), dual
write, fill, readers, stop the old writes, drop.

- **Names** (295 A): `nft_ownership_changes`, `nft_ownership_changes_pending`
  — a row is any change of owner (mint, transfer, burn).
- **`nfts` current owner** (296 A): its same-ledger tie is fixed in its own
  PR after the readers — another table, another step.
- **PRs** (297 A):

| PR  | What                                                                                         | Deploy                 | Operator           |
| --- | -------------------------------------------------------------------------------------------- | ---------------------- | ------------------ |
| 0   | move: `nft.rs` tests → `nft/tests.rs`, NFT state → `state/nfts.rs`                           | no                     | —                  |
| 1   | parser carries the event position; new tables beside the old; promotion moves both           | yes                    | `CREATE` ×2 before |
| 2   | fill tool from `soroban_events` + gate against the old tables                                | no (run from a laptop) | —                  |
| 3   | readers: transfers tab by position, new wire fields + frontend; `balance_changes` exact join | yes                    | —                  |
| 4   | old tables no longer written; allowlist empty                                                | yes                    | `DROP` ×2 after    |

- **PR 0 opened** (2026-09-28):
  [#521](https://github.com/rumblefishdev/soroban-block-explorer/pull/521),
  `refactor/0424-move-nft-parsing` — `nft.rs` 1,203 → 425, `state.rs`
  1,344 → 1,191; 926 lines moved, glue only; `xdr-parser` 501 tests pass.
- **PR 1 (write both)** — branch `feat/0424-nft-ownership-changes-dual-write`,
  local, stacked on #521: `50ea9de1` — `NftEvent` / `ExtractedNftEvent` keep
  the source `event_id`; `nft_ownership_changes{,_pending}` (DDL, one row
  struct for both, staging beside the old pair, writer); staging refuses a
  change without an event id; `nft-reclassify` moves both pairs; merge
  scripts list the new tables; `stage.rs` 2,949 → 2,948. `eca010a1` — schema
  overview §4.13.2, pipeline, deployment step 1. Checks: workspace clippy
  clean; parser test that a `consecutive_mint`'s tokens share one id and a
  transfer keeps its own; staging tests (hot / pending / dropped routing of
  the new rows, refusal without an id); `db-clickhouse` all tests pass on a
  local ClickHouse 26.3 (the G9 e2e writes and reads the location);
  `xdr-parser`, `backfill-runner`, `indexer`, `api` 1,294 tests pass.

## Implementation

- Thread the transaction's **application order** (and the event's index within the
  transaction) into `NftEvent` / `ExtractedNftEvent`, so a total order exists:
  `(ledger_sequence, application_order, event_index)`.
- Use that tuple for `event_order` (or add columns) and for the `nfts` RMT
  **version**, so the same-ledger tie is resolved by data, not by the merge.
- Backfill/re-ingest the affected range; verify the 88 at-risk tokens resolve
  deterministically afterwards.
- Add a regression test: two ownership events for one token in one ledger, in both
  emission orders, must yield the later one as current owner.

## Subtask: event display order ignores the CAP-67 stage

Same class (ordering thrown away at ingest), different table — found in the
2026-07-21 audit.

`TransactionEvent.stage` is decoded but never persisted or used
(`crates/xdr-parser/src/event.rs`), and tx-level events are numbered `0..k`
**before** the per-operation events. Both read paths then sort by `event_index`
ascending (`crates/api/src/contracts/queries.rs`, and the tx-detail split).

CAP-0067 places the initial fee charge at `BEFORE_ALL_TXS` and the **fee refund at
`AFTER_ALL_TXS`** — i.e. after every transaction in the ledger. Numbering the
refund into the low indices renders it **before** the contract events that caused
it, which is simply the wrong story on the transaction page.

Reported measurement (from the audit agent, **not independently re-verified** —
confirm before acting): in ledgers 63,578,000–63,578,074, `event_index = 0` is a
`fee` event in 36,635 transactions; `event_index = 1` is `fee` in 12,188 and
`transfer` in 2,780.

Fix: carry `stage` through to storage and order by `(stage, application_order,
index-within-stage)` rather than by a flat ingest counter. Verify the CAP-67 stage
semantics against the spec first — the protocol, not our current numbering, is the
authority on what order these belong in.

## Subtask: is the same tie possible in the OTHER state tables?

This is a **class** of bug, not one table. Any `ReplacingMergeTree` whose version
column is **ledger-only** cannot break a tie between two writes for the same key
inside one ledger — the merge picks arbitrarily. `init.sql` has ~9 such tables:

| Version column            | init.sql                                                      |
| ------------------------- | ------------------------------------------------------------- |
| `last_seen_ledger`        | 157                                                           |
| `wasm_uploaded_at_ledger` | 210                                                           |
| `last_updated_ledger`     | 388, 412, 506, 516                                            |
| `current_owner_ledger`    | 442, 468                                                      |
| `version`                 | 242, 341, 489 — check whether this one is composite/monotonic |

**A working mitigation already exists in the codebase** — copy it rather than
inventing one. `persist::stage::build_balance_rows` dedups by key keeping the LAST
occurrence _before_ insert, precisely because "two txs in one ledger can touch the
same holder+asset, producing rows that share the RMT version … a tie the merge
would resolve nondeterministically". So `balances` is mitigated **in Rust**, not by
the version column.

Audit each table above and record one of:

- **mitigated in-process** (like `build_balance_rows`) — note where, and check the
  guarantee still holds if the same key is written by a _different_ path (live
  ingest vs S3 re-ingest overlap, or two writers in one run), which in-batch dedup
  does not cover;
- **not mitigated** (like `nfts`) — same defect as this task, fix the same way;
- **not applicable** — same-ledger double-write for one key is structurally
  impossible; state why.

Note the `version`-based tables (242/341/489) may already be the correct pattern —
if so, promote it as the convention instead of spreading in-process dedup.

## Acceptance Criteria

- [ ] Ownership events carry a total, chain-derived order within a ledger
- [ ] `nfts` RMT version breaks same-ledger ties deterministically
- [ ] Re-ingested range: the 88 at-risk tokens resolve to a stable current owner
      across repeated merges
- [ ] Regression test covers both emission orders in a single ledger
- [ ] 0415's consistency checks re-run against the corrected ordering (the earlier
      "transfer before mint" signal must be re-evaluated, not carried over)
- [ ] Every ledger-only-versioned RMT table audited and classified
      (mitigated in-process / not mitigated / not applicable), with the in-batch
      dedup's cross-path limitation assessed
- [ ] A single convention chosen and written down (composite version column vs
      in-process last-wins), so new state tables do not reintroduce the tie
