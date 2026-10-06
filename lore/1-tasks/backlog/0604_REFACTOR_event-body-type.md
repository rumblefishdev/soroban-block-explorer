---
id: '0604'
title: 'REFACTOR: one EventBody type for consensus and diagnostic events [structure only]'
type: REFACTOR
status: backlog
related_adr: ['0059']
related_tasks: ['0573']
tags: ['xdr-parsing', 'effort-small', 'priority-low']
links:
  - crates/xdr-parser/src/event.rs
  - crates/xdr-parser/src/types.rs
history:
  - date: 2026-10-01
    status: backlog
    who: claude
    note: >
      Spawned from 0573 future work: its step 2 asked for a shared body type,
      and the review of the logic PR found the four body fields copied in three
      places.
  - date: 2026-10-06
    status: backlog
    who: claude
    note: >
      Scope widened by the independent review of the 0573 logic PR: the
      invocation wildcard, and two proof gaps (duplicate ids). Not purely
      structural any more: the staging check is a guard, so the PR splits into
      a structure part and a small test-and-guard part.
---

# One EventBody type for consensus and diagnostic events

## Summary

Finish the model task 0573 described in its step 2:
`Event { id, origin, body: EventBody }` and `diagnostic: Vec<EventBody>`. It
is a rename across the event consumers with no behaviour change, so it ships as
one `[structure only]` PR after 0573's logic PR is merged.

## Context

Task 0573 rewrote event extraction but left the event flat. As a result:

- `decode` returns a four-element tuple (type, contract, topics, data);
- `DiagnosticEvent` repeats the same four fields as `ExtractedEvent`;
- the API's `event_dtos` maps those fields twice.

Two smaller leftovers sit next to it:

- NFT events keep `ledger_sequence` beside an `event_id` that already holds it;
- `LedgerEvents` stores its fee counters as an unnamed `(u32, u32)`.

## Implementation

- Add `EventBody { event_type, contract_id, topics, data }`. `decode` returns
  it, `ExtractedEvent` holds it as `body`, and `TxEvents.diagnostic` becomes a
  `Vec<EventBody>`.
- Change consumers from `ev.topics` to `ev.body.topics`, and so on (parser
  modules, staging, API extractors, tests).
- Remove `ledger_sequence` from `NftEvent` and `ExtractedNftEvent`, and read it
  from `event_id`.
- Name the `(before_all, after_all)` start pair as a two-field struct.
- `invocation.rs`'s `collect_diagnostic_events` ends in `_ => Vec::new()`,
  the silent wildcard `containers()` dropped: a future meta version would lose
  every invocation tree. Take the diagnostic container from `containers()`
  (`pub(crate)`), so one match over meta versions serves the whole parser.

Proof gaps from the 0573 review, for the same PR's tests:

- `event_id_reconciliation` compares `BTreeSet`s, so a duplicated id cannot
  fail it; compare the counts before the sets.
- Staging writes `soroban_events` keyed by the id; check the ids of one ledger
  are unique before the write, since a collision silently merges two rows.

## Acceptance Criteria

- [ ] `EventBody` is the only place the four body fields are declared.
- [ ] NFT events carry their ledger only inside `event_id`.
- [ ] One match over `TransactionMeta` versions in the parser, no wildcard.
- [ ] Reconciliation compares counts; staging refuses a duplicated id.
- [ ] Golden test `event_extraction_golden` passes with its expected files
      untouched; no assertion changes beyond the field paths.
- [ ] API types regenerate with no diff.
- [ ] PR title ends with `[structure only]`.
- [ ] Docs updated: `docs/architecture/xdr-parsing/xdr-parsing-overview.md`
      (event model), or N/A with reason.
