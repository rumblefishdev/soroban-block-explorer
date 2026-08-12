---
id: '0441'
title: 'FEATURE: SAC contract detail shows the classic asset it mirrors (reverse of the join we already run)'
type: FEATURE
status: completed
related_adr: ['0051']
related_tasks: ['0339', '0472']
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
  - date: '2026-08-10'
    status: completed
    who: karolkow
    note: >
      Merged in PR #387 (1c29dd43); review corrections in 0004eb75. 17 files,
      +448/-23, 4 new vitest cases (32 FE test files green), 228 Rust tests
      green, all 6 CI checks green including API types freshness. Measured on
      prod query_log: a whole list page is 13 ms / 461,849 rows / 6.83 MiB,
      a single detail 6 ms / 1.36 MiB. Three CodeRabbit doc findings fixed,
      all real, none in code. Follow-up 0472 spawned and extended with three
      accepted /ux-expert findings. NOT yet deployed — issue #368 stays open
      until production, per the repo convention that issues close at deploy.
  - date: '2026-08-10'
    status: active
    who: karolkow
    note: >
      Implemented on feat/0441. One shared fetch_sac_assets (batched IN +
      GROUP BY collapse, issuers via resolve_accounts, native by
      asset_type=0), sac_asset field on list + detail DTOs, linked SAC·CODE
      chip + "Mirrors asset" row. Field named sac_asset (not "mirrored") to
      match the is_sac / asset_sac family. EXPLAIN ESTIMATE re-measured:
      461,796 rows, plan identical for 1 vs 10 ids. All ACs checked; awaits
      PR + deploy. Issue #368 stays open until production.
  - date: '2026-08-10'
    status: active
    who: karolkow
    note: >
      Activated. Note: the stash@{2} referenced in the 2026-07-30 entry no
      longer holds this task's implementation — stash indices shifted during
      the 2026-07-30 cleanup and a content grep over all current stashes
      finds no SAC→asset reverse lookup. Implementation starts fresh from
      the LP-query pattern.
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
- [x] The query is issued **once per page** with every SAC id in one `IN` list —
      `fetch_sac_assets` is the single shared fn for list + detail; the list
      collects the page's SAC ids into one call (2026-08-10)
- [x] `read_rows` measured on the contract LIST page, not just the detail
      page; bounded as the asset table grows — `EXPLAIN ESTIMATE` on prod
      identical for 1 and 10 ids: 461,796 rows / 7 parts / 62 marks = one
      scan per page (table 443k→462k since July, linear; 5M bloom trigger
      stands) (2026-08-10)
- [x] Contract detail returns the mirrored classic asset when `is_sac` —
      `sac_asset: Option<SacAsset>` on detail + list DTOs
- [x] Reverse join collapses `asset_sac` duplicates (mirror the LP subquery) —
      `GROUP BY sac_contract_id` + `max()`; prod shows 1–3 facet rows per
      contract
- [x] `is_sac` true with no resolvable asset degrades to the current bare
      badge — unresolvable facet omitted from the map (2 of 3,946 on prod);
      covered by a vitest case
- [x] Native (XLM) SAC handled — detected by `asset_type = 0` (the prod facet
      row is `('', 0)`, so empty-issuer is NOT the signal); renders `SAC · XLM`
      → `/assets/native`; covered by a vitest case
- [x] Frontend links the asset; StrKey of the contract stays canonical —
      linked chip `SAC · CODE` on the list, "Mirrors asset" row (code +
      issuer) on the detail; 4 vitest cases in `ContractsTable.test.tsx`
- [x] **Docs updated** — backend-overview (both contract endpoints),
      frontend-overview §6.10, database-schema asset_sac readers note,
      canonical SQL 11 statement C
- [x] **API types regenerated** — `SacAsset` in
      `libs/api-types/src/generated/`, same commit

## Implementation Notes

Shipped in PR #387 (merged 2026-08-10, `1c29dd43`); doc corrections follow in
`0004eb75`. 17 files, +448/−23.

- **`fetch_sac_assets`** (`crates/api/src/contracts/queries.rs`) — the single
  reverse-lookup fn, shared by list and detail so both can never drift. One
  batched `IN` list per page, `GROUP BY sac_contract_id` + `max()`, issuers
  resolved through the existing `resolve_accounts` bloom seek. Returns
  `HashMap<i64, SacAsset>`; an unresolvable facet is simply absent from the
  map, so the caller degrades rather than emitting half an identity.
- **List** collects the page's `is_sac` ids and skips the call entirely when
  the page holds none. **Detail** calls it only when `is_sac`.
- **DTO** — `sac_asset: Option<SacAsset { asset_code, issuer }>` on both
  `ContractListItem` and `ContractDetailResponse`; `is_sac` kept for callers
  that only need the flag.
- **Frontend** — linked `SAC · CODE` chip on the list, "Mirrors asset" row
  (code + issuer) on the detail, both fed by `web/src/pages/contracts/
sacAsset.ts` (code + canonical `CODE-ISSUER | native` route token).
- **Tests** — 4 vitest cases in `ContractsTable.test.tsx`: classic link,
  native → `/assets/native`, unresolvable → bare unlinked badge, non-SAC →
  no chip.

### Measured on production (2026-08-10)

Real `system.query_log`, not `EXPLAIN`:

| Shape                    | duration | read_rows | read_bytes |
| ------------------------ | -------- | --------- | ---------- |
| 1 SAC id (detail)        | 6 ms     | 120,881   | 1.36 MiB   |
| 20 SAC ids (a list page) | 13 ms    | 461,849   | 6.83 MiB   |

The 20-id case reads the whole table — that is the ceiling, and a page costs
the same as a single contract. Well under the ~0.10 s the access-path decision
assumed.

## Issues Encountered

- **`stash@{2}` did not hold the implementation.** The 2026-07-30 history
  entry pointed there; stash indices had shifted during that day's cleanup and
  a content grep over all 25 stashes found no SAC→asset lookup anywhere.
  Started from the LP-query pattern instead. (Lesson: reference stashes by
  content, not index.)
- **Worktree package resolution.** `node_modules` symlinks to the main
  checkout, so `@rumblefish/api-types` resolved to develop's build and the FE
  typecheck reported `sac_asset` as non-existent. Fixed with a worktree-local
  shadow symlink; CI (fresh branch checkout) was never affected.
- **Merge conflict with 0465.** Both branches extended the same
  `contracts::dto` import list in `openapi/mod.rs` (`SacAsset` vs
  `DecompiledResponse`/`DecompileDiagnostic`). Resolved by keeping all three;
  the `components(schemas(...))` registration had merged cleanly on its own.
  `libs/api-types` was **regenerated** after the merge rather than trusted as
  a text merge of generated files — the regen produced no diff.
- **Pre-commit blocked on a foreign missing dep.** `prismjs` (0465's syntax
  highlighter) was declared in `package.json` but absent from the shared
  `node_modules`. Fixed with `npm install`, not `--no-verify`.
- **Three doc defects found in review**, all real: the canonical SQL
  contradicted itself on statement count; frontend-overview implied a NON-SAC
  contract shows a bare SAC badge (it shows no chip); and the asset route was
  written `/asset/native` when `routeSegments.asset` is `assets`. Docs-only —
  the code builds URLs through `routes.asset()`, never a literal.

## Design Decisions

### From Plan

1. **Accept the scan** (2026-07-30, see above) — no projection, no
   detail-only scoping. Upgrade path named: `bloom_filter` skip index past
   ~5M rows.
2. **One batched query per page**, never per row — the property that makes
   the scan affordable.
3. **Keep `is_sac`** beside the new object rather than replacing it.

### Emerged

4. **Named `sac_asset`, not `mirrored_asset`.** "Mirrored" was invented
   vocabulary; `sac_asset` matches the existing `is_sac` / `asset_sac`
   family. Decided with Karol during implementation.
5. **`max()` is a collapse of identical values, not a "newest" pick.**
   Questioned during review — correctly, since `asset_sac` has no version or
   time column. A SAC's address is derived deterministically from the asset
   identity, so one `sac_contract_id` can only ever map to one asset;
   measured **zero** contracts with more than one distinct
   `(asset_type, asset_code, issuer_id)`. The duplicate rows differ only in
   the carrier `contract_id` / `sac_deployed`. `max()` over equal values is
   `any()`, but deterministic if an ingest bug ever produced a second
   identity.
6. **Native detected by `asset_type = 0`**, not by an empty issuer. The prod
   facet row for the XLM SAC is literally `('', 0)`, so the empty-string form
   IS present here — it just is not the signal. Guards the trap in Notes.
7. **Issuer shown beside the code on the detail.** An asset code alone is
   ambiguous: prod carries many distinct issuers of "USDC". The list chip
   shows the code only (column width) and links to the fully-qualified asset.
8. **Type chips stay unlinked** (/ux-expert, 2026-08-10). A linked chip
   points at a different entity; a category label linking to the row's own
   asset would read as a filter. Recorded in [[0472]].

## Future Work

Spawned as **[[0472]]** — contract pages link + name what they represent:
Fungible → its asset page, NFT → its collection view, plus three accepted
/ux-expert findings on this task's UI (header chip names the asset, row label
"Asset", and the redundant `Token`+`SAC` chip pair collapsed to one).

That last one rests on a measurement worth keeping: on prod **`contract_type =
Token` ⟺ `is_sac`, exactly** (3,946 of 3,946; zero non-SAC type-0 contracts).
Soroban-native fungible tokens classify as `Fungible`, so the `Token` bucket
holds nothing but SACs and the double chip carries no information.

Also corrected here: "the query usually does not fire at all" is true of the
DEFAULT list page (newest 50 hold zero SACs) but **false of the `Token`
filter**, where every row is a SAC and the lookup runs on every page.

## Notes

Native XLM carries two competing conventions in this codebase (positive
surrogate from `hash64("native")` vs empty string). Use the surrogate form; the
empty-string form falls through filters silently.
