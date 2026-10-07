---
id: '0634'
title: 'A ledger is visible to the API only when it is whole — atomic ledger visibility during backfills below the tip'
type: FEATURE
status: backlog
related_adr: ['0044']
related_tasks: ['0584', '0322', '0293', '0553']
tags:
  [
    area-indexer,
    area-clickhouse,
    area-api,
    backfill-runner,
    priority-low,
    effort-large,
  ]
links: []
history:
  - date: '2026-10-07'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0584 (PR #639). In Stellar a ledger is one unit; in our
      store it becomes visible table by table during a backfill, so a reader
      can see a ledger's events before its transactions. #639 handles it on
      the contract events page only; this task is the fundamental fix.
---

# A ledger is visible to the API only when it is whole

## Summary

In the protocol a ledger closes as one unit: every event belongs to a
transaction of the same ledger. In our store a ledger appears table by table.
The backfill writer streams each table as its own insert and ClickHouse makes
each ~1M-row block visible as it lands (`crates/db-clickhouse/src/persist/writer.rs`,
"Memory budget", `min_insert_block_size_rows`); the `ledgers` row — the
commit marker — is written last (`writer.rs` "Commit-marker pattern"). While a
partition is being written for the first time, a reader can see a ledger's
`soroban_events` without its `transactions`, or any other cross-table half.

At the tip this never shows: the live indexer writes ledger by ledger and
readers that need it filter `ledger_sequence <= max(sequence) FROM ledgers`.
It shows only when a range **below the tip** is filled for the first time
while the tip is live: a gap after a restore (`docs/backups.md`), testnet's
from-genesis backfill with live running (0553). Re-ingesting a range that
already has rows is not affected — the old rows are there.

The data itself is right; it is only incomplete for a while. Nothing in the
protocol or in a finished ledger is wrong.

## What exists today

- #639 (task 0584): the contract events page checks a ledger's own `ledgers`
  row when an event's transaction is missing — landed means a broken row
  (500), not landed means the event is not shown yet. One page, on a miss.
- 0322 / 0293: the commit-marker + `ReplacingMergeTree` design is sound; true
  multi-table transactions are not available on ClickHouse MergeTree.

## Options to weigh

- **Read side, one rule:** every API read that combines tables keys on landed
  ledgers — a ledger exists for the API only once its `ledgers` row exists.
  Needs a list of the cross-table reads and a cheap way to apply the rule
  (the per-ledger `ledgers` seek is a primary-key read).
- **Write side, swap in whole:** a first-time fill writes into staging tables
  and moves each table's part in at commit, `transactions` before
  `soroban_events` (see the `EXCHANGE TABLES` / partition paths in
  `docs/backfills.md`). Per-table swaps are atomic; across tables an order
  replaces atomicity. Partition granularity (500k ledgers) vs a gap inside a
  live partition is the open problem.
- **Operational:** run first-time fills below the tip only with the API
  reading the gap's range as absent (e.g. a visible-range floor/ceiling), and
  record it in `docs/backfills.md`.

## Acceptance Criteria

- [ ] The cross-table API reads are listed, each marked safe or not during a
      first-time fill below the tip
- [ ] A first-time fill of a range below the tip shows no ledger half-written
      on any page (a test that writes a partition table by table)
- [ ] The #639 per-page check is removed or kept, with the reason recorded
- [ ] **Docs updated** — `docs/backfills.md` and the ingestion pipeline doc
