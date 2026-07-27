---
id: '0445'
title: 'FEATURE: transaction totals with success/failed split (24h window; all-time needs a rollup)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0171', '0420']
tags:
  [
    backend,
    api,
    frontend,
    clickhouse,
    transactions,
    priority-medium,
    effort-small,
  ]
links: []
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment: "if you have the
      total tx count you can probably show here #count (#success % | #failed
      %)". Not covered by 0171 — that task was about account/contract counts and
      was archived as superseded by 0420. Scoped here to a bounded window,
      because an all-time total is a different (and much larger) piece of work.
---

# FEATURE: transaction totals with success/failed split

## Summary

Show a transaction count alongside the success/failed breakdown, e.g.
`412,908 in the last 24h · 97.6% success / 2.4% failed`. Scoped to a bounded
recent window; an all-time total needs a rollup and is explicitly out of scope
until someone asks for it.

## Why the window matters

The transaction list uses keyset pagination precisely so no request ever has to
count the whole table. Reintroducing an unbounded `count()` over ~30M+ rows
would land straight in the read-row quota — the same trap the query comments
throughout `crates/api/src/transactions/queries.rs` warn about.

A bounded window avoids it entirely:

- `transactions` is `ORDER BY (ledger_sequence, application_order)`
  (`crates/db-clickhouse/schema/init.sql:559-575`), so a
  `WHERE ledger_sequence > head - N` predicate prunes on the sort key.
- 24h ≈ 17.3k ledgers at a 5s close time — a narrow scan of two columns.
- `successful Bool` is already on the row; no new column, no backfill.
- The existing TPS query already has this exact shape
  (`crates/api/src/network/queries.rs`, `WHERE sequence > {head} - 200`).

## Dedup is mandatory

`transactions` is a `ReplacingMergeTree` and prod tables carry unmerged
duplicate rows as a steady state (0420). A naive `countIf` **will over-report**.
Collapse first — `LIMIT 1 BY (ledger_sequence, application_order)` in a
subquery, mirroring the `LIMIT 1 BY sequence` the TPS query already uses — then
aggregate. Do not reach for `FINAL`; 0420 measured a 19x read amplification from
it on a comparable read.

## Scope

1. Extend `/network/stats` (or a sibling endpoint) with
   `tx_24h_total` / `tx_24h_successful` / `tx_24h_failed`.
2. Reuse the existing head probe; window length as a named constant, not
   scattered literals.
3. Frontend: render count plus both percentages on the transactions list header.
   Label the window explicitly — an unlabelled number reads as all-time and will
   be compared against explorers that do mean all-time.

## Out of scope (deliberately)

**All-time totals.** Two viable routes if it is ever wanted, both real work:
a counter table appended by the indexer (needs exactly-once semantics under
retry), or a per-ledger `successful` / `failed` count materialised at write time
plus a historical backfill. `ledgers` carries only `transaction_count` today
(`init.sql:99-104`) with no split, so neither is free. Spawn a separate task
rather than widening this one.

## Acceptance criteria

- [ ] Window-bounded totals + success/failed split exposed on the API
- [ ] Read dedups via `LIMIT 1 BY`, not `FINAL`; verified against a
      known-duplicated ledger range
- [ ] `read_rows` measured and recorded; bounded and stable as history grows
- [ ] Window is labelled in the UI, never presented as an all-time figure
- [ ] Percentages sum to 100 with a zero-transaction window handled (no
      divide-by-zero, no `NaN%`)
- [ ] **Docs updated** — `docs/architecture/database-schema/endpoint-queries-clickhouse/01_get_network_stats.sql`
      and the endpoint contract per ADR 0032
- [ ] **API types regenerated** — touches `crates/api/**`; run
      `npx nx run @rumblefish/api-types:generate`
