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

| #   | Decision                                                                                                    | From         | vs this task           | Who does the same                                     | Code | Test |
| --- | ----------------------------------------------------------------------------------------------------------- | ------------ | ---------------------- | ----------------------------------------------------- | ---- | ---- |
| 1   | Readers are developers and regular users: every value keeps its type, nothing is a JSON wall                | design round | matches B              | stellar.expert                                        | —    | —    |
| 2   | One table row per event, with columns; one view, no per-row expand, no two-line cards                       | design round | matches                | stellar.expert, Soroscan                              | —    | —    |
| 3   | The Type column stays with its coloured chip                                                                | design round | matches F              | stellar.expert                                        | —    | —    |
| 4   | Column order Type, Where, Contract, Topics, Data, ID; the ID in full, wrapping only after the dash          | design round | changes (ID was first) | Soroscan (ID last)                                    | —    | —    |
| 5   | A scalar renders as its value plus a muted type tag (`81147 i128`); an unknown type renders as its raw JSON | design round | matches B              | stellar.expert (`0i128`)                              | —    | —    |
| 6   | `vec` and `map`: one entry per line, indented                                                               | design round | matches                | Stellar Lab                                           | —    | —    |
| 7   | Lists and maps always in full (measured: map median 2 fields, max 15; vec median 3, max 13)                 | design round | beyond                 | stellar.expert                                        | —    | —    |
| 8   | Bytes in full, wrapped, as lowercase hex — the form the contract page uses for the wasm hash                | design round | beyond                 | Soroscan, Stellar Lab, `stellar xdr decode`           | —    | —    |
| 9   | Addresses truncated 4…4, linked, with copy                                                                  | design round | matches                | stellar.expert, Soroscan                              | —    | —    |
| 10  | A number never breaks across lines                                                                          | design round | beyond                 | all compared explorers                                | —    | —    |
| 11  | Amounts stay in smallest units in this PR; token units come later in this task                              | design round | changes order          | stellar.expert, Soroscan, Stellar Lab (raw in events) | —    | —    |
| 12  | The Events table first; operation args, the execution trace and the contract Events tab follow, one PR each | design round | matches                | —                                                     | —    | —    |

## Not yet decided

- **How values are encoded between parser and page.** `crates/xdr-parser/src/scval.rs`
  writes its own JSON dialect, which the official `stellar-xdr` JSON
  (`stellar xdr decode --output json`) differs from in five value types:

  | Type                                  | Ours                         | Official                   |
  | ------------------------------------- | ---------------------------- | -------------------------- |
  | `u64`, `i64`, `timepoint`, `duration` | JSON number                  | decimal string             |
  | `u256`, `i256`                        | 64 hex digits                | decimal string             |
  | `bytes`                               | base64                       | hex                        |
  | `string`, `sym` with invalid UTF-8    | `<invalid-utf8>` placeholder | escaped bytes (`ab\xffcd`) |
  | `error`                               | name only                    | type and code (0352)       |

  The first row loses data: a browser's `JSON.parse` rounds integers above
  2^53. Example: the USDC `transfer` in transaction `e376edec…a19a` (ledger 64081288) carries `to_muxed_id` 6952519163801588163; the page reads 6952519163801588000. Measured on three 200,000-ledger slices (from 52M,
  58M, 64M): 6,378 / 6,871 / 6,966 events carry such a value. The same
  dialect is stored in `soroban_events.topics_xdr` / `data_xdr`, so a change
  meets stored rows. Related: 0431 (adopt the `stellar-xdr` API), 0456
  (`u256` decode), 0352 (error code).

## Out of scope

- Interpreting contract-specific fixed-point values (e.g. a pool's square-root
  price in `pool_state`): no SEP or CAP defines them.
