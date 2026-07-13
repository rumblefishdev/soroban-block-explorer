---
id: '0379'
title: 'OPS: deploy + backfill operation_asset_appearances (0359 classic write-side)'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0359']
tags: [priority-high, effort-medium, ops, backfill]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 §13/§16. The deploy + backfill of the 0359 classic write-side.'
---

# OPS: deploy + backfill operation_asset_appearances

## Summary

Deploy and backfill the 0359 classic-op write-side (asset fan-out + account
participants). The code is complete and triple-verified (0359 §16); this is the
OPS execution. From-S3 re-parse (classic multi-leg data lives only in XDR).

## Context

Spawned from 0359. Write-side is backfill-ready: 3 adversarial agents clean,
decision 1c applied (issuer dropped), all baked-in decisions settled. The new
fan-out table is fresh-only in `init.sql` (prod is an existing DB), so the CREATE
is manual. Est. ~50-70 GiB, Soroban era ~5-6M ledgers.

## Implementation

- [ ] Manual `CREATE TABLE operation_asset_appearances` on prod (init.sql is
      fresh-only; `CREATE ... IF NOT EXISTS` will not re-run on the existing DB).
- [ ] Backfill `backfill-runner Run` Soroban era from ledger **50,457,424** (same
      rollout as the read swap, already merged).
- [ ] Validate sample assets (incl. native + a type-3 token) vs Horizon /
      stellar.expert — list + all detail variants.
- [ ] **#8** read-in-order check — `EXPLAIN indexes=1` / `read_rows` on a hot
      asset (unblocked once the table has data).

## Acceptance Criteria

- [ ] table created on prod, backfill complete for the Soroban era
- [ ] sample assets validated byte-identical vs prod-before / external sources
- [ ] #8 read-in-order confirmed on real data
