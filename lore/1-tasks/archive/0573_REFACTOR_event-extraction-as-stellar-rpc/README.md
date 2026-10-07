---
id: '0573'
title: 'REFACTOR: extract Soroban events the way stellar-rpc does — one pass, consensus and diagnostic apart'
type: REFACTOR
status: completed
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
  - date: 2026-10-06
    status: completed
    who: claude
    note: >
      Shipped in three PRs: #579 (executable_update move, structure only),
      #580 (golden differential test on four archive ledgers, low risk) and
      #475 (the extraction, merged 2026-10-06). Proof: golden output
      unchanged; production reconciliation 102 historical ledgers / 74,951
      ids parsed == stored and 5 tip ledgers / 5,387 ids equal to getEvents.
      Follow-ups 0604 (EventBody, one meta-version match, id checks) and 0626
      (diagnostic decode only for the API).
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
- [x] Production reconciliation across history — 102 ledgers from the
      protocol 20 activation to the tip, `AfterTx`/`AfterAllTxs` boundary
      included: 74,951 ids, parse equals table on every one; tip: 5 ledgers,
      5,387 ids equal on all three sides (2026-10-01)
- [ ] Spec's protocol 22 ledger and the `ledger_58816920` fixture in the
      golden set (not done: the four ledgers cover both refund stages and
      protocols 20 and 28)
- [ ] One shared `EventBody` type (deferred to 0604)

## Design Decisions

### From Plan

1. **Containers from stellar-go, ids from stellar-rpc** — one pass, the
   ledger-wide fee counters precomputed without decoding.
2. **The diagnostic channel is its own type** — the consumer filters go away
   because nothing can hand a diagnostic event to them.

### Emerged

3. **V3 metas read as operation 0** — the old code produced id-less events
   that staging refused; the new types cannot express that. No V3 meta exists
   in the archive: 393 ledgers, three from each of the 131 partitions between
   the protocol 20 activation and protocol 23, 117,579 transaction metas, all
   V4 (2026-10-06). No warning added.
4. **NFT event ids not optional** — task 0424 landed `Option<EventId>` and a
   staging error on `develop` meanwhile; both removed with the rest.
5. **`containers()` names every meta version** — a future one fails to
   compile instead of losing its events (review finding).
6. **PR split after the fact** — the move (#579) and the golden test (#580)
   left #475; the dead example and the history reconciliation test stayed in
   it by decision.

## Issues Encountered

- `develop` moved by hundreds of commits while the PR waited; two merges, the
  second resolved conflicts with #579/#580 in favour of the branch.
- Merging `develop` brought new consumers of the old API (pool tests, NFT
  ids from task 0424); moved onto the new types in the merge follow-up.

## Future Work

- 0604 — `EventBody`, one match over meta versions (the invocation tree still
  has `_ => Vec::new()`), duplicate-id checks in reconciliation and staging.
- 0626 — decode diagnostic events only where the API reads them.

## Notes

Per-event copies of transaction context (`transaction_hash`, `created_at`)
stay for now; moving them to the caller touches the NFT and pool extractors
and is a separate step.
