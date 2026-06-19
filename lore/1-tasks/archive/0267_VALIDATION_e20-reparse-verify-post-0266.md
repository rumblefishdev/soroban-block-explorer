---
id: '0267'
title: 'VALIDATION: E20 re-run post-0266, confirm 100 % path-payment pool coverage'
type: VALIDATION
status: completed
related_adr: []
related_tasks: ['0252', '0261', '0266', '0268']
tags: [priority-medium, effort-small, layer-validation, milestone-2]
milestone: 2
links:
  - docs/runbooks/artifacts/e20_validation_20260618.md
  - docs/runbooks/artifacts/endpoint_validation_20260525.md
  - scripts/0267/compare_e20_ch.py
  - lore/1-tasks/backlog/0266_OPS_3machine-s3-reparse-path-payment-pool-ids.md
history:
  - date: '2026-05-25'
    status: backlog
    who: stkrolikiewicz
    note: >
      Closing verification for the 0261/0266 fix arc. Re-run
      `compare_e20.py` against the Hetzner CH state after task
      0266 (3-machine S3 re-parse + INSERT migration) lands and
      the background merge completes. Expect the hash-set-equal
      pass rate to climb from the current ~91 % (0252 post-launch
      verdict — 18/200 anchors with diff) to ~99 % (single-hop
      coverage with Option A scalar `pool_id`) or 100 % (with
      Option B Array via task 0268).
  - date: '2026-06-17'
    status: backlog
    who: stkrolikiewicz
    note: >
      Pre-validation spot-check during the 0281 window (after the 0266
      transport + per-partition OPTIMIZE landed). Took the top path-payment
      pool by op count (a01fce…512159, 3.7M type-2/13 ops) and verified 10
      recent path-payment txs three ways in parallel: Horizon effects,
      stellar.expert result-XDR, and an independent py-stellar-sdk decode of
      the result XDR (ground truth). 10/10 CONFIRMED — the target pool id is
      literally present in the path-payment ClaimLiquidityAtoms of every tx
      (each a 3-hop path payment crossing 3 pools), proving the backfill read
      the claim atoms correctly and that Array(pool_ids) captures multi-hop.
      CH attribution matches chain truth. The FORMAL compare_e20.py (200
      anchors, hash-set) still pends step-5 redeploy — it hits the API
      endpoint, which needs the has(pool_ids) Lambda live. Also surfaced:
      idx_oa_pool_ids (the bloom "E20 floor") full-scans for POPULAR pools
      (present in ~every granule), so the bounded prod seek (0281 C) is
      required before the endpoint serves top pools within the api_reader
      quota.
  - date: '2026-06-17'
    status: backlog
    who: stkrolikiewicz
    note: >
      Second spot-check — a SPARSE pool (3d53f0b2…, last activity ledger
      57.27M < W), where the whole tx set fits one comparison so both recall
      AND precision are checkable. Recall: PERFECT — CH holds all the pool's
      genuine transactions (≤W), 0 missed. Precision: 11/11 correct. A parallel
      verifier subagent flagged 1 "over-attribution" (tx 90c05ae9 at the pool's
      CREATION ledger 52,977,190), but the CH-side op-type check OVERTURNED it:
      that tx is change_trust (op1, pool_ids=[]) + liquidity_pool_deposit (op2,
      type 22, pool_ids=[3d53f0b2]) — the pool birth + initial deposit, which
      genuinely touches the pool. The subagent was misled because Horizon 404s
      the pool's old ops (retention) → it byte-inspected the XDR, misread the
      deposit as order-book, and conflated "not in the trades list" with
      "doesn't touch the pool" (a deposit is not a trade). Lesson: confirm a
      subagent's attribution claim against CH directly when the external source
      is retention-limited. Net: backfill recall AND precision validated on both
      a POPULAR pool (10/10 path-payment crossings) and a SPARSE pool (11/11 =
      10 trades + 1 deposit). Strong green ahead of the formal compare_e20.py
      (200 anchors), which still pends the step-5 redeploy.
  - date: '2026-06-18'
    status: completed
    who: stkrolikiewicz
    note: >
      PASS. Ran the formal E20 directly against prod CH (200 anchors) via a
      committed reimplementation — scripts/0267/compare_e20_ch.py — since the
      0252 compare_e20.py was never committed and is lost. The new harness
      queries CH directly (docker exec, no API redeploy / no api_reader quota)
      and jumps Horizon to the <=W window with a TOID cursor (W+1)<<32 to skip
      live-drift. Result: evaluated=200 skipped=0, mean recall/precision 0.9999,
      min 0.9899/0.9847; tx-level diff 0.023% (9 tx / ~39.6k) vs the 0252
      baseline 1.87% — ~80x reduction, 195/200 anchors hash-set-identical. All 9
      residual diffs classified by CH-side op-type as CH<->Horizon semantics,
      not backfill error: recall -4 = change_trust (type 6) to pool-share, which
      CH does not attribute to a pool (0266 extracts pool_ids only from
      2/13/22/23); precision +3 = liquidity_pool_deposit (type 22) in failed
      txs (successful=false), which CH indexes and Horizon (successful-only)
      omits. Path-payment single- + multi-hop attribution confirmed correct.
      Artifact: docs/runbooks/artifacts/e20_validation_20260618.md. Closes 0267.
---

# VALIDATION: E20 re-run post-0266, confirm 100 % path-payment pool coverage

## Summary

Re-execute `scripts/0252/compare_e20.py` against the post-migration
Hetzner CH state. Confirm that the path-payment hash-set divergence
surfaced in 0252 E20 (1.87 % fail rate on the original run) drops
to the < 1 % tolerance band thanks to the parser fix + backfill
re-parse delivered by tasks 0261 + 0266 (and optionally 0268 for
multi-hop).

## Why

- 0252 Phase B Group A E20 verdict closed with `pass=943 fail=18`
  on 200 anchor pools (1.87 % fail rate, 0.12 % per pool average).
- Root cause: parser miss on `pool_id` for `path_payment_strict_*`
  ops crossing liquidity pools. Tracked in 0261.
- Fix sequence: 0261 Phase 1 (parser) → 0266 (re-parse + INSERT
  migration) → this task (re-run E20 → verify).
- Post-fix expected: ≥ 99 % hash-set ratio (single-hop), 100 %
  if Option B Array column (0268) also shipped.

## Procedure

1. Confirm 0266 close — `OPTIMIZE TABLE operations_appearances
FINAL` completed; system.merges has no pending merges on the
   table.
2. Clean previous E20 artifacts on Hetzner:
   ```bash
   rm -f /tmp/sbe-artifacts/0252/phase_b_e20.tsv \
         /tmp/sbe-artifacts/0252/phase_b_e20_summary.json
   rm -rf /tmp/sbe-artifacts/0252/diffs/E20
   ```
3. Re-run `compare_e20.py` (active pool sample, 200 anchors —
   same params as the original 0252 run):
   ```bash
   tmux new -d -s e20rerun \
     '/root/sbe-0252-venv/bin/python3 /tmp/0252/compare_e20.py \
       2>&1 | tee /tmp/sbe-artifacts/0252/e20_rerun_full.log'
   ```
4. Read `phase_b_e20_summary.json` — verify `fail_total` and
   `hash_set_equal` field counts.
5. Spot-check the `bb06b8082e…` / `43fa84e7…` tx hashes used in
   0252 E20 diagnosis — `operations_appearances` should now
   surface them under the correct `pool_id`.
6. Update `docs/runbooks/artifacts/endpoint_validation_20260525.md`
   (or spawn a new dated artifact) — E20 verdict moves from
   `1 fail / 0.12 %` to PASS.

## Acceptance Criteria

- [x] 0266 OPTIMIZE completed; no pending merges on
      operations_appearances. (Done during the 0266/0281 live window.)
- [x] E20 re-run shows ≥ 99 % pass rate (`fail_total <= 2`-equivalent):
      mean recall/precision 0.9999, tx-level diff 0.023 %, residual all
      semantic. Run via the committed `scripts/0267/compare_e20_ch.py`
      (direct-CH), not the lost `compare_e20.py`.
- [ ] Spot-check tx hashes from 0252 E20 diff (`43fa84e7…`,
      `bb06b8082e…`) — **not re-run; superseded** by the formal 200-anchor
      pass + full residual classification (strictly stronger than a 2-hash check).
- [x] Validation artifact written with re-run verdict + classification:
      `docs/runbooks/artifacts/e20_validation_20260618.md` (links 0261/0266/0268).
- [x] **Docs updated** — N/A (validation only; no system-shape change).
- [x] **API types regenerated** — N/A.

## Notes

- Sample stays 200 anchors so the comparison with the 0252
  baseline is apples-to-apples. Bump only if statistical envelope
  demands tighter bounds.
- If post-fix fail rate stays > 1 %, root-cause via diff dumps;
  most likely cause is multi-hop residue → 0268 Array column
  becomes required.

## Implementation Notes

- Harness `scripts/0267/compare_e20_ch.py` (stdlib, ~210 lines), committed,
  run on ch-prod-01. Full write-up + result table + residual classification in
  `docs/runbooks/artifacts/e20_validation_20260618.md`.

## Design Decisions

### Emerged

1. **Direct-CH, not the API endpoint** — `compare_e20.py` (0252) was lost and
   the API path needs the step-5 redeploy + `api_reader` quota; querying CH
   directly decouples E20 from the redeploy.
2. **TOID cursor jump `(W+1)<<32`** to skip live-drift — CH frozen at W, Horizon
   live ahead; the hottest pools' drift is thousands of txs, so paging back is
   infeasible. One Horizon request per pool lands the `≤ W` window.
3. **Residual left as semantic** — the 9-tx diff is change_trust-scope +
   failed-tx inclusion (op-type confirmed); no code change warranted.

## Issues Encountered

- First smoke SKIPped all top pools (recent `order=desc` page was all live-drift
  > W). Fixed via the cursor jump (commit `89ca1b55`) — harness windowing, not a
  > data issue.
