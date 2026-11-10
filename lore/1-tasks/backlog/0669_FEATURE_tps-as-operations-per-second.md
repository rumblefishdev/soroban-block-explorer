---
id: '0669'
title: 'Report TPS as operations per second over the last hour'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0045']
tags: [priority-medium, effort-small, layer-api, layer-frontend-pages]
links: []
history:
  - date: '2026-11-10'
    status: backlog
    who: stkrolikiewicz
    note: >
      SDF asked why our TPS is about half of Chainspect's. Chainspect counts
      operations, not transactions; the team decided our TPS should too.
---

# Report TPS as operations per second over the last hour

## Summary

The home KPI and the top nav show "TPS" as transactions per second over the
last 60 s, about 60–67 on mainnet. Chainspect, the number people compare us
with, shows about 130 for Stellar because it counts operations. Our TPS will
mean the same: operations per second, including the operations of failed
transactions, averaged over the last 720 ledgers (~1 h).

## Stan teraz

- Done: root cause found and checked against Chainspect's own ledger range
  (see Context).
- Next: Step 1.
- In force: label stays "TPS"; value = operations/s incl. failed
  transactions; window = the 720 ledgers ending at head.

## Context

- `GET /v1/network/stats` → `tps_60s` is `sum(ledgers.transaction_count)`
  over the ledgers closed in the last 60 s, divided by max − min `closed_at`
  (`crates/api/src/network/queries.rs`). `transaction_count` is
  `tx_processing.len()`, so failed transactions are included.
- Chainspect puts the ledger range behind its Stellar figure in the page data
  (`tps.start` / `tps.finish`, `txns`). For `65384312..65385031` (720
  ledgers, 3,595 s) it reported 130.7805 "tx/s" and 470,156 "txns". Horizon
  for the same range: Σ `tx_set_operation_count` = 470,156 exactly (130.78/s);
  transactions incl. failed = 237,301 (66.0/s). Its "Max TPS (100 blocks)"
  of 218.088 also matches `tx_set_operation_count`, not transactions.
- Our ClickHouse agrees. Over the 720 ledgers before head on 2026-11-09:
  `count()` = 60.7 tx/s, `sum(operation_count)` = 132.85 op/s.
  `transactions.operation_count` is the envelope's operation count (inner tx
  for fee-bumps) and is set for failed transactions too, the same population
  as Horizon's `tx_set_operation_count`.
- The window length is not what makes the 2× gap: within one hour the 60 s
  values had a median of 66.6 tx/s against an hourly average of 61.7.
- The 60 s window over-counts by ~8%: it sums ~12 ledgers but divides by ~11
  inter-ledger intervals. Over 720 ledgers the same effect is 0.14%, and
  Chainspect divides the same way (470,156 / 3,595 s).

## Implementation Plan

### Step 1: Query

In `fetch_stats`, replace the `tps_60s` subquery:

```sql
toFloat64(ifNull(
    (SELECT sum(operation_count) FROM (
        SELECT ledger_sequence, application_order, operation_count
        FROM transactions
        WHERE ledger_sequence > {head} - 720 AND ledger_sequence <= {head}
        LIMIT 1 BY ledger_sequence, application_order))
    / nullIf((SELECT dateDiff('second', min(closed_at), max(closed_at))
              FROM ledgers
              WHERE sequence > {head} - 720 AND sequence <= {head}), 0),
    0
)) AS tps
```

- Both reads are primary-key ranges; no `closed_at` filter and no `now64()`.
- `LIMIT 1 BY` drops ReplacingMergeTree copies, the same rule as lore-0420.
  `min` / `max` of `closed_at` are not affected by copies.
- Rewrite the comment block above the query; it documents the 60 s window
  and the `closed_at` prune.
- Cost: measured 0.078 s and 225.5k rows on prod, once per new ledger (the
  cache is keyed on head). If it shows in ClickHouse load, store a
  ledger-level operation count in `ledgers` at ingestion instead (schema
  change).

### Step 2: DTO and API types

- Rename `NetworkStats.tps_60s` and `StatsRow.tps_60s` to `tps`. Doc
  comment: operations per second over the 720 ledgers ending at head,
  including the operations of failed transactions.
- `pnpm nx run @rumblefish/api-types:generate`.

### Step 3: UI

- Read the new field in `libs/ui/src/layout/TopNav.tsx` and
  `web/src/pages/home/ChainOverview.tsx`.
- Caption "Last 60s" → "Last hour" in `ChainOverview.tsx` and
  `HomeSkeleton.tsx`. Tooltip: operations per second, including failed
  transactions.

### Step 4: Verify on real data

- Run the Step 1 query with `head` = Chainspect's current `tps.finish`. The
  sum must equal its `txns`.
- `cargo run -p api --bin local` against prod ClickHouse; check the home KPI
  and the top nav.

## Acceptance Criteria

- [ ] For a range Chainspect publishes, the Step 1 query returns its `txns`
      exactly and its `tps.value` to two decimals.
- [ ] Home KPI and top nav show the new value and caption, checked on prod
      data through `cargo run -p api --bin local`.
- [ ] **Docs updated** — `GET /network/stats` in
      `docs/architecture/backend/backend-overview.md` and
      `docs/architecture/technical-design-general-overview.md`.
- [ ] **API types regenerated** — the `NetworkStats` field changes.

## Notes

- Other Stellar tools, checked 2026-11-09: StellarChain.io shows
  transactions per second (incl. failed, last 50 ledgers); StellarExpert
  shows no TPS, only per-ledger transactions and operations.
- If the hourly value feels too static, the same operation count over 60 s
  is a one-predicate change; it keeps the ~8% over-count unless the first
  ledger is dropped from the sum.
