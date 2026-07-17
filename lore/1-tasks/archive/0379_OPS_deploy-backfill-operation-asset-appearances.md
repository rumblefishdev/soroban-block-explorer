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
      **Phase 2 validation and the Phase 3 drain both ran 2026-07-16 and passed**;
      this file was simply never updated, which is why it still read "task not
      done" a day later. Their results are now transcribed in full above, recovered
      from the execution session's own record rather than re-run: validation was a
      COMPLETE PASS (referential integrity clean — 55/55, 170/170, 0 orphans in a
      10k tip window; 5/5 samples matching Horizon / stellar.expert / raw XDR; Circle
      USDC per-asset attribution exact), and the drain completed all four steps with
      `repair-tier1` correcting 14.33M accounts / 107728 lp_positions / 12835 nfts /
      439062 nfts_pending / 129121 soroban_contracts, dry == real.
      A genuine by-product: the Ada audit's reported orphan was proven a **phantom**
      (a transcription error in its hand-built tuple list), voiding its
      index-to-header-mismatch headline and mooting its "repair fan-out" follow-up.
      **What is NOT on the record, stated plainly:** (1) the full empty-oaa-range
      **coverage gap-scan** — the task's own warning that "watermarks reached !=
      coverage proven" still stands, and the Phase-2 pass tested referential
      integrity and sample correctness, which is not the same as proving no gap
      exists across all ~13M ledgers; (2) the **#8 `EXPLAIN indexes=1`** read-in-order
      check, attested but never captured. If a coverage question is ever raised
      against `operation_asset_appearances`, **run the gap-scan — do not cite this
      entry for it.**
      **This unblocks 0388**, which stays active: its ACs 1-2 ask for exactly this
      run's output, and the numbers now exist above (`soroban_contracts` = 129121
      corrected rows is its non-zero reconstruction criterion; no unknown-column
      error occurred across the 5 tables). 0388 needs those transcribed into it.
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
      stellar.expert — list + all detail variants. **Done 2026-07-16 — COMPLETE
      PASS, see "Validation (Phase 2)" below.**
- [x] **#8** read-in-order check — `EXPLAIN indexes=1` / `read_rows` on a hot
      asset. **Attested as run; output not captured.** The weakest item on the
      record — see the closing history entry.

## Validation (Phase 2, executed 2026-07-16, prod read-only)

Run with the `/compare-with-stellar-api` skill. **COMPLETE PASS.**

1. **Referential integrity — CLEAN.** native-XLM `oaa` keys resolved against
   `transactions`: backfill **55/55** across 3 regions of the range; live tip
   (ledger 63502776) **170/170**; a 10k-ledger tip window returned **0 orphans**.
2. **Correctness — 5/5 samples match** across Horizon (2/5; the other 3 sit below
   the **57195361** retention floor), stellar.expert (5/5) and raw XDR (5/5 —
   the authoritative leg).
3. **Per-asset attribution — PASS.** Circle USDC (surrogate
   `-6422464080247619664`, issuer `GA5ZSEJY…KZVN`): 3 transactions decoded, every
   USDC leg carrying the exact Circle issuer; native XLM and USDC both present in
   path-payment paths and multi-leg offers, including a failed transaction.
4. **The Ada audit's reported orphan is a PHANTOM.** Tx id
   `6643554620678510641` at ledger 63502776 exists in neither `oaa` nor
   `transactions` anywhere — a transcription error in that audit's hand-built
   tuple list (it self-noted 2 other corrections). Its "index-to-header mismatch"
   headline is **void**, and its follow-up #2 ("repair fan-out") is moot.

**Query gotcha found here:** `oaa` / `transactions` with `FINAL` over a wide
ledger range OOMs the 5.59 GiB cap. Post-OPTIMIZE partitions 100-126 are
single-part and not live-written, so drop `FINAL` and bound the ledger window.

## Phase 3 — Tier-1 drain (executed 2026-07-16)

Indexer stopped (ESM `27553d98…`, tip frozen at 63503839), box binary rebuilt
with #336 + 0394, all steps ClickHouse-only:

| step                                        | result                                                                                                                                                                                                             |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **2a** `soroban-token-flow-backfill` (#332) | 275.55M `transaction_participants` + 299.96M `oaa` rows (300.86M events scanned). **Must precede repair-tier1** — it feeds its participants.                                                                       |
| **2b** `repair-tier1`                       | accounts **14.33M**, lp_positions **107728**, nfts **12835**, nfts_pending **439062**, soroban_contracts **129121**. dry == real; EXCHANGE row-count-preserving; `first_seen > last_seen` = **0**.                 |
| **2c** `contract-type-rebuild`              | flipped_nft 105, flipped_fungible 3738, assets_inserted 0.                                                                                                                                                         |
| **2d** `nft-reclassify`                     | promoted 13037, promoted_ownership 21716, dropped_pending 438788 + 953762, **dropped_legacy 0** (hot NFT untouched). Hot nfts → 13037 / 66 collections (was 12835 / 60); pending → 274; max_minted 63501944 ≤ tip. |

`repair-tier1` **OOMed on the first attempt** (Code 241): #332's +275M rows made
`accounts.first_seen = min(ledger) GROUP BY … LEFT JOIN accounts FINAL` exceed the
6 GB `default` cap. Fixed by editing the `default` profile in `users.d/timeouts.xml`
in place (`max_memory_usage` 6→20 GB, `max_bytes_before_external_group_by=3e9`,
`join_algorithm=grace_hash`) + `SYSTEM RELOAD CONFIG` — per-query `.with_setting()`
does **not** reach the wire on CH 26.3, only profile XML does. **Profile reverted
afterwards**; `grace_hash` had leaked to `api_reader` (it inherits `default_profile`)
and broke LP-detail's constant-ON join.

## Acceptance Criteria

- [x] table created on prod, backfill (re-index) write complete for the Soroban
      era — 2026-07-16
- [x] sample assets validated byte-identical vs prod-before / external sources —
      **met 2026-07-16, COMPLETE PASS**; see "Validation (Phase 2)" above for the
      per-check results.
- [x] #8 read-in-order confirmed on real data — **attested, output not recorded.**
      The one criterion here without a captured result.

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
done"; both items had in fact been executed on 2026-07-16 and this file was
simply never updated):

- **Phase G / Phase 2 validation** — sample-asset cross-check
  (`/compare-with-stellar-api`) vs Horizon / stellar.expert / raw XDR: **DONE,
  COMPLETE PASS** — results transcribed into "Validation (Phase 2)" above.
  Referential integrity clean, 5/5 samples match, per-asset attribution passes.
  **Not done:** the full empty-oaa-range coverage gap-scan, and the `EXPLAIN
indexes=1` read-in-order check. See the closing history entry.
- **Phase 3** — `repair_tier1` and the surrounding Tier-1 drain: **DONE
  2026-07-16**, full step-by-step results in "Phase 3 — Tier-1 drain" above.
  Its gate, PR #336 (the `name`-column fix from 0388), merged 2026-07-14 as
  `7a99423c`. **This unblocks 0388**, whose ACs 1-2 ask for exactly this run's
  output — the numbers are in the Phase-3 table above (notably
  `soroban_contracts` = 129121 corrected rows, satisfying its non-zero
  reconstruction criterion, and no unknown-column error across the 5 tables).
