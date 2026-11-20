---
id: '0572'
title: 'RESEARCH: store event topics and data as binary XDR instead of JSON text'
type: RESEARCH
status: backlog
related_adr: ['0059']
related_tasks: ['0541', '0538']
tags:
  [
    'clickhouse',
    'storage',
    'soroban-events',
    'xdr-parsing',
    'effort-medium',
    'priority-medium',
  ]
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/xdr-parser/src/event.rs
history:
  - date: 2026-09-22
    status: backlog
    who: karolkow
    note: >
      Filed after task 0541 closed: the two payload columns are 88% of
      `soroban_events`, and the table is one fifth of the database.
  - date: '2026-11-18'
    status: backlog
    who: karolkow
    note: >
      Decided, no longer only a question: store events as raw XDR and serve
      the official SEP-51 JSON. Stage 1 of 0363 decision 13 moved here.
---

# Event topics and data as binary XDR instead of JSON text

## Summary

`soroban_events` is 195.9 GiB, 19% of the database (1,011 GiB, 2026-09-22).
`topics_xdr` alone is 155.89 GiB (15.67 B/row) and `data_xdr` 16.14 GiB: the
payload is 88% of the table. Despite the names, both hold JSON text — typed
values such as `{"type":"sym","value":"transfer"}` — written by the parser.
Measure whether the raw `ScVal` XDR is materially smaller once compressed,
before deciding anything.

## Why a question, not a plan

- **The table stays.** Task 0541 checked its readers on 2026-09-22: the
  contract events tab, the contract stats, the transaction page's event
  appearances, a price service outside this repository, and our own SQL tools.
  The derived tables do not depend on it; the parser builds them from XDR.
- **JSON is readable in SQL.** The price service's pool discovery and our
  analyses call `JSONExtractString(topics_xdr, …)`. Binary XDR moves decoding
  into the readers: the API has `xdr-parser`, SQL has nothing.
- **Any change is a rebuild** of the whole table plus a window, like 0541's.
  It should carry the other rebuild-only savings with it:
  [I-stage-enum-instead-of-transaction-index](../../archive/0541_FEATURE_canonical-event-location/notes/I-stage-enum-instead-of-transaction-index.md)
  (~7 GiB, _estimate_).

## Measure

1. Parse a sample of ledgers from one partition from the public archive, with
   the indexer's own code — enough for at least 10 M events.
2. Load the same rows into a local ClickHouse twice, same sort key: the payload
   as today's JSON, and as raw XDR bytes (`String CODEC(ZSTD(3))`).
3. Compare compressed bytes per row, column by column.
4. Time the API's decode of a 20-row events page from binary XDR.
5. List every SQL reader of the JSON shape — this repository's and the price
   service's known queries — with what each would need instead.

## Acceptance Criteria

- [ ] Bytes per row of topics and data, JSON against binary XDR, on the same
      sample in a local ClickHouse
- [ ] Projected saving on the whole table, labelled as an estimate from the
      sample's ratio
- [ ] Every reader of the JSON shape listed with its migration
- [ ] Decision recorded: a rebuild (folding in the 0541 idea) or not, and why

## Decided 2026-11-18 — build it (stage 1 of 0363 decision 13)

The owner decided (rows 22–24 of [`0363 S note`](../0363_FEATURE_advanced-scval-decoded-rendering/notes/S-events-table-value-rendering.md)):

- events are stored as raw XDR; the contract events tab cannot get the
  official JSON from our stored dialect;
- the API sends event topics and data only as the official stellar-xdr JSON
  (SEP-51, the library's serde; enable its `serde` feature), and every web
  view reads that one format;
- in stages: this task is events only; indexer readers of our JSON move in 0431.

Done when the transaction page and the contract events tab show SEP-51 JSON
(e.g. `{"u64":"17933488432273231284"}` for tx `cd965249…c591`), new events
store XDR, the backfill command is handed to the owner, tests cover each
value type against the library, and PR #742 (our dialect with integers as
text) is closed as superseded. Differences ours vs official: second pass of
[`R-audit-hand-rolled-decoding`](../0363_FEATURE_advanced-scval-decoded-rendering/notes/R-audit-hand-rolled-decoding-2026-11.md).
