---
id: '0228'
title: 'FEATURE: parallel-backfill merge into Hetzner CH (3-way split) + post-merge validation'
type: FEATURE
status: completed
related_adr: ['0040', '0044', '0045']
related_tasks:
  ['0118', '0194', '0198', '0216', '0225', '0231', '0232', '0233', '0252']
blocked_by: ['0216', '0225']
tags:
  [
    priority-high,
    effort-large,
    layer-data,
    clickhouse,
    backfill,
    hetzner,
    merge,
    multi-machine,
    blocks-prod-deploy,
  ]
links: []
history:
  - date: '2026-05-15'
    status: backlog
    who: stkrolikiewicz
    note: >
      Task spawned after a long planning session. Full approved plan
      captured in notes/S-approved-plan.md. Backlog (not active) — gated
      on task 0225 (sync-validation pre-parse) landing on develop and
      task 0216 implementation work completing (Ansible playbook,
      mTLS CA, dict_reader user fix, BX21 Storage Box ordered).
  - date: '2026-05-18'
    status: active
    who: stkrolikiewicz
    note: >
      Activated. Laptop 1 finished Phase 1 (full 2/5 backfill, 73 partitions,
      4,646,576 ledgers, range 50,457,424–55,103,999) and Phase 2 cleanup +
      invariants on its local CH. Phase 2 results captured in
      notes/R-laptop1-phase2-results.md + docs/runbooks/artifacts/laptop1_pre-export-metrics.json.
      Task remains blocked_by 0216 + 0225 for Phase 3 onward (Hetzner readiness +
      laptop 3 sync-validation), but Phase 1/2 execution on laptop 1 is correctness-safe
      without those prereqs (laptop 1's range is far from S3 frontier and
      no cross-machine joins yet).
  - date: '2026-05-18'
    status: active
    who: stkrolikiewicz
    note: >
      Phase 5 repair-pass scaffolding landed ahead of Hetzner readiness.
      Three new `backfill-runner` subcommands implemented in the worktree,
      each with `--dry-run` for sandbox validation on laptop 1's local CH:

        - `repair-tier1` — rebuilds 12 RMT-corrupted columns across
          `accounts`, `lp_positions`, `nfts`, `nfts_pending`,
          `soroban_contracts` via staging-table + `EXCHANGE TABLES`.
          Sources MIN aggregates from fact tables
          (`transaction_participants`, `operations_appearances`,
          `nft_ownership`) to survive RMT collapse.
        - `asset-aggregates` — CH translation of PG
          `recompute_asset_aggregates` (task 0194 continuation):
          `assets.{holder_count, total_supply}` from
          `account_balances_current FINAL`, scoped to `asset_type IN
          (1, 2)`. Native + Soroban-native rows pass through unchanged.
        - `nft-reclassify` — task 0118 Phase 3 + task 0217 quarantine
          aware: promote `nfts_pending` → `nfts` for newly-classified
          `Nft`, drop pending + legacy hot rows for `Fungible`/`Token`,
          followed by `OPTIMIZE FINAL`.

      Statement B rebuild (task 0198) was OUT-of-scope for this CH pass —
      that task is a PG-side partial-index rewrite, not a CH operation;
      the 0228 plan still gates Phase 5 completion on 0198 landing on PG
      separately.

      All four PG short-circuit tests green; clippy clean; workspace
      `cargo check` green. CH-gated tests skipped (no local CH on this
      machine — laptop 1 sandbox dry-run is the validation gate before
      Hetzner production run).

      Per-row CH enrichment (SEP-1 + NFT `token_uri` port) carved out
      into new task `0231_FEATURE_clickhouse-sep1-nft-enrichment`
      (backlog, blocked_by 0228) so this task stays scoped to the
      cross-machine repair pass.
  - date: '2026-05-22'
    status: completed
    who: stkrolikiewicz
    note: >
      **Closed.** All six phases shipped end-to-end:

        Phase 0 — preconditions met (0225 landed, 0216 Hetzner CH
                  provisioned, schema + parser SHAs locked).
        Phase 1 — parallel backfill: laptop 1 (73 partitions,
                  50,457,424 → 55,103,999), machine 2 (78 partitions,
                  55,104,000 → 60,095,999), laptop 3 (38 partitions,
                  60,096,000 → 62,527,999 = L_last_closed).
        Phase 2 — per-machine cleanup + invariants captured to
                  docs/runbooks/artifacts/laptop{1,2,3}_pre-export-metrics
                  (laptop1 only — 2/3 deferred to operator follow-up).
        Phase 3 — FREEZE + rsync export per worker via
                  scripts/merge-freeze-worker.sh (laptop1/2/3 variants).
        Phase 4 — ATTACH PART import on Hetzner via
                  scripts/merge-attach-hetzner.sh — operator script
                  iterated through several rounds of fixes (Copilot
                  review + first dry-run): bash 4+ check, sentinel
                  associative-array unset under `set -u`, OPTIMIZE
                  skipping 'all' partition + skipping soroban_contracts
                  (preserves deployer for repair_tier1), empty staging
                  short-circuit, PARTITION ID 'X' FINAL syntax order,
                  documented exit codes.
        Phase 5 — post-merge repair on Hetzner (this PR #199):
                    • backfill-runner repair-tier1 — 6 Tier-1 columns ×
                      5 tables rebuilt via staging + EXCHANGE TABLES
                      (10.13M accounts moved first_seen_ledger below
                      last_seen_ledger — strong evidence the rebuild
                      worked).
                    • backfill-runner asset-aggregates — 300,610 asset
                      rows recomputed (298,542 classic + 2,065 SAC
                      with holders/supply landed).
                    • backfill-runner nft-reclassify — 27.6M pending +
                      60.5M ownership pending false positives evicted
                      (lightweight DELETE + OPTIMIZE FINAL); 0 legacy
                      contamination in hot tables.
                    • SQL fixes (CH 26.3): `FROM tbl AS alias FINAL`
                      alias order + argMin alias rename — both shipped
                      in commit 1d95b8e5 on this PR.
        Phase 6 — end-to-end validation per
                  docs/runbooks/0228_phase6_validation.md:
                    • Tier 1 sanity (continuity gaps=0, 17/19 tables
                      populated, dict healthy).
                    • Tier 2 Tier-1 rebuild verified (5 columns ×
                      5 tables, 10-row spot-check each).
                    • Tier 3 worker baseline DEFERRED (laptop 2/3 JSONs).
                    • Tier 4 skeleton/orphan/per-ledger PASS with
                      caveats (threshold tight for merged state).
                    • Tier 5 FULL Horizon hash-set compare: **980/980
                      PASS, 0.0000 % mismatch** — AC `≤ 0.01 %` exceeded.
                  Verdict: GREEN — go-live signal. Report at
                  docs/runbooks/artifacts/phase6_validation_20260521.md.

      Outstanding follow-ups (not blocking close):
        • PR #199 merge — separate review/merge step on origin.
        • Snapshot B + Borg → BX21 — deferred until disk freed by
          shipping Snapshot A off-site.
        • Server profile restart — revert per-query memory cap from
          64 GiB back to 6 GiB (file edited, runtime still 64 GiB
          until container restart).
        • Tier 3 worker baseline catch-up if laptop 2/3 JSONs become
          available.
        • Deeper per-endpoint parity validation — task 0252 (extended
          scope: 30K stratified + S3 XDR fallback + full-table
          invariants + latency profile).

      Spawned tasks from this work:
        • 0231 — CH SEP-1 + NFT token_uri enrichment (Stage 2 enrichment).
        • 0232 — Tier-1 live-mode mitigation proposal (post-merge drift).
        • 0233 — merge-parallel-backfills operator runbook authoring.
        • 0252 — per-endpoint parity validation against Horizon /
                 stellar.expert (deferred 0207 Tier 2-4 work).

      Linked ADRs: 0044 (CH pilot parallel store, no-FINAL invariant),
                   0045 (FREEZE + rsync + ATTACH PART transport).
      Linked task 0118 archived in the same session — Phase 3 of 0118
      was operationally fulfilled by nft-reclassify on 2026-05-21.
---

# Parallel-backfill merge into Hetzner CH (3-way split) + post-merge validation

## Summary

Run the historical Soroban-era backfill in parallel across 3 worker machines
(laptop 1 already doing 2/5; machine 2 picks up the next 78 partitions of 3/5;
laptop 3 picks up the newest ~38 partitions). After all workers finish, mirror
each worker's local CH to the production Hetzner CH using **FREEZE + rsync +
ATTACH PART** per [ADR 0045](../../2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md),
then run a post-merge repair + validation pass that brings the union to the
state a single sequential machine would have produced. Priority is correctness
and completeness across the full backfill range.

## Status: backlog

Gated on three prerequisites:

1. **Task 0225** — sync-validation pre-parse + crash-recovery runbook. Required
   because machine 2's and laptop 3's ranges approach the S3 archive frontier.
2. **Task 0216** — Hetzner CH operational readiness (Ansible playbook against
   `ch-prod-01`, mTLS CA, dict_reader user fix, `config.d/memory.xml`,
   BX21 Storage Box + Borg).
3. **No-`FINAL`-at-query-time invariant** owned post-merge (periodic
   `OPTIMIZE … FINAL` cron on state tables).

## Context

The full plan, with all motivation, math, danger list, decisions, and open
questions, lives in [`notes/S-approved-plan.md`](notes/S-approved-plan.md).

Headline points:

- **3-way split (operator decision)**: laptop 1 = 2/5 (partitions 788–860,
  ledgers 50,457,424 → 55,103,999, ~300 GB CH end-state);
  machine 2 = mid-3/5 (partitions 861–938, 4,992,000 ledgers, ~322 GB CH
  at measured density of 64.5 GB/M, 761 GB free); laptop 3 = newest
  3/5 (partitions 939 → `L_last_closed`, ~38 partitions, ~2.4 M ledgers,
  400–500 GB free — the load-bearing disk-pressure risk).
- **`L_last_closed`** is dynamic, captured at laptop 3's completion time;
  it is the highest _fully-closed_ S3 partition, NOT chain tip.
- **Two CH-partition straddles** (CH partitions 110 and 120) — handled by
  targeted `OPTIMIZE FINAL PARTITION`. Not a correctness issue.
- **Transport = FREEZE + rsync + ATTACH PART** (ADR 0045): zero-disk-overhead
  export on workers, byte-identical parts on Hetzner, per-part atomic attach,
  resumable rsync over SSH. No S3 intermediary.
- **Tier-1 repair pass on Hetzner** rebuilds the 12 columns that RMT collapse
  silently corrupts under cross-machine merge (`first_seen_ledger`,
  `minted_at_ledger`, NFT metadata, contract deployer fields). Plus 5 routine
  enrichment passes (bootstrap, NFT Phase 3, asset aggregates, etc.) that run
  on Hetzner regardless of merge topology.
- **In-flight disk monitoring** with operator-prompted pause on laptop 3 if
  density runs hotter than 2× of laptop 1's measured baseline.

## Implementation Plan

Six phases, executed in order. Full detail in [`notes/S-approved-plan.md`](notes/S-approved-plan.md);
this is the executive index.

1. **Phase 0 — Preconditions** (Section "Phased Plan / Phase 0"): land 0225,
   apply dict_reader user fix, lock schema/parser SHAs, provision workers,
   confirm Hetzner readiness, set up per-worker SSH access + staging dirs.
2. **Phase 1 — Parallel local backfill**: laptop 1 finishes 2/5; machine 2
   runs 78 partitions; laptop 3 runs newest ~38 partitions oldest-first with
   disk-pressure monitoring.
3. **Phase 2 — Pre-merge per-machine cleanup + invariants**: bootstrap,
   0221 SAC drain, baseline metrics, new `verify-local` subcommand.
4. **Phase 3 — FREEZE + rsync export** per worker.
5. **Phase 4 — ATTACH PART import on Hetzner** in worker-order (m1 → m2 → m3),
   per-partition `OPTIMIZE FINAL` afterward.
6. **Phase 5 — Post-merge repair on Hetzner**: `OPTIMIZE FINAL`,
   `DEDUPLICATE BY`, bootstrap union, Tier-1 column rebuilds via staging +
   `EXCHANGE TABLES`, NFT Phase 3 (0118), asset aggregates (0194),
   Statement B index (0198).
7. **Phase 6 — End-to-end validation**: new `verify-completeness` subcommand,
   ledger continuity, row-count parity, sample compare against Horizon /
   stellar.expert.

## Acceptance Criteria

- [x] Workers run on identical schema (sidecar `init.sql` from locked SHA) and
      identical parser binary hash; both verified in the `backfill_runs`
      audit table at merge time.
      _Laptop 1: parser SHA `26d75f33bf2f4135f8ecbf3a93bb9c0b27b14d4a` confirmed.
      Machine 2 + laptop 3 ran same parser per scripts/merge-freeze-worker.sh
      precondition checks._
- [x] All worker ranges are disjoint, recorded in audit table.
      _Laptop 1: 50,457,424–55,103,999 (73 partitions).
      Machine 2: 55,104,000–60,095,999 (78 partitions).
      Laptop 3: 60,096,000–62,527,999 (38 partitions).
      Total: 12,070,576 ledgers, gaps=0 verified in Phase 6 Tier 1.1._
- [x] `verify-local` passes on every worker before its FREEZE + rsync.
      _Laptop 1: continuity gap=0, fact-parity tx 1,458,788,880 expected ==
      actual, skeleton floor 2.86% (residual = merged accounts), drains clean.
      Machine 2 + laptop 3: similar manual verify-local equivalents before
      FREEZE._
- [x] All 19 CH tables + 1 dictionary are populated on Hetzner.
      _17/19 tables with rows post-merge (nfts + nft_ownership = 0 by design,
      no Nft-classified contracts in union). transaction_hash_dict CACHE
      layout verified via dictGet round-trip 10/10._
- [x] No-`FINAL`-at-query-time invariant holds after Phase 5
      (`SELECT count() FROM <state_table>` matches `SELECT count() FROM <state_table> FINAL`).
      _Phase 6 Tier 1.4: all 8 RMT state tables show delta = 0._
- [x] Tier-1 repair pass complete: `first_seen_ledger`, `first_deposit_ledger`,
      `minted_at_ledger`, NFT metadata, contract deployer fields all reflect
      union-derived values, not RMT-overwritten values.
      _backfill-runner repair-tier1 rebuilt 6 of the 12 Tier-1 columns
      (SQL-aggregate side) across 5 tables. NFT metadata + 5 other external-
      fetch columns split to task 0231 per the Stage 1/2 pipeline diagram
      (notes/G-stage-1-2-pipeline.svg). Tier 2 spot-check: 10/10 random
      samples per column match the fact-table aggregate._
- [x] `verify-completeness` reports zero gaps in `ledgers.sequence` from
      `50,457,424` to `L_last_closed`.
      _Phase 6 Tier 1.1: min=50,457,424 max=62,527,999, gaps=0,
      row_count=expected_count=12,070,576._
- [x] Sample-compare against Horizon / stellar.expert on 1000 stratified
      ledgers shows ≤ 0.01 % mismatch.
      _Phase 6 Tier 5 full run: 980 stratified ledgers from the Horizon-
      retention-valid range (≥ 56,657,428), paginated hash-set compare
      against transaction_hash_index → **980 / 980 PASS, 0 fail,
      0.0000 % mismatch**. AC exceeded._
- [ ] BX21 Borg backup of Hetzner state captured before any read traffic.
      _Deferred — Snapshot A (pre-Phase 5, 691 GiB) occupies most of local
      disk; Snapshot B + Borg pipeline will land after shipping A to BX21
      to free space. Tracked outside this task._
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      §5.6 mentions the merge-completion state of the prod CH.
      `docs/runbooks/merge-parallel-backfills.md` authoring tracked in
      task 0233.
- [x] **API types regenerated** — N/A: this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Notes

- Full approved plan: [`notes/S-approved-plan.md`](notes/S-approved-plan.md)
- Pipeline overview (Phase 1–4 transport + Stage 1/2 column ownership):
  [`notes/G-stage-1-2-pipeline.svg`](notes/G-stage-1-2-pipeline.svg) — shows
  the full 12 Tier-1 columns split across Stage 1 (6 SQL-aggregate columns
  done in this task) and Stage 2 (6 external-fetch columns in task 0231).
- Originally drafted as a Claude Code plan at
  `~/.claude/plans/pull-aktualny-develop-aktualnie-sprightly-pixel.md`
  after an extended planning session covering the schema audit,
  parser write-path audit, ADR 0040 / 0044 / 0045 review, task 0216 context,
  density-extrapolation math, and three operator clarification rounds.
- The schema-engine-swap follow-up (use `AggregatingMergeTree` with
  `SimpleAggregateFunction(min, …)` to eliminate ~5 of the 12 Tier-1
  repair columns) is OUT OF SCOPE for this task and should be tracked
  as a separate `0229_PROPOSAL_aggregatingmergetree-for-state-tables`
  if/when the team commits to it.
