---
id: '0598'
title: 'Classic pools join pool_movements; pool_operation_amounts retires'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0374', '0372']
tags: ['effort-large', 'priority-medium']
links: []
history:
  - date: 2026-09-29
    status: backlog
    who: karolkow
    note: 'Spawned from 0374 (decision 147 A′): one movements table for both pool kinds.'
---

# Classic pools join `pool_movements`

## Summary

`pool_movements` (0374, W1) holds soroban pool swaps, deposits and
withdrawals at event grain, `Int128`, kind stored — the shape both pool kinds
would share if built from scratch (decision 147 A′). Classic amounts still
live in `pool_operation_amounts` (`Int64`, per operation), so every reader of
activity, volume and fees branches by pool kind. Move classic pools into the
one table and retire the old one.

## Implementation

- Classic writer also writes `pool_movements` (`event_index = 0`, kind from
  the operation type), dual-write first.
- Before sourcing classic history from `asset_transfers` (`L` transfers +
  operation type) instead of `pool_operation_amounts`, measure it the way
  0374 measured soroban pools (`pool_movements_vs_asset_transfers`): that
  table cannot see tokens with non-standard transfer events.
- History: `INSERT … SELECT` from `pool_operation_amounts` joined to the
  operation type — ClickHouse-side, no archive re-parse (991M rows; Karol runs
  it). Verify row counts and `sum(amount)` per pool against the old table.
- Readers (activity feed, volume, fees, chart) read `pool_movements` for both
  kinds; the per-kind branches go.
- Drop `pool_operation_amounts` after a deploy with no reader (replacing a
  table in `init.sql`: new CREATE under the old, then the drop).

## Measured 2026-10-01 (read-only, production)

- `pool_operation_amounts`: 994M rows, 52,903 pools, 6.07 GiB on disk;
  `pool_movements` (soroban only): 12.3M unique rows, 708 pools, 123 MiB.
- Classic volume and fees do not read `pool_operation_amounts`: they come from
  `liquidity_pool_snapshots` (`gross_volume_*`). Only the activity feed reads
  it. Moving classic pools here therefore changes the source of classic volume
  too — the before/after check must cover volume, not only activity.
- Reads on `pool_movements`, busiest soroban pool (4.1M rows): an activity
  page of 20 events 70 ms / 4.1M rows read; 24 h volume 12 ms / 26k rows; a
  full-history daily volume series 2.4 s / 20M rows / 1.9 GiB, almost all of
  it the `ledgers` join — the chart's windowed read stays well under.

## Acceptance Criteria

- [ ] Classic activity, volume and fees unchanged on production for sampled
      pools (API before/after equal).
- [ ] No reader branches by pool kind for movements.
- [ ] `pool_operation_amounts` gone from `init.sql` and production.
- [ ] **Docs updated** — `database-schema-overview.md`, `backend-overview.md`,
      `docs/backfills.md`.
