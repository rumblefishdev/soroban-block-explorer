---
id: '0579'
title: 'OPS: storage savings that need no deploy window — drop the unused hash bloom, codecs on integer columns'
type: OPS
status: active
related_adr: ['0059']
related_tasks: ['0538', '0575']
tags: ['clickhouse', 'storage', 'effort-small', 'priority-medium']
links:
  - crates/db-clickhouse/schema/init.sql
history:
  - date: 2026-09-23
    status: active
    who: karolkow
    note: >
      Batch 1 of the 0538 database-size list: the changes that neither the
      writer nor the reader sees, so no indexer pause. Started with the
      transactions hash bloom, dropped on production the same day.
---

# Storage savings that need no deploy window

## Summary

Batch 1 of the 0538 database-size list
([survey](../0538_EPIC_canonical-transaction-and-event-location/notes/R-whole-database-survey-2026-09-23.md)):
changes invisible to the clickhouse-rs `DESCRIBE` check, so they ship
without pausing the indexer. The unused `transactions.idx_tx_hash_bloom`
(4.93 GiB) and codecs on the integer columns that have none (~15–25 GiB,
_estimate_).

## Context

Both reads of `transactions` by hash pin `ledger_sequence` first
(`search/queries.rs`, `transactions/queries.rs` detail: `hash OR
inner_tx_hash`), and one ledger fits one granule. `EXPLAIN indexes = 1` on
ledger 64,454,000: the primary key keeps 1 of 1,278 granules, the bloom 1 of

1. `system.query_log`, 30 days: 163 API and 34 operator reads of
   `transactions` by hash, every one with the ledger pinned.

## Implementation Plan

### Step 1: drop the hash bloom — done 2026-09-23

Production: `ALTER TABLE transactions DROP INDEX idx_tx_hash_bloom` (operator).
Repository: the index out of `init.sql`; the docs that named it as a serving
index corrected.

### Step 2: codecs on integer columns

Candidates with no codec today: `soroban_events` (`ledger_sequence`,
`transaction_index`, `application_order`, `operation_index`, `event_index`),
`contract_transactions` (`ledger_sequence`, `application_order`),
`transaction_hash_index.ledger_sequence`. Measure one partition locally with
codec variants side by side (task 0575 method, disk budget first), then
`ALTER TABLE … MODIFY COLUMN … CODEC(…)` and a per-partition rewrite by the
operator.

## Acceptance Criteria

- [x] `idx_tx_hash_bloom` gone from production and from `init.sql`; no read
      of `transactions` by hash without a pinned ledger (query log, 30 days)
- [ ] Codec trial recorded per column with numbers
- [ ] Codecs applied on production; saving re-measured per table
- [x] **Docs updated** — `database-schema-overview.md` (skip-index inventory),
      `endpoint-queries-clickhouse/22_get_search.sql` and `README.md`,
      `docs/scf/ch-demo-queries.sql` comment
