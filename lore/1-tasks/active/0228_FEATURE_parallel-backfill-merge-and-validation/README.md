---
id: '0228'
title: 'FEATURE: parallel-backfill merge into Hetzner CH (3-way split) + post-merge validation'
type: FEATURE
status: active
related_adr: ['0040', '0044', '0045']
related_tasks: ['0118', '0194', '0198', '0216', '0225']
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

- [ ] Workers run on identical schema (sidecar `init.sql` from locked SHA) and
      identical parser binary hash; both verified in the `backfill_runs`
      audit table at merge time.
      _Laptop 1: parser SHA `26d75f33bf2f4135f8ecbf3a93bb9c0b27b14d4a` confirmed._
- [ ] All worker ranges are disjoint, recorded in audit table.
      _Laptop 1 range locked: 50,457,424–55,103,999 (73 partitions, 4,646,576 ledgers)._
- [ ] `verify-local` passes on every worker before its FREEZE + rsync.
      _Laptop 1: manual verify-local equivalent done — continuity gap=0,
      fact-parity tx 1,458,788,880 expected == actual, skeleton floor 2.86%
      (residual = merged accounts), `nfts_pending` + `nft_ownership_pending`
      drained (leaked=0 in both)._
- [ ] All 19 CH tables + 1 dictionary are populated on Hetzner.
- [ ] No-`FINAL`-at-query-time invariant holds after Phase 5
      (`SELECT count() FROM <state_table>` matches `SELECT count() FROM <state_table> FINAL`).
- [ ] Tier-1 repair pass complete: `first_seen_ledger`, `first_deposit_ledger`,
      `minted_at_ledger`, NFT metadata, contract deployer fields all reflect
      union-derived values, not RMT-overwritten values.
- [ ] `verify-completeness` reports zero gaps in `ledgers.sequence` from
      `50,457,424` to `L_last_closed`.
- [ ] Sample-compare against Horizon / stellar.expert on 1000 stratified
      ledgers shows ≤ 0.01 % mismatch.
- [ ] BX21 Borg backup of Hetzner state captured before any read traffic.
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      §5.6 mentions the merge-completion state of the prod CH.
      `docs/runbooks/merge-parallel-backfills.md` authored, extending
      the 2/5 fresh-machine runbook + citing ADR 0045 + task 0216.
- [ ] **API types regenerated** — N/A: this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Notes

- Full approved plan: [`notes/S-approved-plan.md`](notes/S-approved-plan.md)
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
