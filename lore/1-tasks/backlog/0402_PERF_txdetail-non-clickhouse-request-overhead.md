---
id: '0402'
title: 'PERF: txdetail — the ≥427 ms that is provably outside ClickHouse (connection / batching, not SQL)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0357']
tags: [priority-high, effort-medium, layer-api, phase-post-launch]
links:
  - crates/api/src/queries.rs
history:
  - date: 2026-07-17
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0357 future work, where it was explicitly left un-owned.
      0357 proved by `system.query_log` join that txdetail's p95 is NOT a query
      problem, so it does not belong to a read-path/SQL task — it needs its own
      connection/transport investigation.
---

# PERF: txdetail — non-ClickHouse request overhead

## Summary

`txdetail` is the single largest contributor to the p95 tail (1160 ms, 34% at
10M/mo), and **at least 427 ms of it happens outside ClickHouse**: the endpoint
issues 6 CH queries per request, each measuring `max <= 12 ms` server-side, so
the total CH work is `<= 72 ms`. No SQL change can fix this.

## Context

Spawned from [0357](../archive/0357_PERF_launch-readpath-perf-cluster.md), which
measured it and deliberately left it un-owned: it is a connection / transport
story (mTLS handshake per query, Lambda), not a read-path one. The same task also
established a **~60-90 ms floor** on every request before any query runs
(`netstats` does `<= 32 ms` of CH work and takes 90 ms) — a third of the 200 ms
AC4 budget. txdetail is the worst instance of that class, not a separate bug.

## Implementation

- [ ] Attribute the ≥427 ms: mTLS handshake per query vs connection-pool misses
      vs Lambda cold/warm vs serialization. Measure, do not infer — 0357's method
      (client duration joined to `system.query_log`) is the precedent.
- [ ] Establish whether the 6 queries/request can share one connection, or be
      batched/pipelined into fewer round-trips.
- [ ] Quantify the shared floor separately from txdetail's own excess, so the
      fix is credited to whichever it actually is (cf. 0357's #347-vs-index
      attribution split).
- [ ] Re-measure with the 0357 harness (`--rps` open model), same tiers.

## Acceptance Criteria

- [ ] The ≥427 ms is attributed to named causes with measurements, not a guess.
- [ ] Either the per-request overhead is reduced (with a before/after on the open
      model), or it is documented as a platform floor with the cause named and
      the AC4 report updated to say so.
- [ ] Findings state whether the fix generalises to the ~60-90 ms floor on every
      endpoint, or is txdetail-specific.
- [ ] Docs updated — mark each `docs/architecture/**` file updated or
      `N/A — reason`.
- [ ] API types regenerated if `crates/api/**` or `Cargo.*` changed.
