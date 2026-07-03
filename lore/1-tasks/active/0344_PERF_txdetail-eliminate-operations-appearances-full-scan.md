---
id: '0344'
title: 'PERF: txdetail — eliminate ~100M-row full-scan of operations_appearances (filter by ledger/tx_hash PK)'
type: PERF
status: active
related_adr: []
related_tasks: ['0338', '0329']
tags:
  [priority-high, effort-medium, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Spawned from 0338 load-test analysis — top bottleneck (39% of all CH rows, all timeouts).'
  - date: 2026-07-03
    status: active
    who: fmazur
    note: 'Activated — starting the txdetail full-scan fix.'
---

# PERF: txdetail — eliminate ~100M-row full-scan of operations_appearances

## Summary

`txdetail` is the #1 bottleneck found in the 0338 load test. It reads **~102M
rows / ~7.6 GB per request** and spends **~25 s of ClickHouse time**, so **7 of
10 requests timed out (504)** at the API Gateway 29 s integration cap. That is
**39% of all CH read_rows from just 7% of requests**. Reading 100M rows to render
one transaction's detail = a full scan of `operations_appearances` with no
primary-key / partition pruning.

## Context

Evidence: `crates/load-tests/out/2026-07-01T13-43-39Z/results.csv` (10-VU smoke).
`txdetail`: dur p50/p95/max = 29050/29302/29302 ms, read_rows max 102.3M,
read_bytes max 7.67 GB, ch_duration max 27.8 s. Likely related to the folded-
operations behaviour in **0329**.

## Implementation

- Locate the `txdetail` handler's CH query (`crates/api/**`) and the
  `operations_appearances` table PK / partition key.
- Ensure the query filters by `tx_hash` (and/or `ledger`) so CH prunes by
  primary key / partition instead of scanning the whole table. Target: from
  ~100M rows → thousands.
- Verify with `EXPLAIN` / `system.query_log` read_rows on a representative tx.

## Acceptance Criteria

- [ ] `txdetail` read_rows/request drops from ~100M to <~10k (measured in query_log)
- [ ] No 504s for `txdetail` at load; p95 well under the 29 s gateway cap
- [ ] Fix coordinated with 0329 (folded operations) if they touch the same query
