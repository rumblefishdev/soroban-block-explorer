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

### Decide before implementing

1. **Accept the scan** — add a `read_rows` measurement to the acceptance
   criteria and revisit when the asset table grows. Cheapest now, worst later.
2. **ClickHouse projection ordered by `sac_contract_id`** — the engine
   maintains it, the query needs no change; cost is disk plus slower writes.
   Cleanest, and the only option that keeps the list page bounded.
3. **Scope down to the contract DETAIL page only** (one row, no list join) —
   closes the reported ask with a single-row read and leaves the list without
   the mirrored asset. Partial, but honest and cheap.

Recorded because the original "why this is cheap" reasoning was right about
the join existing and wrong about it being reusable: it exists in the
direction that has a sort key.

## Scope

1. Contract detail + contract list queries: left-join the mirrored asset when
   `is_sac`.
2. DTO: replace the bare boolean with the boolean plus optional
   `{ asset_code, issuer, asset_id }`; keep `is_sac` for callers that only
   need the flag.
3. Frontend: badge becomes `SAC · USDC` linking to the asset detail page; the
   detail page gains a "Mirrors asset" row.

## Acceptance criteria

- [ ] Reverse-lookup access path decided (scan / projection / detail-only) and
      recorded — not left to whoever writes the query
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
