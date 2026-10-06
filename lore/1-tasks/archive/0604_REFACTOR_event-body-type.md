---
id: '0604'
title: 'REFACTOR: one EventBody type for consensus and diagnostic events'
type: REFACTOR
status: completed
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
  - date: 2026-10-06
    status: active
    who: claude
    note: >
      Started on request. Two PRs: `[refactor]` (EventBody, one match over
      meta versions, NFT ledger only in the id), then a small behaviour PR
      (duplicate-id refusal in staging, count check in the reconciliation).
  - date: 2026-10-07
    status: completed
    who: claude
    note: >
      Shipped in four PRs: #613 soroban_events staging move (structure only),
      #614 duplicate-id refusal (behaviour), #615 reconciliation count (low
      risk), #612 EventBody (refactor). Golden output unchanged; four golden
      ledgers carry 3,598 ids, none repeated.
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
- ~~Name the `(before_all, after_all)` start pair as a two-field struct~~ —
  dropped: a private pair destructured into named locals in one file; a type
  would add a definition to open, not clarity.
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

- [x] `EventBody` is the only place the four body fields are declared.
- [x] NFT events carry their ledger only inside `event_id`.
- [x] No wildcard over `TransactionMeta` versions in the event model and the
      invocation tree (`operation.rs` and `ledger_entry_changes.rs` keep theirs:
      outside the event model).
- [x] Reconciliation compares counts; staging refuses a duplicated id.
- [x] Golden test `event_extraction_golden` passes with its expected files
      untouched; no assertion changes beyond the field paths.
- [x] API types regenerate with no diff.
- [x] PRs by basket: #612 `[refactor]`; #613 `[structure only]` (the
      `soroban_events` staging move `stage.rs` needed first); #614 the
      duplicate-id guard (behaviour); a `[low risk]` reconciliation count.
- [x] Docs updated: `docs/architecture/xdr-parsing/xdr-parsing-overview.md`
      (event model), or N/A with reason.

## Outcome

- **Emerged:** #614 needed `stage.rs` split first (#613, `moved.sh`: 96 lines
  out, 96 in), because the file is past the size limit.
- **Emerged:** the duplicate-id guard found five staging fixtures with several
  events under one id; `numbered()` gives them distinct ids, assertions
  unchanged.
- **Issue:** the compiler-guided rename collapsed three NFT state-test events
  to ledger 100 (were 200, 300 and a parameter) while tests still passed;
  caught in review, restored with `event_at(ledger)`.
- **Issue:** the disk filled during the work; merged worktrees were cleaned.
- **Left:** `operation.rs` and `ledger_entry_changes.rs` keep a wildcard over
  meta versions (outside the event model, no task yet).
