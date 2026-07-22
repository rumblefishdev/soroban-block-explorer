---
id: '0401'
title: 'PERF: lplist — stored `created_at_ledger` (re-litigate 0208 Path 1 with the 0357 numbers)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0357', '0208', '0356']
tags: [priority-high, effort-medium, layer-clickhouse, phase-post-launch]
links:
  - crates/api/src/queries.rs
history:
  - date: 2026-07-17
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0357 future work. lplist is the last genuine query offender on
      the read path — every other 07-06 worklist endpoint now lands under 300 ms.
      0208 Path 1 was rejected on writer/RMT grounds before the 0357 open-model
      numbers existed; this task re-litigates that decision with them.
---

# PERF: lplist — stored `created_at_ledger`

## Summary

`lplist` derives `created_at` as `min(ledger_sequence)` over the pool's whole
history, reading ~9.6-11.3M rows/request (CH ~425 ms, p95 618 ms at 10M/mo).
It is the **only** remaining read-path endpoint whose p95 is dominated by
ClickHouse work it does not need to do.

## Context

Spawned from [0357](../archive/0357_PERF_launch-readpath-perf-cluster.md), which
measured it against an open model and confirmed it unmoved by #347/#349 (both
removed saturation, not per-request cost). The fix — storing `created_at_ledger`
at write time instead of deriving it — is 0208 Path 1, rejected at the time on
writer/RMT grounds. 0357 produced the numbers that argument lacked: lplist is
32% of the p95 tail at the AC4 target rate.

## Implementation

- [ ] Re-read the 0208 Path 1 rejection and state which of its objections still
      hold given a RMT write path (the writer/RMT concern is the crux).
- [ ] If Path 1 survives: add `created_at_ledger` to the pool write side,
      backfill it (derivable from existing history — no S3 re-parse), swap the
      `min(ledger_sequence)` read.
- [ ] If Path 1 still fails: record why, and either propose an alternative
      (bounded seek / companion table, cf. 0365's `operation_pools` pattern) or
      promote lplist to a documented known-issue with the cause named.
- [ ] Re-measure with the 0357 harness (`--rps` open model) — same tiers, so the
      before/after is comparable to the series 1-3 record.
- [ ] **Check whether `min(ledger_sequence)` is the whole story.** Measured
      2026-07-22 (task 0397): over 7 days lplist's own queries read **45.8 bn
      rows under `FINAL`** — 27.3 bn on `lp_positions` (2 168 calls × 12.6M) and
      18.5 bn on `liquidity_pools` (2 025 × 9.1M). These are the #2 and #3
      `FINAL` consumers in the cluster after 0397's. Neither table carries a skip
      index (prod has them only on `accounts`, `soroban_contracts`,
      `transactions`, `ledgers`, `operations_appearances`), so any lookup off
      their sort key has nothing to seek with. If storing `created_at_ledger`
      does not also remove those `FINAL`s, the remainder is a separate piece of
      work and should be split out rather than silently left.

## Acceptance Criteria

- [ ] The 0208 Path 1 decision is re-litigated in writing, either reversed or
      re-affirmed with the 0357 numbers addressed — not left implicit.
- [ ] If implemented: lplist reads are bounded (no whole-history `min()` scan),
      output byte-identical vs prod-before, and p95 re-measured on the open model.
- [ ] Any schema / write-side change is added to `init.sql`, not applied
      prod-only (the recurrence class 0357 found — see 0400).
- [ ] Docs updated — a write-side column change fires the ADR 0032 gate; mark
      each `docs/architecture/**` file updated or `N/A — reason`.
- [ ] API types regenerated if `crates/api/**` or `Cargo.*` changed.
