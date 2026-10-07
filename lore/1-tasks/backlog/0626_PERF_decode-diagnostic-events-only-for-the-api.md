---
id: '0626'
title: 'PERF: decode diagnostic events only where the API reads them'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0573', '0604']
tags: ['xdr-parsing', 'indexer', 'performance', 'effort-small', 'priority-low']
links:
  - crates/xdr-parser/src/event.rs
  - crates/indexer/src/handler/process.rs
history:
  - date: 2026-10-06
    status: backlog
    who: claude
    note: >
      Spawned from the independent review of the 0573 logic PR (finding C3).
---

# Decode diagnostic events only where the API reads them

## Summary

`LedgerEvents::extract` decodes a transaction's diagnostic events (ScVal to
JSON) together with its consensus events. The indexer and the backfill keep
only `.events` and drop the rest. Only the API's transaction page reads
`.diagnostic`. With diagnostic mode on, that channel holds a copy of every
consensus event plus the host trace, so ingest does about twice the decode work
it needs (estimate, not measured).

This is not a regression: the extraction before 0573 decoded them too.

## Implementation

- Measure first: time `parse_ledger` on the four golden ledgers with and
  without the diagnostic decode.
- If the gain is worth it, split `extract` so the diagnostic list is decoded
  only on request (for example a separate `diagnostic(tx_index)` the API
  calls), and keep `extract` for consensus events.

## Acceptance Criteria

- [ ] The measurement is recorded here, with the command.
- [ ] If implemented: the indexer path decodes no diagnostic event; the
      transaction page still shows them; golden test unchanged.
