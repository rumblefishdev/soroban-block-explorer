---
id: '0424'
title: 'BUG: NFT ownership order is ambiguous within a ledger — current owner can be nondeterministic'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0415']
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
