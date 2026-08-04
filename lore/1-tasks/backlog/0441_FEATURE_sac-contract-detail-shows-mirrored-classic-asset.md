---
id: '0441'
title: 'FEATURE: SAC contract detail shows the classic asset it mirrors (reverse of the join we already run)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0339']
tags:
  [
    backend,
    api,
    frontend,
    contracts,
    sac,
    assets,
    priority-medium,
    effort-small,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/368'
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment: "if SAC, show
      classic asset name (since it is available?) — reverse lookup". Correct —
      the mapping exists and is already used in the other direction on the
      liquidity-pool endpoints. Not covered by 0339, which reshaped the data
      model rather than the contract-detail presentation.
  - date: '2026-07-31'
    status: backlog
    who: karolkow
    note: >
      Sizing corrected by measurement, not re-estimated from the task text.
      The reusable join runs asset → SAC and prunes on the sort key; this
      task runs SAC → asset, which has neither key nor skip index, so the
      duplicate collapse scans all 436,753 rows per request (1.1M for a
      50-row list page). Still `effort-small` in code, but it now carries an
      access-path decision — see "Decide before implementing". Dropped from
      first place in the quick-win ranking because of it.
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Access path decided: **accept the scan**. The row count was the wrong
      unit — `system.parts` puts the whole table at **7.79 MiB** over 7 parts,
      the query runs in ~0.10 s, and `EXPLAIN ESTIMATE` is identical for 1 and
      for 50 ids, so a list page costs one scan rather than one per row. The
      1,105,551-row figure was a different query shape and is corrected. SACs
      are 2.89% of contracts and the default list page holds none, so the
      query usually does not fire at all. A projection would mean an ALTER on
      a live table plus write amplification for an 8 MB read; detail-only
      scoping would give up the list for zero saving. The upgrade path is
      named instead — a `bloom_filter` skip index (not `minmax`, which cannot
      prune a cityhash64 surrogate), triggered past ~5M rows. Implementation
      in `stash@{2}` already uses the batched shape.
---

# FEATURE: surface the classic asset behind a SAC contract

## Summary

A Stellar Asset Contract is the contract-side facet of a classic asset, but the
contract pages expose only a boolean `is_sac`. Show which asset it mirrors —
code plus issuer, linked to the asset detail page — instead of an unqualified
`SAC` badge.

## Current behaviour

- `crates/api/src/contracts/dto.rs:34` and `:65` expose `is_sac: bool` and
  nothing else about the mirrored asset;
  `crates/api/src/contracts/queries.rs:224` / `:389` select `sc.is_sac`.
- `web/src/pages/contracts/ContractsTable.tsx:39` renders a bare `SAC` chip.

## Why this looked cheap — and what the measurement says

The mapping is already in the database and already queried — in the opposite
direction. `crates/api/src/liquidity_pools/queries.rs:288-341` resolves
`(asset_code, issuer_id)` → `asset_sac.sac_contract_id` → `soroban_contracts`
to attach a SAC contract to a classic pool leg. This task needs the same join
read the other way: `soroban_contracts.id` → `asset_sac` → `(asset_type,
asset_code, issuer_id)`.

Note `asset_sac` requires a `GROUP BY` collapse before use (see the existing
subquery at `:293-295`) — it is not one row per contract by construction:
436,780 rows over 297,511 distinct `sac_contract_id`, up to 7 rows for one
contract (measured 2026-07-31).

**The direction is the whole cost.** `asset_sac` is an `AggregatingMergeTree`
`ORDER BY (asset_type, asset_code, issuer_id, contract_id)` — sorted by the
ASSET side. The existing LP query prunes on that key; this task's reverse
lookup has no key and no skip index over `sac_contract_id`
(`system.data_skipping_indices` is empty for the table), so it scans.

Measured on production (`rows_read`, 2026-07-31):

| Query                                                      | rows_read                |
| ---------------------------------------------------------- | ------------------------ |
| single contract, match found early in scan order           | ~200–350                 |
| **single contract, no match**                              | **436,753 (full table)** |
| single contract, aggregated (the collapse this task needs) | **436,753**              |
| contract list page of 50                                   | **1,105,551**            |

The small numbers are a `LIMIT 1` artefact — ClickHouse stops at the first
match, so the cost depends on where the asset happens to sort. A SAC contract
with no `asset_sac` row is an ordinary case, and the mandatory duplicate
collapse cannot short-circuit at all; both read the whole table every request.

At today's size that is ~3.5 MB / 4 ms, not an outage. But it grows linearly
with the asset count (376k assets and climbing), and against the read-row
quota a contract-list page costs ~1.1M rows — roughly 1,800 requests to
exhaust an hour's budget on this one join.

### Access path — DECIDED 2026-07-30: accept the scan

Three options were open: accept the scan, add a projection ordered by
`sac_contract_id`, or scope down to the detail page. **Accept the scan.**

Four measurements settled it.

**The scan is 7.79 MiB, not 436k rows of pain.** `system.parts` on prod:
443,606 rows across 7 active parts, **7.79 MiB on disk**, 66 marks. The row
count is the alarming-sounding number; the byte count is the one that matters,
and we read 4 of 6 narrow columns from it. End-to-end **~0.10 s**, measured
three times through the same transport as any other query.

**One batched query, not one per row.** `EXPLAIN ESTIMATE` for a 50-id
`IN (…)` list and for a single id return the identical plan — 443,614 rows,
59 marks, 7 parts. A whole list page costs one scan, and the earlier
**1,105,551 rows for a 50-row page is not this query's shape** — it came from
a per-id or repeated-aggregate form. Corrected here rather than left standing.

**It usually does not run at all.** SACs are **2.89%** of contracts (3,944 of
136,538), and the query is skipped when a page holds none. The newest 50
contracts — the default list page — contain **zero**.

**A projection is over-engineering at this size.** It means an `ALTER` on a
live table plus permanent write amplification, to optimise an 8 MB read that
fires on a minority of page loads. **Detail-only scoping buys nothing** — the
list page costs the same single scan, so giving up the list feature saves
zero.

#### The upgrade, named so nobody has to re-derive it

If this ever bites, add a **`bloom_filter` skip index on `sac_contract_id`** —
not `minmax`, which prunes nothing on a cityhash64 surrogate because the values
are random with respect to the sort order. The table has ~59 marks and a page
carries one or two SAC ids, so a bloom would prune to a handful of granules.
This is the same pattern as `idx_oaa_transaction_id` (task 0393), which exists
for exactly this shape: filtering a non-sort-key column by a scattered id set.

**Trigger to revisit:** `asset_sac` past ~5M rows, or this query showing up in
the slow log. Not before — at 7.79 MiB the index would cost more to maintain
than the scan costs to run.

Recorded because the original "why this is cheap" reasoning was right about
the join existing and wrong about it being reusable: it exists in the
direction that has a sort key. The direction really is the whole cost — it is
just that the whole cost is 8 MB.

## Scope

1. Contract detail + contract list queries: left-join the mirrored asset when
   `is_sac`.
2. DTO: replace the bare boolean with the boolean plus optional
   `{ asset_code, issuer, asset_id }`; keep `is_sac` for callers that only
   need the flag.
3. Frontend: badge becomes `SAC · USDC` linking to the asset detail page; the
   detail page gains a "Mirrors asset" row.

## Acceptance criteria

- [x] Reverse-lookup access path decided (scan / projection / detail-only) and
      recorded — **accept the scan**, 2026-07-30, see above
- [ ] The query is issued **once per page** with every SAC id in one `IN` list —
      a per-id form multiplies the scan and is the shape the 1.1M figure came
      from
- [ ] `read_rows` measured on the contract LIST page, not just the detail
      page; bounded as the asset table grows
- [ ] Contract detail returns the mirrored classic asset when `is_sac`
- [ ] Reverse join collapses `asset_sac` duplicates (mirror the LP subquery)
- [ ] `is_sac` true with no resolvable asset degrades to the current bare badge
- [ ] Native (XLM) SAC handled — a positive surrogate, not an empty issuer
- [ ] Frontend links the asset; StrKey of the contract stays canonical
- [ ] **Docs updated** — contract endpoint contract under
      `docs/architecture/**` per ADR 0032
- [ ] **API types regenerated** — touches `crates/api/**`; run
      `npx nx run @rumblefish/api-types:generate`

## Notes

Native XLM carries two competing conventions in this codebase (positive
surrogate from `hash64("native")` vs empty string). Use the surrogate form; the
empty-string form falls through filters silently.
