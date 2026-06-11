---
id: '0266'
title: 'OPS: 3-machine S3 re-parse + INSERT migration for path_payment pool_ids backfill'
type: OPS
status: backlog
related_adr: ['0033', '0044', '0045']
related_tasks:
  ['0199', '0228', '0247', '0252', '0261', '0267', '0268', '0279', '0281']
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
  - date: '2026-06-09'
    status: backlog
    who: stkrolikiewicz
    note: >
      Scope extension (per 0261 Decision): this re-parse holds the path-payment
      `ClaimLiquidityAtom`s, so it must ALSO compute and INSERT `gross_volume_a`
      per (pool, ledger) into `liquidity_pool_snapshots`, not just `pool_id` into
      `operations_appearances`. One parse pass, two write targets. Capturing
      gross_volume_a here (even though USD volume/fee stay off until the Prices
      API, ADR 0048) avoids a second full re-parse of the range later. Default
      Option A (scalar) is superseded toward emitting the full pool list per the
      claim-atom extractor.
  - date: '2026-06-10'
    status: backlog
    who: stkrolikiewicz
    note: >
      Audit pass (0261 plan review). Four deltas: (1) the INSERT payload must
      be the COMPLETE per-tx fold output for any tx containing a path-payment
      op — pool_ids refines the 0163 fold identity, so groups can split;
      emitting only pool-touching rows leaves stale `amount` fold counts
      behind. (2) Targeted write only (op rows + snapshot rows) — running the
      full persist pipeline would re-emit nfts_pending and re-leak the 0221
      SAC rows into drained partitions. (3) Range widens to the full backfill
      range 50,457,424 → W (W = 0281-window indexer deploy ledger, pinned at
      kickoff): claim atoms come from result XDR, no retention-state
      dependency — the old 56.6M floor was an asset-pair-lookup constraint.
      (4) Preconditions: 0268 ALTERs applied (pool_ids + gross_volume_a),
      oa_pool_seek projection dropped pre-INSERT, the 0279 per-op-amounts
      payload decision recorded before kickoff (one-pass rule), fresh
      snapshot. Transport default flips to direct INSERT; ADR 0045
      FREEZE/rsync/ATTACH stays as bandwidth fallback.
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
Range: 50,457,424 → W (window deploy ledger; ~12 M+ ledgers)
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
INSERT over   INSERT over   INSERT over
WAN to        WAN to        WAN to
Hetzner       Hetzner       Hetzner
(ADR 0045     (ADR 0045     (ADR 0045
 fallback)     fallback)     fallback)
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

### `liquidity_pool_snapshots` — FULL-ROW replace, not column patch

`liquidity_pool_snapshots` is **also** a version-less `ReplacingMergeTree`
(`ORDER BY (pool_id, ledger_sequence)`). RMT replaces the **entire** row for a
key at merge time — there is no per-column UPDATE. Live ingest already wrote
`(pool, ledger)` rows with `gross_volume_a = NULL` and the correct
`reserve_a/reserve_b/total_shares/tvl`. The backfill INSERT for the same key
must therefore carry the **complete, correct row** (re-derived reserves + the
new `gross_volume_a`), NOT a sparse row with only `gross_volume_a` set —
otherwise the merge silently nulls/zeroes the reserves. Symmetric hazard: any
live re-ingest of that key _after_ the backfill, writing `gross_volume_a =
NULL`, reverts the backfilled value. Mitigations: (a) restrict the backfill
range to `≤ W` (the window deploy ledger) so live never re-touches a
backfilled key (already the plan); (b) emit full snapshot rows; (c) verify
post-merge that `countIf(gross_volume_a IS NOT NULL)` matches the expected
pool-touching-ledger count and that reserves are unchanged vs a pre-backfill
sample.

## Sequence

Preconditions (2026-06-10 audit):

- 0268 ALTERs applied on Hetzner: `pool_ids` (ADD + MATERIALIZE,
  online, pre-window) and `liquidity_pool_snapshots.gross_volume_a`.
- `oa_pool_seek` projection DROPPED before any INSERT (avoids
  per-part projection rebuild during the backfill; unblocks the
  eventual `DROP COLUMN pool_id`). Replacement seek lands per 0281 C.
- 0279 per-op LP amounts payload decision recorded: deposit/withdraw
  amounts come from `LedgerEntryChanges` (0247 Path B), claim atoms
  only cover trades — decide whether this run carries the 0279
  payload BEFORE kickoff; the alternative is a second 12M-ledger
  pass.
- Fresh Hetzner snapshot (0272 precedent: RESTORE of 690 GiB = 642 s).

1. **Locked SHA** — 0261 Phase 1 parser fix merged to develop; pin
   the commit SHA for all three machines.
2. **3-machine prep** — re-spin or reuse the original task 0228
   parallel-backfill hosts. Ensure they have AWS CLI / Rust
   xdr-parser binary built at locked SHA.
3. **Split ledger range** — assign 3 contiguous ranges covering
   `50,457,424 → W` (W = ledger from which the 0281-window indexer
   redeploy writes `pool_ids` itself; pin W + the partition
   boundaries in this task's history).
4. **Per-machine run** (parallel) — for each ledger in the assigned
   range:
   - Download `.xdr.zst` from
     `s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/.../FC…--<ledger>.xdr.zst`
   - Run `xdr-parser` (claim-atom extractor, locked 0261 SHA). For
     every tx containing ≥ 1 path-payment op, emit the **complete
     `operations_appearances` fold output of that tx** — never just
     the pool-touching rows; `pool_ids` refines the 0163 fold
     identity, groups can split, and partial emission leaves stale
     `amount` fold counts (see 2026-06-10 history note). Also emit
     the `gross_volume_a` rows per `(pool, ledger)` for
     `liquidity_pool_snapshots`.
   - **Targeted write only** — do NOT run the full persist pipeline:
     no events / nfts_pending / participants re-emission (would
     re-leak 0221 SAC rows into drained partitions).
   - INSERT to the per-machine CH local instance (or buffer files).
5. **Transport** — default: direct `INSERT` over WAN into Hetzner CH
   (payload is a small fraction of full parts; sparse re-parse parts
   make ATTACH awkward). Fallback if WAN throughput disappoints:
   ADR 0045 `FREEZE PARTITION` → rsync → `ATTACH PART`.
6. **Verify gates** (HARD STOP before forcing merge):
   - Row count delta within expected envelope (per the row-growth
     analysis in 0261, +2–5 % total operations_appearances rows).
   - FK resolve sample: new `pool_id`s resolve in
     `liquidity_pools FINAL`; pools created + removed entirely
     before the retention floor may legitimately miss (claim atoms
     reach further back than retained pool state) — record as
     tolerance, not failure.
   - Spot-check 50 random `(ledger, tx, app_order)` triples:
     `WHERE pool_id IS NOT NULL` row content matches the freshly
     parsed op-meta.
7. **Force merge** — `OPTIMIZE TABLE operations_appearances FINAL`.
   Long-running; run in `tmux` with file-fd output.
8. **Re-validate** via task 0267 → E20 compare hash-set ratio
   expected ≥ 99 % (single-hop), 100 % if 0268 Array schema also
   landed.

## Risk + mitigations

- **Live writes during migration**: live ingestion is already
  running (post-0241; the first cutover failed and was rolled back
  via 0272 — hence the snapshot precondition). Re-parse range is
  **strictly historical** (≤ W, the window deploy ledger); rows
  ≥ W are written correctly by the redeployed indexer; no overlap.
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
- **0268 dependency**: settled (2026-06-09/10) — the payload is
  `pool_ids Array(FixedString(32))` (Flow B; the 0268 ALTERs are a
  hard precondition). Scalar Option A is dead.

## Acceptance Criteria

- [ ] 0261 Phase 1 parser fix merged to develop; commit SHA
      pinned in this task's history.
- [ ] Preconditions met + recorded in history: 0268 ALTERs applied,
      `oa_pool_seek` dropped, 0279 payload decision, fresh snapshot.
- [ ] 3-machine split + per-machine ledger ranges documented in
      task history.
- [ ] Hetzner CH disk headroom verified ≥ 100 GiB before kickoff.
- [ ] All three machines complete their range; per-machine row
      counts + timing captured.
- [ ] Rows landed on Hetzner (direct INSERT default; ADR 0045
      transport fallback); verify-gates pass.
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
