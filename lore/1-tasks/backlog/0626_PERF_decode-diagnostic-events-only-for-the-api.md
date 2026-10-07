---
id: '0626'
title: 'PERF: decode diagnostic events only where the API reads them'
type: PERF
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
  - date: 2026-10-07
    status: backlog
    who: claude
    note: >
      Measured: skipping the diagnostic decode saves ~8-12% of `parse_ledger`,
      which is ~6% of a backfill's time. Decision on whether to implement
      pending. Frontmatter type corrected to PERF.
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

## Measurement (2026-10-07)

Release build, the four golden ledgers, `parse_ledger` and `LedgerEvents::extract`
over every transaction, 100 rounds per run, 8 runs alternating A (as on
develop) and B (diagnostic list left empty), machine load average ~4.

|                     | consensus events | diagnostic events | `extract`, all txs    | `parse_ledger`          |
| ------------------- | ---------------- | ----------------- | --------------------- | ----------------------- |
| A — decode both     | 3,598            | 5,590             | 14.0 ms (median 14.3) | 108.7 ms (median 109.6) |
| B — skip diagnostic | 3,598            | 0                 | 7.5 ms (median 7.8)   | 95.5 ms (median 100.7)  |

Minimum over runs, total for the four ledgers. Per ledger: 50,500,000 carries
no diagnostic events (the archive's protocol-20 export); the other three carry
1,334–2,188, more than their consensus events.

- The diagnostic decode is about half of `extract`, and 8–12% of `parse_ledger`:
  ~2–3 ms of ~27 ms per ledger.
- A backfill spends ~6% of its time parsing (laptop1 run, range
  50,457,424–55,103,999: parse 6,735,881 ms of 130,696 s elapsed; persist
  100,653,662 ms), so the saving there is under 1%.
- Live ingest closes a ledger every ~5 s; 3 ms of it is not a constraint.

Method: a throwaway example calling `indexer::handler::process::parse_ledger`
on the fixtures, and an env switch around the `diagnostic` collect in
`extract`; neither committed.
