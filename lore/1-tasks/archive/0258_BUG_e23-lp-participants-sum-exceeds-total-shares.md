---
id: '0258'
title: 'BUG: E23 LP participants page sum > liquidity_pool_snapshots.total_shares'
type: BUG
status: active
related_adr: []
related_tasks: ['0252']
tags: [priority-medium, effort-small, layer-clickhouse, data-correctness]
milestone: 1
links:
  - scripts/0252/phase_d_e23.py
  - docs/architecture/database-schema/endpoint-queries-clickhouse/23_get_liquidity_pools_participants.sql
history:
  - date: '2026-05-25'
    status: backlog
    who: stkrolikiewicz
    note: >
      Surfaced by task 0252 Phase D E23 internal-consistency check.
      Pool `510faa345abc...` (full hex in TSV) reported
      `page_sum = 1,556,294,929,934.0696` (sum of first 150
      participants by shares DESC, three pages × 50) against
      `total_shares = 518,764,976,644.6898` from the latest
      `liquidity_pool_snapshots` row — page_sum ≈ 3 × total. The
      invariant `Σ lp_positions.shares == liquidity_pool_snapshots.total_shares`
      should hold point-in-time for any pool; a 3× excess means
      either:
        a. `lp_positions FINAL` is not deduping historical positions
           (Replacing engine — needs an `OPTIMIZE FINAL` pass or the
           parser is over-emitting on multi-snapshot ledgers); or
        b. `liquidity_pool_snapshots` `argMax(total_shares,
           ledger_sequence)` lags the actual reality on this pool;
           or
        c. `lp_positions` carries un-zeroed rows for accounts that
           have since withdrawn (parser missing the
           remove/zero-out event).

      Spawned as backlog so Phase D can close 9/9 with this single
      anomaly tracked separately. 214/215 valid pools in the sample
      pass cleanly.
  - date: '2026-06-30'
    status: active
    who: stkrolikiewicz
    note: Promoted to active to begin diagnosis (root cause a/b/c).
---

# BUG: E23 LP participants page sum > total_shares

## Summary

Task 0252 Phase D E23 sanity check found one pool where the sum of
participant shares (across the first 150 LPs by shares DESC) is ~3×
the pool's reported `total_shares`. Should be ≤ total by construction.

## Repro

```python
# /tmp/0252 on sorban-prod
import sys
sys.path.insert(0, "/tmp/0252")
from phase_d_e23 import fetch_participants, latest_total_shares
from decimal import Decimal

pool = "510faa345a..."  # full hex in
                        # /tmp/sbe-artifacts/0252/phase_d_e23.tsv

s = Decimal(0)
for page in range(3):
    cursor = None if page == 0 else cursor
    rows = fetch_participants(pool, cursor)
    for r in rows:
        s += Decimal(r["shares"])
    if rows:
        last = rows[-1]
        cursor = (Decimal(last["shares"]), int(last["account_id"]))
print(f"page_sum={s}, total={latest_total_shares(pool)}")
```

## Plan

1. Reproduce on the exact pool: full participant sum (no LIMIT) +
   latest snapshot read.
2. Diagnose which of (a) / (b) / (c) above applies — likely (a) +
   (c) combined:
   - `SELECT count() FROM lp_positions WHERE pool_id = X`
     vs `SELECT count() FROM lp_positions FINAL WHERE pool_id = X`
   - check distinct (account_id, ledger_sequence) tuples per
     account to see if multiple un-deduped rows survive FINAL
   - sample a few accounts with non-zero `shares` and verify
     Horizon agrees they hold the position
3. If parser issue: fix in
   `crates/xdr-parser/src/state.rs::extract_lp_positions` (the
   "trustline removed" path may be dropping a remove event).
4. If FINAL/Replacing issue: schedule `OPTIMIZE TABLE
lp_positions FINAL` on Hetzner CH as a one-shot.
5. If snapshot lag: investigate why argMax misses the latest
   `liquidity_pool_snapshots` row for this pool.
6. Re-run E23 across the full 5K pool sample (raise from 300) to
   measure the rate.

## Acceptance Criteria

- [ ] Root cause identified (a / b / c).
- [ ] Fix landed (parser, schema, or runbook OPTIMIZE).
- [ ] E23 re-run on 5K pools reports `shares_bounded_by_total.fail
= 0`.
- [ ] **Docs updated** — if the fix touches parser, update
      `docs/architecture/xdr-parsing/xdr-parsing-overview.md`;
      if schema/OPTIMIZE, update
      `docs/architecture/database-schema/database-schema-overview.md`
      or a runbook under `docs/runbooks/`.
