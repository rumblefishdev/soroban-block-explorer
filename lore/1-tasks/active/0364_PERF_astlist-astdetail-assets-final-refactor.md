---
id: '0364'
title: 'PERF: astlist + astdetail — replace whole-`assets FINAL` scan with a bounded seek/dedup (shared assets-select refactor)'
type: PERF
status: active
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
  - date: 2026-07-08
    status: active
    who: karolkow
    note: >
      Promoted to active. Pre-impl code/schema review corrected the plan:
      `assets` is plain ReplacingMergeTree with NO version column, so Step 1's
      argMax/ORDER-BY-version dedup does not apply — all projected columns are
      the identity 4-tuple or the deterministic `id`, hence byte-identical
      across physical versions → drop FINAL + read-in-order + Rust dedup is
      deterministic with no tiebreak. Step 3 widened: 3 more whole-table reads
      on the hydration path (soroban_contract_metadata FINAL, asset_enrichment
      + asset_sac GROUP BY), not just issuer/contract.
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
whole-table `FINAL` collapse with a bounded seek + key-dedup, shared across the
list and detail paths.

> **Plan correction (2026-07-08 pre-impl review).** `assets` is **plain
> `ReplacingMergeTree` — NO version column** ([`init.sql:253`]), so the original
> Step 1 ("`argMax` on the version column / `ORDER BY version DESC`") has no
> column to sort on. But dedup is trivially deterministic anyway: **every column
> this query projects from `a` is either the identity 4-tuple (immutable per key)
> or `a.id` (deterministic `cityhash64` of that key).** The mutable columns
> (`total_supply`, `holder_count`, `icon_url`) are DEAD — externalized to
> `balance_aggregates` / `asset_enrichment`. So all physical versions of a key
> are **byte-identical in the projected columns** → drop `FINAL`, read in PK
> order, dedup by the 4-tuple; no version tiebreak needed, byte-identical output
> guaranteed. Step 3 also widened — 3 more whole-table reads on the hydration
> path, not just issuer/contract.

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
asset. Drop `FINAL`-over-table; filter on the `assets` key columns so read_rows ≈
the matching versions, not the whole table. Since all projected `a` columns are
byte-identical across a key's versions (see correction above), a plain
`LIMIT 1`/take-first over the keyed match is deterministic — no version pick
needed. Worst offender is `fetch_by_contract_id`: it filters on the JOINED
`sc.contract_id` / `sac.sac_contract_id`, **zero predicate on `assets` columns**,
so today it forces a full `assets FINAL` scan; the fix must give it an `assets`-
side seek or resolve the surrogate id first, then key-seek `assets`.

### Step 2: astlist (paginated) — read-in-order + Rust dedup (approach-B)

Mirror the asttxs/acclist finding: drop `FINAL`, over-fetch in PK order (the
list `ORDER BY` IS the `assets` PK — the identity 4-tuple), Rust
consecutive-dedup by that 4-tuple, NOT `LIMIT 1 BY` (which defeats
`optimize_read_in_order`). Versions are byte-identical in the projection, so the
dedup keeps the first and is deterministic. No projection needed here (unlike 0353) because the sort column already IS the PK.

### Step 3: hydration joins — bound EVERY side table, not just issuer/contract

The shared select materializes **four** whole side tables, all
`ReplacingMergeTree(version)` / GROUP-BY subqueries — each a whole-dimension read
that survives the `assets FINAL` fix and then dominates the point-lookup detail:

- `soroban_contract_metadata FINAL` (~L234) — whole-table `FINAL`.
- `asset_enrichment` GROUP BY (~L237–243) — whole-table aggregate.
- `asset_sac` GROUP BY (~L245–252) — whole-table aggregate (small, but scanned).
- issuer `accounts` seek (~L446–473) — **already** a bounded id-IN key-seek
  (task 0319); `soroban_contracts` join (~L231) is tiny (159k).

Bound the three subqueries to the page/lookup ids (id-IN), same as the issuer
seek — do not leave them whole-table once `assets` is fixed.

## Acceptance Criteria

- [ ] astlist + astdetail read_rows bounded to the working set (not the ~2M
      whole-`assets FINAL` scan); verified via `system.query_log`.
- [ ] Outputs byte-identical to pre-change (prod before/after or a local range
      containing the data) — list + ALL detail variants (by contract, by
      code+issuer, native).
- [ ] No whole-dimension read remains on the hydration path — issuer + contract
      AND the three side-table subqueries (`soroban_contract_metadata FINAL`,
      `asset_enrichment`, `asset_sac`) bounded to the page/lookup ids.
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
