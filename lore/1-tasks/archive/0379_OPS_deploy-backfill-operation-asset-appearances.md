---
id: '0379'
title: 'OPS: deploy + backfill operation_asset_appearances (0359 classic write-side)'
type: OPS
status: completed
related_adr: []
related_tasks: ['0359', '0365', '0388']
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
  - date: 2026-07-17
    status: completed
    who: stkrolikiewicz
    note: >
      Closed. The full job ran on the prod box 2026-07-13→17: manual
      `CREATE TABLE` + pre-backfill `BACKUP` (07-13), then a `backfill-runner run
      --reindex` re-parse of **50,457,424–63,460,100** (~13M ledgers) via s5cmd
      pre-fetch, supervisor-governed overnight at ~127-150k ledgers/hr
      (persist-bound; >6 workers gave nothing). Full `OPTIMIZE 100..126` auto-ran,
      `optimize.err = 0`. `operation_pools` (0365) rode the same window via
      `INSERT … SELECT` with no re-parse. Runbook + worked example:
      `docs/runbooks/backfill_derived_table_reparse_hetzner.md`.
      **Phase G and Phase 3 are recorded as executed on the operator's attestation
      (2026-07-17), not on evidence held in this task.** Stated plainly for whoever
      reads this next: no verified row count, no sample-asset comparison output and
      no `EXPLAIN indexes=1` result were captured, and nothing in git records either
      pass. The task's own text had said "watermarks reached != coverage proven".
      If a coverage question is ever raised against `operation_asset_appearances`,
      **re-run the gap-scan rather than citing this entry** — it attests that the
      check happened, not what it returned.
      Phase 3 (`repair_tier1`) ran here after its gate, PR #336, merged 07-14
      (`7a99423c`). **That unblocks 0388**, which stays active: its ACs 1-2 ask for
      the dry-run's actual output (no unknown-column error across the 5 tables, and
      a non-zero corrected-row count for the `soroban_contracts` `deployer_id` /
      `deployed_at_ledger` reconstruction), and those numbers are still unrecorded.
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
- [x] Validate sample assets (incl. native + a type-3 token) vs Horizon /
      stellar.expert — list + all detail variants. **Phase G executed on the prod
      box 2026-07-17 — attested by the operator, run record not captured (see the
      closing history entry).**
- [x] **#8** read-in-order check — `EXPLAIN indexes=1` / `read_rows` on a hot
      asset. **Executed as part of the same Phase-G pass — attested, output not
      captured.**

## Acceptance Criteria

- [x] table created on prod, backfill (re-index) write complete for the Soroban
      era — 2026-07-16 (coverage gap-scan is the next criterion)
- [x] sample assets validated byte-identical vs prod-before / external sources
      — Phase-G gap-scan + Horizon / stellar.expert **run on the prod box
      2026-07-17. Attested by the operator; no row counts, sample list or
      comparison output were captured into this task.** The criterion is recorded
      as met on that attestation, not on evidence held here.
- [x] #8 read-in-order confirmed on real data — same pass, same caveat: attested,
      `EXPLAIN` output not recorded.

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

**Closed out 2026-07-17** (this section previously read "Still open — task not
done"; both items were executed on the prod box, see the closing history entry
for what is and is not on the record):

- **Phase G** — gap-scan `operation_asset_appearances` vs `ledgers` / Horizon +
  sample-asset cross-check (`/compare-with-stellar-api`) + `EXPLAIN indexes=1`.
  **Executed.** Attested by the operator; the run record (row counts, sample list,
  `EXPLAIN` output) was not captured into this task.
- **Phase 3** — `repair_tier1` after PR #336 (indexer STOP → `repair-tier1
--dry-run` → `repair-tier1` → `nft-reclassify` → validate → START). **Executed**
  under this task. PR #336 (the `name`-column gate from 0388) merged 2026-07-14 as
  `7a99423c`, lifting the blocker. **This unblocks 0388**, whose ACs 1-2 require
  exactly this prod run — 0388 must be updated with the dry-run's actual output.
