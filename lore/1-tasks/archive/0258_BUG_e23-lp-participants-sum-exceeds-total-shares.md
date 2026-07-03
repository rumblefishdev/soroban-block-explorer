---
id: '0258'
title: 'BUG: E23 LP participants page sum > liquidity_pool_snapshots.total_shares'
type: BUG
status: completed
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
  - date: '2026-06-30'
    status: completed
    who: stkrolikiewicz
    note: >
      NOT-A-BUG. Diagnosed on sorban-prod (app-clickhouse-1). Root
      cause is none of (a/b/c): the production endpoint query and the
      underlying data are both correct. The original 3.0000× was an
      artifact of the Phase-D E23 validation harness pagination. Data
      invariant `Σ lp_positions.shares (FINAL) ≤ total_shares` verified
      across ALL 26,375 pools: fail=0, max_ratio=1.0. No production
      fix, no parser/schema/OPTIMIZE change. See Resolution section.
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

- [x] Root cause identified — **none of (a/b/c)**. Validation-harness
      pagination artifact; production query + data both correct.
- [x] Fix landed — **N/A, no production defect**. No parser, schema,
      or `OPTIMIZE` change required.
- [x] E23 re-run reports `shares_bounded_by_total.fail = 0` — verified
      across **all 26,375 pools** (not just 5K), `max_ratio = 1.0`.
- [x] **Docs updated** — **N/A**: no system-shape change (no schema,
      API, parser, ingestion, or infra change). Legitimate N/A per
      [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).

## Resolution (2026-06-30)

**Not a bug.** Diagnosed live on `sorban-prod` / `app-clickhouse-1`.

### Root cause

The original `page_sum = 1,556,294,929,934.07 = 3.0000 × total_shares`
came from the **Phase-D E23 validation harness** (`phase_d_e23.py` on
prod `/tmp/0252`), not from production. The endpoint's keyset cursor is
on the numeric `(shares, account_id)`, but the endpoint returns the
StrKey as `account`; the harness rebuilt its cursor from the wrong
field, so it never advanced and summed the **same top page across all
three `page()` iterations** → exactly `3 × total`. "150 rows = 3 × 50"
and the precise `3.0000×` are the fingerprint of three identical pages.

The three hypotheses in the original plan are all disproved:

- **(a) lp_positions FINAL not deduping** — disproved. `Σ lp_positions
FINAL (shares>0)` equals the snapshot total _exactly_ on the flagged
  pool, and `max_lp_per_acct_FINAL = 1`.
- **(b) snapshot lag** — disproved. Latest snapshot (ledger 58700382)
  carries the current `total_shares`; no lag.
- **(c) un-zeroed withdrawn rows** — disproved by the exact-equality
  invariant.

### Evidence

Flagged pool `510FAA345A1B8577CD2973722800614003FF8073BEBD2ABCAECC1981C8F8E9BE`:

| metric                                        | value                                |
| --------------------------------------------- | ------------------------------------ |
| `Σ lp_positions FINAL (shares>0)`             | `518764976644.6898527`               |
| `argMax(total_shares)` (latest snapshot)      | `518764976644.6898527` (exact match) |
| endpoint `JOIN accounts acc FINAL` rows / sum | 1 / `518764976644.69` (no fan-out)   |

`FINAL`-on-join proven to deduplicate (pool-independent test): account
`-2641338041311796945` has **11** raw Replacing parts → `FINAL` = 1 →
`JOIN accounts acc FINAL` = 1. The endpoint query in
[23_get_liquidity_pools_participants.sql](../../../docs/architecture/database-schema/endpoint-queries-clickhouse/23_get_liquidity_pools_participants.sql)
is correct as written.

Invariant `Σ lp_positions.shares (FINAL) ≤ total_shares` at scale —
**all 26,375 pools** with positive LP shares + a snapshot:

```
pools_checked=26375  fail_exceeds_1bps=0  fail_exceeds_exact=0  max_ratio=1.0
```

(`lp_positions` total: 108,060 rows / 39,582 pools — small enough that
the whole population was checked instead of a 5K sample.)

### Design Decisions — Emerged

1. **Closed as not-a-bug rather than patching the harness.**
   `phase_d_e23.py` is a throwaway validation script on prod `/tmp`
   (task 0252, already archived); the real signal — the data invariant
   — is validated directly in SQL. Fixing an ephemeral scratch script
   adds no value.
2. **Validated all 26,375 pools, not the 5K sample the AC asked for.**
   `lp_positions` is only ~108k rows, so full-population coverage was
   cheaper than sampling and removes the sampling caveat.
3. **No follow-up spawned for the accounts dup.** Up to 11 un-merged
   Replacing parts per `accounts.id` exist, but `FINAL`-on-join
   deduplicates them correctly on every read, so they are harmless to
   this and every other FINAL-reading endpoint.
