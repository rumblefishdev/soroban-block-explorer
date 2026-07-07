---
id: '0364'
title: 'PERF: astlist + astdetail — replace whole-`assets FINAL` scan with a bounded seek/dedup (shared assets-select refactor)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0357', '0354', '0334']
tags:
  [priority-medium, effort-medium, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links:
  - crates/api/src/assets/queries.rs
history:
  - date: 2026-07-07
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from the 0357 read-path perf cluster (group C). The 2026-07-07
      load test confirmed astlist/astdetail still read ~2M rows via `assets
      FINAL` (10-VU p95: astlist 4.5 s, astdetail 1.1 s; CH `assets` p95 2.85 s;
      100-VU: astlist 2% 504-timeout, astdetail 8.5 s p95). Own task per 0357 —
      the shared assets-select touches both the list and detail paths.
---

# PERF: astlist + astdetail — bounded assets read (shared-select refactor)

## Summary

`astlist` (`assets::fetch_list`) and the asset-detail fetchers share an
`assets a FINAL` + lookup-join pattern that reads **~2M rows per request** even
though the `assets` table is only **~359k rows** — `FINAL` collapses every
re-ingested version across the whole table instead of seeking the working set.
This is the last unfixed whole-dimension read in the 0357 cluster that is a
query/refactor: not blocked (unlike the 0356 LP snapshots) and not
CH-engine-rejected (unlike the acclist projection, 0353). Replace the
whole-table `FINAL` collapse with a bounded seek + deterministic version-dedup,
shared across the list and detail paths.

## Context

From the 0357 load test (2026-07-07) + `system.query_log`:

- `assets` table: **359k rows**, but astlist/astdetail read **~1.68M avg / 2.0M
  max** per request, CH p95 **2851 ms** (97 queries in the 10-VU window).
- Client p95 (10-VU idle): **astlist 4507 ms**, **astdetail 1143 ms**.
- Under 100 VU: astlist 1.95% 504-timeout, astdetail p95 8551 ms.
- Root cause: `assets a FINAL` (ReplacingMergeTree keyed on the asset 4-tuple)
  applies `FINAL` over the **whole table** before WHERE/ORDER prunes, so a list
  page or a single-asset lookup both scan every version. Same class the cluster
  removed elsewhere (0344 / 0345 / 0354 accounts joins, acclist scan).

Code: the shared select in `crates/api/src/assets/queries.rs` — `assets a
FINAL` (~L230) + `soroban_contract_metadata FINAL` (~L234), consumed by
`fetch_list` (~L383, astlist) and the detail variants `fetch_by_contract_id`
(~L501), `fetch_by_code_issuer` (~L526), `fetch_native` (~L558).

## Implementation Plan

### Step 1: detail (point-lookup) — seek, don't scan

`fetch_by_contract_id` / `fetch_by_code_issuer` / `fetch_native` resolve ONE
asset. Replace `FINAL`-over-table with a keyed seek + deterministic latest-
version pick (`argMax` on the version column, or `ORDER BY version DESC LIMIT 1`
/ Rust dedup) so read_rows ≈ the matching versions, not the whole table.

### Step 2: astlist (paginated) — read-in-order + Rust dedup (approach-B)

Mirror the asttxs/acclist finding: a raw over-fetch on the list's sort column +
Rust consecutive-dedup by asset id, NOT `LIMIT 1 BY` (which defeats
`optimize_read_in_order`). If the sort column is non-PK and can't seek, note the
residual — a projection is CH-26.3-rejected on RMT (see 0353).

### Step 3: hydration joins — id-IN resolvers, not whole-dimension

Confirm the issuer (`accounts`, 22M) and contract (`soroban_contracts`, 159k)
lookups are id-IN resolvers / bounded — `soroban_contracts` is tiny, but the
accounts issuer lookup must not whole-scan. `soroban_contract_metadata FINAL`
bounded to the page ids.

## Acceptance Criteria

- [ ] astlist + astdetail read_rows bounded to the working set (not the ~2M
      whole-`assets FINAL` scan); verified via `system.query_log`.
- [ ] Outputs byte-identical to pre-change (prod before/after or a local range
      containing the data) — list + ALL detail variants (by contract, by
      code+issuer, native).
- [ ] No whole-dimension JOIN remains on the hydration path (issuer / contract).
- [ ] p95: aim `< 200 ms`; a flat 200 ms is **not guaranteed** for the
      analytical list on single-node CH — document the achieved number either way.
- [ ] **Docs updated** — N/A unless a projection/index is added → then update the
      schema pages under `docs/architecture/**` per
      [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — N/A (query-internal; no API surface change)
      unless a DTO changes.

## Notes

- Sibling of the 0357 cluster: nftdetail (#314) + asttxs driver (#315) done;
  acclist (0353) is a CH-rejected-projection known-issue; the LP snapshot
  endpoints (lplist / lpdetail / lpchart) are blocked on 0356 / #318; the
  tx-list family (asttxs / acctxs / lptxs) shares a separate
  `operations_appearances` entity-keyed MV.
- `< 200 ms` not guaranteed — this is a list / analytical endpoint; the win is
  removing the whole-table scan, which is what makes it load-resistant.
