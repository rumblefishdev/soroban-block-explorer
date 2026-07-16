---
id: '0364'
title: 'PERF: astlist + astdetail — replace whole-`assets FINAL` scan with a bounded seek/dedup (shared assets-select refactor)'
type: PERF
status: done
related_adr: ['0051']
related_tasks: ['0357', '0354', '0334', '0398']
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
  - date: 2026-07-16
    status: done
    who: karolkow
    note: >
      Shipped as a two-phase read (FINAL-free PK seek + Rust consecutive-dedup →
      hydration bounded to the page/lookup keys), PR #343 merged to develop. 17
      commits, re-split into small in-place diffs. Byte-identity verified on prod
      over ALL 333,559 assets (0 diffs: list projection, key-set == FINAL, detail
      deployed_at_ledger, native constructed key, code+issuer determinism, arm-A
      shape). Prod read cost: astlist driving scan 3.24M → 15k rows (211×),
      astdetail 1.77M → ~38k (~46×). clippy 0 / 239 tests / api-types 0 diff.
      One accepted behavior change: SAC `C…` addresses under /assets/ now 404
      (arm B dropped) — signed off, app never links /assets/{SAC}. Spawned 0398
      (contract-surrogate redundancy). ADR 0051 (SAC-as-facet) is the basis for
      the arm-B drop.
  - date: 2026-07-16
    status: done
    who: karolkow
    note: >
      Post-merge 4-agent audit (/review, /simplify, /devils-advocate, checklist).
      Verdict: production-ready, fundamental fix for the headline scan. Corrected
      two of my own numbers: soroban_contract_metadata is ~3.8k rows (NOT ~159k —
      I conflated it with soroban_contracts); the metadata FINAL id-IN bound
      measured worse (214k→331k) because the pruning subquery re-scans
      soroban_contracts, so it stays unbounded but the cost is trivial. The real
      residual on type-3 pages is the soroban_contracts read (~129k ids / ~289k
      physical), bounded by id-IN but poorly bloom-pruned — so "no whole-dimension
      read remains" / "~200×" hold for classic, not type-3. assets max
      versions-per-key = 4 (not 7; SEEK_OVERFETCH=8 comment is stale). Follow-ups
      surfaced → see Future Work (F1 search-join under-fill, F2 skip redundant
      seek, F3 cleanups, F5 FE isAssetId 404 for external SAC deep-links).
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

- [x] astlist + astdetail read_rows bounded to the working set (not the ~2M
      whole-`assets FINAL` scan); verified via `system.query_log`. Driving scan
      astlist 3.24M → 15k rows, astdetail 1.77M → ~38k end-to-end.
- [x] Outputs byte-identical to pre-change — verified on prod over ALL 333,559
      assets (list projection, key-set == FINAL, detail `deployed_at_ledger`,
      native, code+issuer, arm-A shape): 0 diffs. One INTENDED change: SAC `C…`
      under /assets/ → 404 (arm B dropped, ADR 0051, signed off).
- [~] No whole-dimension read remains — `assets FINAL` (2M) eliminated;
  `asset_enrichment` / `asset_sac` / issuer bounded by IN-list. Two residuals:
  (a) **`soroban_contract_metadata FINAL` left whole-table — but it is only
  ~3.8k rows** (trivial; id-IN bound measured WORSE, 214k→331k, because the
  pruning subquery re-scans `soroban_contracts`). (b) The `soroban_contracts`
  read is `WHERE id IN (…)` but the `idx_sc_id` bloom prunes poorly (~15k rows
  for one id), so a type-3 page still touches most of that dimension (~129k
  ids / ~289k physical). So "no whole-dimension read remains" holds for classic
  pages, only partially for type-3. Both are ≪ the eliminated 2M scan.
- [x] p95: aim `< 200 ms` — astdetail ~100 ms; astlist ~250 ms (analytical list,
      flat 200 ms not guaranteed on single-node CH, as flagged). Driving-scan
      collapse is what makes it load-resistant.
- [x] **Docs updated** — N/A (no projection/index added).
- [x] **API types regenerated** — N/A (query-internal; `check-generated` 0 diff).

## Notes

- Sibling of the 0357 cluster: nftdetail (#314) + asttxs driver (#315) done;
  acclist (0353) is a CH-rejected-projection known-issue; the LP snapshot
  endpoints (lplist / lpdetail / lpchart) are blocked on 0356 / #318; the
  tx-list family (asttxs / acctxs / lptxs) shares a separate
  `operations_appearances` entity-keyed MV.
- `< 200 ms` not guaranteed — this is a list / analytical endpoint; the win is
  removing the whole-table scan, which is what makes it load-resistant.

## Implementation Notes

All in `crates/api/src/assets/queries.rs`. Two-phase shape:

- **Phase 1 — seek** (`build_list_seek_sql` / detail seeks): the identity 4-tuple
  (+ `id`) from `assets` WITHOUT `FINAL`, in PK order, over-fetched ×8
  (`SEEK_OVERFETCH`) for a Rust consecutive-dedup (`dedup_consecutive`) that keeps
  the first version per key. Deterministic because every projected `a` column is
  byte-identical across a key's physical versions.
- **Phase 2 — hydrate** (`hydrate_assets` / `hydrate_sql` / `resolve_soroban_contracts`):
  side tables bounded to the resolved keys (4-tuple / id IN-lists). The
  `assets` header, the `soroban_contracts` context (StrKey + deploy + metadata,
  one bloom seek, `argMax(_, wasm_uploaded_at_ledger)` for version-correct dedup),
  and the per-page issuer seek run concurrently under `tokio::join`.
- **Detail forms:** `fetch_native` hydrates the compile-time NATIVE key directly
  (no seek); `fetch_by_code_issuer` resolves the issuer StrKey → surrogate then
  seeks `(code, issuer_id)`; `fetch_by_contract_id` PK-seeks `(3,'',0,surrogate)`
  (bespoke type-3 only).

Verification harness: OLD (`assets FINAL`) vs NEW SQL diffed on prod over the
whole `assets` table via `system` (0 diffs), read cost from `system.query_log`.

## Issues Encountered

- **`deployed_at_ledger` non-determinism (caught in review):** `soroban_contracts`
  is `ReplacingMergeTree(wasm_uploaded_at_ledger)` with ~11% low-version NULL
  "stub" rows. A raw read + arbitrary-version dedup surfaced NULL
  non-deterministically. Fixed with `argMax(_, wasm_uploaded_at_ledger)`. Not a
  regression — the pre-0364 select hid it behind the bigger `assets FINAL` scan.
- **metadata FINAL id-IN bound backfired:** bounding
  `soroban_contract_metadata FINAL` by `contract_id IN (subquery of soroban_contracts)`
  made the list read WORSE (214k → 331k rows) — the pruning subquery scans
  `soroban_contracts` whole (id-IN does not prune there). Reverted; the metadata
  table is only ~3.8k rows so the whole-table FINAL is trivial anyway. (Note: my
  earlier "~159k floor" was wrong — that figure is `soroban_contracts`, not the
  metadata table; the post-merge audit corrected it.)
- **Re-split dropped the parallel issuer seek:** splitting the big commit into
  small in-place diffs accidentally inlined the issuer resolve to run
  sequentially after hydration. Caught by diffing against the replaced commits;
  restored `resolve_page_issuers` + `tokio::join`.

## Design Decisions

### From Plan

1. **Two-phase seek + bounded hydrate** (Steps 1–3). Drop `FINAL`, read in PK
   order, dedup in Rust, bound every side table to the working set.
2. **approach-B over `LIMIT 1 BY`** for the list dedup — `LIMIT 1 BY` defeats
   `optimize_read_in_order`; measured 41× cheaper to over-fetch + Rust-dedup.

### Emerged

3. **Dropped arm B (SAC-facet) of `fetch_by_contract_id`** — a pasted SAC `C…`
   no longer aliases the wrapped asset (→ 404). ADR 0051: a SAC is a facet
   addressed by CODE-ISSUER; the app never links `/assets/{SAC}`. This is what
   let the point-lookup become a clean PK seek instead of a scan. **Accepted
   behavior change** (signed off 2026-07-16).
4. **`fetch_native` skips the phase-1 seek** — the native key `(0,'',0,0)` /
   `NATIVE_ASSET_ID` is a compile-time constant, hydrated directly.
5. **opcja A — the list skips SAC-wrapper deploy resolution** (`with_sac_wrapper`
   flag false on the list path): `AssetItem` (list DTO) carries no
   `deployed_at_ledger`, so resolving it there is wasted work; the detail path
   keeps it.
6. **`assemble_asset_row` moves the name/decimals/deploy coalesce out of SQL into
   Rust** (2c) so `soroban_contracts` is read once per request, not twice per row.
7. **metadata FINAL left whole-table** — see Issues; only ~3.8k rows, and pruning
   it costs more than it saves.

## Future Work

Post-merge 4-agent audit (2026-07-16) surfaced these. None block; all are
follow-ups on an already-shipped, verified change.

- **0398** — `REFACTOR: contract-surrogate redundancy / naming` (the
  `contract_id_key` / `sac_contract_surrogate` / StrKey-vs-surrogate overlap in
  the assets row). Spawned from this task's hydration rework.
- **F1 (search-path under-fill):** `build_list_seek_sql`'s search join `LEFT JOIN
soroban_contracts` is NOT deduped; ~95% of `soroban_contracts` ids carry >1
  version, so a type-3-heavy search page multiplies raw rows ~2.2× (max 6×),
  shrinking the over-fetch's distinct-key headroom. Under clustering it can
  under-fill → short page → `finalize_page` emits no `next_cursor` → tail
  unreachable by pagination (the guard only `warn!`s). Fix: dedup the search join
  (`argMax(contract_id, wasm_uploaded_at_ledger) GROUP BY id`) so it can't
  multiply. Low probability today (max assets versions = 4 ≪ 8) but a real cliff.
- **F2 (skip redundant seek):** `fetch_by_contract_id` still runs a phase-1 seek
  whose match tuple is fully known (`(3,'',0,surrogate)`, and `id == surrogate`
  deterministically for type-3), so it can synthesize the key and hydrate
  directly like `fetch_native` — one fewer serial CH round-trip.
- **F3 (cleanups):** `resolve_page_issuers` reinvents `id_in_list`; a stale doc
  paragraph sits above `hydrate_sql`; the `SEEK_OVERFETCH` comment says "max 7"
  (measured max is 4); a comment count drifted (360k vs 0.6M).
- **F5 (FE `isAssetId` inconsistency — product decision):** `isAssetId()` returns
  true for a SAC `C…`, and `AssetDetailPage` uses it as a fetch guard, so an
  externally-pasted / bookmarked `/assets/{SAC}` now renders a 404 (in-app links
  route SAC → `/contracts/`, so no in-app regression). Either redirect
  `/assets/{SAC}` → `/contracts/{SAC}` on the FE, or accept the 404. Needs a
  product call.
