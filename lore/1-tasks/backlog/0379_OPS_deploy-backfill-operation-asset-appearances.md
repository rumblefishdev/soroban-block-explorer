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
  - date: 2026-07-16
    status: backlog
    who: stkrolikiewicz
    note: >
      Backfill EXECUTED 2026-07-13→16 on the prod box: re-index of the full range
      50,457,424–63,460,100 (~13M ledgers) via s5cmd pre-fetch + `run --reindex`,
      supervisor-governed. CREATE TABLE + re-index write COMPLETE; Phase-G validation
      (gap-scan + Horizon) and Phase-3 repair_tier1 PENDING — task NOT done. Runbook:
      docs/runbooks/backfill_derived_table_reparse_hetzner.md.
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

- [x] Manual `CREATE TABLE operation_asset_appearances` on prod (init.sql is
      fresh-only; `CREATE ... IF NOT EXISTS` will not re-run on the existing DB).
      Done 2026-07-13.
- [x] Backfill Soroban era from ledger **50,457,424** — required
      `backfill-runner run --reindex` (plain `Run` resume-skips already-ingested
      history → 0 rows). Re-indexed the full range **50,457,424–63,460,100**
      2026-07-16 (s5cmd pre-fetch + `--reindex`, supervisor-governed). Write
      complete; **coverage not yet gap-scanned**.
- [ ] Validate sample assets (incl. native + a type-3 token) vs Horizon /
      stellar.expert — list + all detail variants. **PENDING (Phase G).**
- [ ] **#8** read-in-order check — `EXPLAIN indexes=1` / `read_rows` on a hot
      asset (unblocked once the table has data). **PENDING.**

## Acceptance Criteria

- [x] table created on prod, backfill (re-index) write complete for the Soroban
      era — 2026-07-16 (coverage gap-scan is the next criterion)
- [ ] sample assets validated byte-identical vs prod-before / external sources
      — **PENDING (Phase-G gap-scan + Horizon / stellar.expert)**
- [ ] #8 read-in-order confirmed on real data — **PENDING**

## Execution (2026-07-13→16)

Executed on the prod box per the runbook
[`docs/runbooks/backfill_derived_table_reparse_hetzner.md`](../../../docs/runbooks/backfill_derived_table_reparse_hetzner.md)
(its worked-example appendix is the full run record).

- **CREATE TABLE + pre-backfill `BACKUP`** — 2026-07-13.
- **Re-index** — `s5cmd` pre-fetch of the public `aws-public-blockchain` ledger
  XDR + `backfill-runner run --reindex`, fan-out over **50,457,424–63,460,100**
  (~13M ledgers), governed overnight by a supervisor `tmux` pane (worker throttle
  16→6, disk-shed + OPTIMIZE). **Complete 2026-07-16**; throughput ~127–150k
  ledgers/hr (persist-bound; >6 workers no faster). Full `OPTIMIZE 100..126`
  auto-ran, `optimize.err = 0`.
- **`operation_pools` (0365)** landed the same day via `INSERT … SELECT` (no
  re-parse).

**Still open (task not done):**

- **Phase G** — gap-scan `operation_asset_appearances` vs `ledgers` / Horizon +
  sample-asset cross-check (`/compare-with-stellar-api`) + `EXPLAIN indexes=1`.
  Watermarks reached ≠ coverage proven; **no verified row count yet**.
- **Phase 3** — `repair_tier1` after PR #336 (indexer STOP → `repair-tier1
--dry-run` → `repair-tier1` → `nft-reclassify` → validate → START).
