---
id: '0266'
title: 'OPS: 3-machine S3 re-parse + INSERT migration for path_payment pool_ids backfill'
type: OPS
status: backlog
related_adr: ['0033', '0044', '0045']
related_tasks: ['0228', '0252', '0261', '0267', '0268']
tags:
  [priority-medium, effort-large, ops, hetzner, parser, backfill, milestone-2]
milestone: 2
links:
  - lore/1-tasks/backlog/0261_BUG_parser-missing-pool-id-on-path-payment-ops.md
  - lore/2-adrs/0033_soroban-events-appearances-read-time-detail.md
  - lore/2-adrs/0044_clickhouse-pilot-parallel-store.md
  - lore/2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md
history:
  - date: '2026-05-25'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0261 plan. Once Phase 1 parser fix lands and the
      locked SHA is known, run a 3-machine S3 re-parse over the
      full Soroban-era retention window to backfill the missing
      `pool_id` on path_payment appearance rows for historical
      ingestion. Mirror the proven 0228 partial-backfill split
      (3-way per-ledger-range partition) + ADR 0045 FREEZE +
      rsync + ATTACH PART transport. Leverages ReplacingMergeTree
      dedup on `operations_appearances` (per-op sort key — see
      0265 schema docs clarification) so an INSERT with the same
      `(ledger_sequence, transaction_id, application_order)` tuple
      and the correct `pool_id` replaces the existing NULL row at
      next background merge.

      Decision on Option A (scalar `pool_id`, multi-hop loss) vs
      Option B (schema migration to `pool_ids Array` — task 0268)
      gates the exact shape of the INSERT payload. Default plan is
      Option A first → record multi-hop gap in artifact → run 0268
      schema migration as follow-up.
---

# OPS: 3-machine S3 re-parse + INSERT migration for path_payment pool_ids backfill

## Summary

Backfill the missing `operations_appearances.pool_id` for
historical `path_payment_strict_send` and `path_payment_strict_receive`
ops by re-parsing the affected ledger range from the public S3 XDR
archive on three parallel machines, then INSERTing the corrected
rows into Hetzner CH. ReplacingMergeTree handles the dedup
automatically because the sort key is per-op.

Close-out unblocks task 0267 (E20 re-validate — confirms 100 %
coverage post-migration) and removes the documented gap in the
`endpoint_validation_20260525.md` artifact.

## Why now

- Task 0261 ships the parser fix forward-only — new ingestion is
  correct from the moment 0241 deploys. Historical rows in the
  Hetzner backfill snapshot (5.71 M pool-touching ledgers across
  the retention window) still carry `pool_id = NULL` for
  path-payment ops.
- E20 endpoint compare (0252) measured ~6 % per-pool hash-set
  divergence vs Horizon because of this gap.
- Frontend `/liquidity-pools/:id/transactions` under-reports
  swap activity until backfill lands.

## Architecture (mirror task 0228)

```
Retention window: 56,657,428 → 62,527,999  (~5.87 M ledgers)
                  ↓
       Split per-ledger-range (3 partitions)
                  ↓
   ┌──────────────┼──────────────┐
   ↓              ↓              ↓
Machine 1     Machine 2     Machine 3
ledgers       ledgers       ledgers
N₁..N₂        N₂..N₃        N₃..N₄
   ↓              ↓              ↓
download      download      download
s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/...
   ↓              ↓              ↓
xdr-parser    xdr-parser    xdr-parser
(locked SHA   (locked SHA   (locked SHA
 = 0261       …)            …)
 Phase 1
 merge)
   ↓              ↓              ↓
INSERT into   INSERT into   INSERT into
CH local      CH local      CH local
   ↓              ↓              ↓
FREEZE +      FREEZE +      FREEZE +
rsync to      rsync to      rsync to
Hetzner       Hetzner       Hetzner
ATTACH PART   ATTACH PART   ATTACH PART
per partition per partition per partition
                  ↓
       merged into operations_appearances
                  ↓
       OPTIMIZE TABLE … FINAL (force merge)
                  ↓
       0267 verify (E20 re-validate)
```

## ReplacingMergeTree dedup pattern

`operations_appearances` engine = `ReplacingMergeTree`, sort key =
`(ledger_sequence, transaction_id, application_order)`. Per-op
uniqueness. INSERTing a new row for the same `(ledger, tx,
app_order)` triple with the correct `pool_id` set:

- Both old (NULL) and new (LP_n) rows coexist until next merge.
- Background merge keeps the latest insert (insert order =
  tiebreaker since the table has no version column).
- API queries that filter `WHERE pool_id = X` start matching the
  new rows immediately — old NULL rows do not match the filter.
- For audit queries on `WHERE pool_id IS NULL`, stale results
  return until merges complete; force with `OPTIMIZE TABLE …
FINAL` after the INSERT batch.

## Sequence

1. **Locked SHA** — 0261 Phase 1 parser fix merged to develop; pin
   the commit SHA for all three machines.
2. **3-machine prep** — re-spin or reuse the original task 0228
   parallel-backfill hosts. Ensure they have AWS CLI / Rust
   xdr-parser binary built at locked SHA.
3. **Split ledger range** — assign 3 contiguous ranges covering
   `56,657,428 → 62,527,999`. Document the partition boundaries
   in this task's history.
4. **Per-machine run** (parallel) — for each ledger in the assigned
   range:
   - Download `.xdr.zst` from
     `s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/.../FC…--<ledger>.xdr.zst`
   - Run `xdr-parser` extracting `operations_appearances` rows
     **only** for op types 2 / 13 (path_payment) that crossed
     pools per op_meta.
   - INSERT to the per-machine CH local instance.
5. **Transport** (mirror ADR 0045) — `clickhouse-client … FREEZE
PARTITION …` on the local CH, `rsync` the frozen part directory
   to Hetzner `/srv/clickhouse-data/store/.../detached/`, then
   `ALTER TABLE … ATTACH PART …`.
6. **Verify gates** (HARD STOP before forcing merge):
   - Row count delta within expected envelope (per the row-growth
     analysis in 0261, +2–5 % total operations_appearances rows).
   - FK resolve sample (every new `pool_id` is in
     `liquidity_pools FINAL`).
   - Spot-check 50 random `(ledger, tx, app_order)` triples:
     `WHERE pool_id IS NOT NULL` row content matches the freshly
     parsed op-meta.
7. **Force merge** — `OPTIMIZE TABLE operations_appearances FINAL`.
   Long-running; run in `tmux` with file-fd output.
8. **Re-validate** via task 0267 → E20 compare hash-set ratio
   expected ≥ 99 % (single-hop), 100 % if 0268 Array schema also
   landed.

## Risk + mitigations

- **Live writes during migration**: 0241 deploy (live mode
  cutover) ships before this task fires. New rows accumulate in
  `operations_appearances` during the multi-day migration window.
  Mitigation: re-parse range is **strictly historical** (≤
  pre-0241-deploy max ledger); new live rows untouched by INSERTs.
- **Disk space**: re-parse only emits `(ledger, tx, app_order)`
  rows where we previously had `pool_id = NULL` and now have a
  derived `pool_id`. Estimated +2–5 % to the operations_appearances
  table size before merges run (≈ +10–25 GiB depending on table
  compression). Audit Hetzner `df -h /srv/clickhouse-data` before
  starting; require ≥ 100 GiB headroom for safety.
- **Insertion-order tiebreaker** in ReplacingMergeTree dedup:
  ensure parser-machine clocks + insert sequence place the new
  rows AFTER the existing NULL rows on the Hetzner timeline. They
  will because the parse + INSERT happens after the backfill that
  produced the NULL rows; no special handling required.
- **0268 dependency**: if Option B (Array column) is chosen, this
  task's INSERT payload changes from `pool_id Nullable(FixedString(32))`
  to `pool_ids Array(FixedString(32))`. Pick A/B before kickoff.

## Acceptance Criteria

- [ ] 0261 Phase 1 parser fix merged to develop; commit SHA
      pinned in this task's history.
- [ ] 3-machine split + per-machine ledger ranges documented in
      task history.
- [ ] Hetzner CH disk headroom verified ≥ 100 GiB before kickoff.
- [ ] All three machines complete their range; per-machine row
      counts + timing captured.
- [ ] FREEZE + rsync + ATTACH PART per partition completed on
      Hetzner; verify-gates pass.
- [ ] `OPTIMIZE TABLE operations_appearances FINAL` completed.
- [ ] Task 0267 (E20 re-validate) shows hash-set ratio ≥ 99 %
      (or 100 % if 0268 landed).
- [ ] `endpoint_validation_<YYYYMMDD>.md` artifact updated with
      post-migration E20 verdict.
- [ ] **Docs updated** — N/A (no schema or API contract change here).
- [ ] **API types regenerated** — N/A.

## Notes

- Wall-clock estimate: ~24 h end-to-end with 3 machines ×
  parallel-8 workers each. Resumable on per-ledger granularity —
  individual ledger failures retry without restarting the batch.
- 0241 deploy unblocks ingestion correctness going forward;
  this task closes the historical gap. Both required for the M2
  pool-tx audit story.
