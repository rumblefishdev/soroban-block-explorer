---
id: '0387'
title: 'PERF: tx-list accounts surrogate→StrKey seek reads ~785k/page (22M churny RMT) — residual read-path monster after 0386'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0357', '0386']
tags: [perf, clickhouse, read-path, priority-high, effort-medium, milestone-3]
milestone: 3
links: []
history:
  - date: 2026-07-14
    status: backlog
    who: karolkow
    note: >
      Spawned from 0386. Prod chq measurement showed the accounts id-IN seek —
      NOT the (now-deleted) contract FINAL — is the dominant per-page cost on
      /transactions. Belongs to the 0357 read-path cluster.
---

# PERF: accounts surrogate→StrKey seek dominates the tx-list read cost

## Summary

After 0386 removed the dead `contract_ids` (whole-table `soroban_contracts
FINAL`) from `fetch_tx_list_aggregates`, the aggregate hydration dropped
**210k → 8k** read_rows/page. But `/transactions` still reads **~1.8M/page**.
Prod `chq` pins the dominant residual on the **accounts surrogate→StrKey
resolution** — `resolve_source_and_closed_at` / `resolve_accounts`
(`WHERE id IN (...) LIMIT 1 BY id` on the 22M-row `accounts` table): **~785k
read_rows for ~11 source-account ids on a single page.**

## Context

Spawned from [0386]; sits under the [0357] read-path cluster (0357's `txlist`
row = ~2M / ~900 ms, "untasked"). The id-IN bloom seek IS the codebase-standard
surrogate→StrKey pattern (0290/0344/0345) — correct, but expensive on `accounts`
because it is a **churny ReplacingMergeTree**: 9 active parts, ~35M physical rows
(21.7M distinct + un-merged dups). Each seek touches the leading granule of every
part → ~785k rows even for a handful of ids. Same class as 0357's search-prefix
finding (accounts many-parts merge); the lever noted there is **merge cadence /
`accounts_recent`**, NOT a query/schema change.

Affects every endpoint resolving account StrKeys per page: `/transactions`,
ledger-detail embedded txs, acctxs / asttxs / lptxs (source account), plus
tx-detail participants/ops.

## Implementation (candidates — measure first)

- **Merge cadence / OPTIMIZE `accounts`** — fewer active parts → fewer leading
  granules per seek. Cheapest lever, no schema/query change (0357's own
  recommendation for the search-prefix case). Measure part-count vs seek cost.
- **`accounts_recent`-style companion / id-keyed dictionary** — 0385/PR #328
  built a `last_seen`-ordered refreshable MV for acclist; assess whether the
  tx-list source-account resolve can ride an id-keyed companion or a CH
  Dictionary for O(1) resolution (dictGet reads zero table rows).
- **Reconfirm** the `idx_acc_id` bloom FP floor at 22M scale (0290 tuned it to
  0.001) — verify FP is not inflating the seek.

## Acceptance Criteria

- [ ] Root-cause the ~785k/page: parts-count vs bloom-FP vs granule spread,
      quantified via `system.query_log` / EXPLAIN on prod.
- [ ] Pick a lever (merge cadence / companion / dict) with a measured
      before/after on `/transactions`.
- [ ] Per-page read_rows on the tx-list family bounded toward the working set;
      verified via `system.query_log`.
- [ ] Output byte-identical (StrKeys unchanged).
- [ ] Docs / API types — per chosen lever (N/A if query-only merge cadence).
