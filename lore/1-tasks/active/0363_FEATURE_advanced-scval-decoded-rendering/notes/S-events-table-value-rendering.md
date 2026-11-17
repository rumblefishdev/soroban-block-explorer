---
title: 'Events table: value rendering (first PR, Refs #735)'
type: synthesis
status: developing
spawns: []
tags: [frontend, transaction-detail, events, scval]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/735']
history:
  - date: '2026-11-17'
    status: developing
    who: karolkow
    note: 'Design round for issue #735; one question open (value encoding).'
---

# Events table: value rendering (first PR, Refs #735)

Issue #735: on a transaction page the Data column is squeezed. The measured
cause is a full address in Topics, rendered by `HighlightedJson`, breaking
character by character; the event ID is not the cause. A design round settled
the shape below for the Events table; the other renderers follow it, one PR
each.

## Decisions so far

| #   | Decision                                                                                                                                                                                                                                                                                                                                                                                                      | From                                   | vs this task           | Who does the same                                                         | Code | Test |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------- | ---------------------- | ------------------------------------------------------------------------- | ---- | ---- |
| 1   | Readers are developers and regular users: every value keeps its type, nothing is a JSON wall                                                                                                                                                                                                                                                                                                                  | design round                           | matches B              | stellar.expert                                                            | —    | —    |
| 2   | One table row per event, with columns; one view, no per-row expand, no two-line cards                                                                                                                                                                                                                                                                                                                         | design round                           | matches                | stellar.expert, Soroscan                                                  | —    | —    |
| 3   | The Type column stays with its coloured chip                                                                                                                                                                                                                                                                                                                                                                  | design round                           | matches F              | stellar.expert                                                            | —    | —    |
| 4   | Column order Type, Where, Contract, Topics, Data, ID; the ID in full, wrapping only after the dash                                                                                                                                                                                                                                                                                                            | design round                           | changes (ID was first) | Soroscan (ID last)                                                        | —    | —    |
| 5   | A scalar renders as its value plus a muted type tag (`81147 i128`); an unknown type renders as its raw JSON                                                                                                                                                                                                                                                                                                   | design round                           | matches B              | stellar.expert (`0i128`)                                                  | —    | —    |
| 6   | `vec` and `map`: one entry per line, indented                                                                                                                                                                                                                                                                                                                                                                 | design round                           | matches                | Stellar Lab                                                               | —    | —    |
| 7   | Lists and maps always in full (measured: map median 2 fields, max 15; vec median 3, max 13)                                                                                                                                                                                                                                                                                                                   | design round                           | beyond                 | stellar.expert                                                            | —    | —    |
| 8   | Bytes in full, wrapped, as lowercase hex — the form the contract page uses for the wasm hash                                                                                                                                                                                                                                                                                                                  | design round                           | beyond                 | Soroscan, Stellar Lab, `stellar xdr decode`                               | —    | —    |
| 9   | Addresses truncated 4…4, linked, with copy                                                                                                                                                                                                                                                                                                                                                                    | design round                           | matches                | stellar.expert, Soroscan                                                  | —    | —    |
| 10  | A number never breaks across lines                                                                                                                                                                                                                                                                                                                                                                            | design round                           | beyond                 | all compared explorers                                                    | —    | —    |
| 11  | Amounts stay in smallest units in this PR; token units come later in this task                                                                                                                                                                                                                                                                                                                                | design round                           | changes order          | stellar.expert, Soroscan, Stellar Lab (raw in events)                     | —    | —    |
| 12  | The Events table first; operation args, the execution trace and the contract Events tab follow, one PR each                                                                                                                                                                                                                                                                                                   | design round                           | matches                | —                                                                         | —    | —    |
| 13  | 64-bit integers (`u64`, `i64`, `timepoint`, `duration`, nonce; the account sequence number; 64-bit operation-detail fields) leave the API as decimal text. The parser and stored rows keep exact JSON numbers — digits are lost only in the browser's `JSON.parse`. Replaces the first draft (official format at the parser), which would have broken Rust readers, stored rows and the price service's query | design round, after the premise review | changes                | Horizon (`sequence` as text), official `stellar-xdr` JSON (`u64` as text) | —    | —    |
| 14  | The parser stores two values without loss: a string or symbol with invalid UTF-8 as escaped bytes (`ab\\xffcd`) instead of `<invalid-utf8>`, and an error with its type and code instead of the name only. New rows only (1 placeholder row in 450,861,344 measured; not rewritten). Shared with 0352                                                                                                         | design round                           | beyond                 | official `stellar-xdr` JSON                                               | —    | —    |

## Not yet decided

- **Storage form of event payloads** — decided 2026-11-17: stays JSON, in the
  decision-13 format. The alternatives (raw XDR for all payloads; standard
  events kept only in typed tables) moved to 0572 with their measurements, see
  [`0572 notes`](../../../backlog/0572_RESEARCH_event-payload-binary-xdr/notes/R-storage-measurements-2026-11.md).

Why decision 13 (measured 2026-11-17): the parser's own dialect differs from the
official JSON in five types, and a browser's `JSON.parse` rounds integers above
2^53 — e.g. the USDC `transfer` in transaction `e376edec…a19a` (ledger 64081288) carries `to_muxed_id` 6952519163801588163, read as
6952519163801588000; 6,378 / 6,871 / 6,966 events per 200,000-ledger slice
(from 52M, 58M, 64M) carry such a value. Audit of other hand-rolled decoding:
[`R-audit-hand-rolled-decoding-2026-11.md`](R-audit-hand-rolled-decoding-2026-11.md).

## Out of scope

- Interpreting contract-specific fixed-point values (e.g. a pool's square-root
  price in `pool_state`): no SEP or CAP defines them.
