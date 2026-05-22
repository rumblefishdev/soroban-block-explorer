---
id: '0246'
title: 'Backend: liquidity pool API extensions for FE list/detail (0077)'
type: FEATURE
status: completed
related_adr: ['0027', '0031', '0032', '0041']
related_tasks: ['0077', '0199', '0215', '0247']
tags: [priority-high, effort-small, layer-api, layer-docs, milestone-2]
milestone: 2
links:
  - https://www.figma.com/design/n1p6WCMVd4iinbuvOA2WjP/Designs?node-id=266-35969
  - https://www.figma.com/design/n1p6WCMVd4iinbuvOA2WjP/Designs?node-id=267-59942
  - https://www.figma.com/design/n1p6WCMVd4iinbuvOA2WjP/Designs?node-id=325-7098
  - https://www.figma.com/design/n1p6WCMVd4iinbuvOA2WjP/Designs?node-id=325-24354
history:
  - date: 2026-05-20
    status: backlog
    who: karolkow
    note: >
      Spawned from 0077 deep-dive (FE liquidity pools list + detail vs
      Figma + backend reality). Three DB-only additions; per-tx LP
      amounts (originally Phase 4) split off into 0247 RESEARCH because
      XDR-archive fetch latency on hot path needs benchmark + arch
      decision before commit.
      Verified non-conflicting vs ADR 0027 (LP schema), 0031 (pool_id),
      0032 (evergreen docs), 0041 (sentinel). Orthogonal to 0199 (LP
      analytics, blocked-on-oracle) and 0215 (FE impact catalog).
  - date: 2026-05-20
    status: backlog
    who: karolkow
    note: >
      Renumbered 0241 → 0246. Origin commit c82c9fa (M1-M3 sequencing
      plan, merged concurrently) had already grabbed 0241 for the
      indexer hard-swap task. Sister research task renumbered 0242 → 0247
      in the same operation. Earlier commit messages (cac0215, ddbbb34)
      retain the original `lore-0241` scope — left intact per
      no-amend convention; this history entry is the renumber trail.
  - date: 2026-05-20
    status: active
    who: karolkow
    note: >
      Promoted backlog → active. Starting implementation on
      feat/0246_backend-liquidity-pools-api-extensions branch.
  - date: 2026-05-20
    status: active
    who: karolkow
    note: >
      Phase 1 (filter[asset_code]) shipped — commit 193e269.
      Phase 2 (participant_count on PoolItem) shipped — commit f105f36.
      Phase 3 (total_count envelope) dropped mid-implementation as
      duplicate of Phase 2 data — see Design Decisions → Emerged.
  - date: 2026-05-22
    status: completed
    who: karolkow
    note: >
      Done. Phases 1 + 2 shipped (commits 193e269, f105f36); Phase 3
      intentionally dropped (documented under Design Decisions → Emerged
      #3). Docs (E18/E19 SQL specs, backend-overview §6.3, frontend-overview
      §6.13/§6.14), OpenAPI regen + CI gate green, unit + integration tests
      green. All acceptance criteria satisfied; one criterion (Phase 3
      envelope) struck through with rationale, not deferred. Status/folder
      hygiene was the only outstanding item — now archived.
---

# Backend: liquidity pool API extensions for FE list/detail (0077)

## Summary

Three additive, DB-only backend extensions to the liquidity pool API
surface, needed to unblock FE task 0077 (frontend liquidity pools list +
detail): single-asset filter, participant counts on `PoolItem`, and
`total_count` envelope on the participants endpoint. Per-tx LP amounts
(originally scoped as Phase 4) extracted into 0247 RESEARCH — that path
requires XDR archive fetch on the hot read path and needs benchmark +
arch decision before commit.

## Status: Backlog

**Current state:** Not started. Spawned from 0077 deep-dive on 2026-05-20.

## Context

FE task 0077 deep-dive revealed four backend API gaps that block 1:1
Figma implementation. Three are pure DB additions and ship together in
this task. The fourth (per-tx LP amounts) is heavier and lives in 0247.

1. **Asset filter.** Figma list filter is a single text input ("Filter by
   asset pair"). API today requires per-leg exact match
   (`filter[asset_a_code]` + `filter[asset_a_issuer]` +
   `filter[asset_b_code]` + `filter[asset_b_issuer]`, all-or-nothing per
   leg). UX requires a simple single-asset variant.

2. **Participant count in list.** Figma list shows a "Participants"
   column per pool. `PoolItem` response has no participant count field.

3. **Participant total on detail.** Figma detail KPI shows
   "Participants: 1,284 liquidity providers".
   `GET /liquidity-pools/:id/participants` paginates the list, does not
   return total count.

4. **Per-transaction LP amounts.** (Split into 0247 RESEARCH.) Figma
   "Recent transactions" section shows per-row LP-specific amounts.
   Per ADR 0029, per-op stroop amounts are not stored in the DB.
   XDR-archive read-time fetch is one option; latency + alternatives
   need benchmark before implementation. See 0247.

This task scopes items 1–3. Frontend 0077 depends on this landing.

## Scope vs 0199 (LP analytics, blocked-on-oracle)

Task 0199 owns `tvl, volume, fee_revenue` per snapshot (USD-denominated,
depends on Oskar's price oracle). This task is **orthogonal**:

- 0199 = aggregate per snapshot, USD-denominated, blocked on oracle.
- This task = scoped helpers (counts, asset-code filter).

The three extensions here do not touch the oracle path and can ship
without 0199. Per 0215, the participants endpoint is "fully populated,
no oracle dependency" — these extensions stay in that lane.

## Implementation Plan

### Phase 1 — `filter[asset_code]` on list endpoint

- **Endpoint:** `GET /liquidity-pools`
- **Files:** `crates/api/src/liquidity_pools/handlers.rs::list_pools`,
  `docs/architecture/database-schema/endpoint-queries/18_get_liquidity_pools_list.sql`
- Add optional query param `filter[asset_code]: Option<String>`. Trim +
  uppercase normalize before query. Case-insensitive exact match against
  `asset_a_code` or `asset_b_code`.
- WHERE clause additive:
  `(asset_a_code = $N OR asset_b_code = $N)` joined with AND to existing
  filters.
- Existing per-leg params (`asset_a_code`, `asset_a_issuer`,
  `asset_b_code`, `asset_b_issuer`) **unchanged** — both modes coexist.
- Indexes already cover both legs:
  `idx_pools_asset_a (asset_a_code, asset_a_issuer_id)` +
  `idx_pools_asset_b (asset_b_code, asset_b_issuer_id)` from
  `20260428000100_add_endpoint_query_indexes.up.sql`.
- OpenAPI: document param + note that per-leg params remain for issuer
  disambiguation (power-user / API-consumer path).

### Phase 2 — `participant_count` on `PoolItem` (list + detail)

- **Endpoints:** `GET /liquidity-pools` **and** `GET /liquidity-pools/:id`
- **Files:** `crates/api/src/liquidity_pools/dto.rs::PoolItem`,
  `docs/architecture/database-schema/endpoint-queries/18_*.sql,19_*.sql`
- Add `participant_count: i64` to `PoolItem`.
- Compute via correlated subquery:
  `(SELECT COUNT(*) FROM lp_positions lpp WHERE lpp.pool_id = lp.pool_id AND lpp.shares > 0)`
- Index `idx_lpp_shares (pool_id, shares DESC) WHERE shares > 0` (from
  `0006_liquidity_pools.sql`) covers it.
- Same projection in both E18 (list) and E19 (detail) — DTO shared.
- Stale pools (no fresh snapshot): `participant_count` is still computed
  (no oracle dependency). Only `tvl/volume/fee_revenue` stay NULL.

### Phase 3 — `total_count` on participants envelope — **DROPPED**

**Status:** dropped mid-implementation. See Design Decisions → Emerged
(2026-05-20) for the full rationale.

Short version: `total_count` on the participants envelope would have
duplicated the per-pool active-participant count that Phase 2 already
surfaces on `PoolItem` (returned by both `GET /liquidity-pools` and
`GET /liquidity-pools/:id`). The frontend's "1,284 liquidity providers"
KPI on the LP detail page (Figma §6.14) reads that field directly from
the detail call — no second source needed. Implementing this phase
would have meant: a window function over the full active-participant
set per request (vs cheap `LIMIT 20` index walk today), a canonical
`PageInfo.total_count` extension cascading on every existing list
handler (`assets/`, `contracts/`, etc.), and zero additional UX. Pure
over-engineering; revisit only when a concrete FE deep-link surface
without a prior detail prefetch shows up.

### Phase 4 — Docs (per ADR 0032)

- `docs/architecture/database-schema/endpoint-queries/18_*.sql` — add
  `filter[asset_code]` clause + `participant_count` projection.
- `docs/architecture/database-schema/endpoint-queries/19_*.sql` — add
  `participant_count` projection.
- ~~`docs/architecture/database-schema/endpoint-queries/23_*.sql` — add
  `total_count` window function + envelope shape.~~ — dropped with
  Phase 3.
- `docs/architecture/backend-overview.md` — update §6.3 (E18, E19),
  §6.13 / §6.14 frontend impact tables.
- OpenAPI regenerated:
  `npx nx run @rumblefish/api-types:generate` (CI gate
  `API types freshness`).

### Phase 5 — Tests

- Handler-level unit tests: query param parsing, validation, error
  mapping per phase.
- DB integration tests (seeded): `filter[asset_code]` matches either
  leg, case-insensitive; `participant_count` accurate for 0 / 1 / many
  positions.
- OpenAPI snapshot test (drift detection).

## Acceptance Criteria

- [x] `filter[asset_code]` on `GET /liquidity-pools` matches either leg
      (case-insensitive) — Phase 1, commit 193e269
- [x] Existing per-leg filters (`asset_a_*`, `asset_b_*`) unchanged +
      still work; both modes can combine — Phase 1, commit 193e269
- [x] `participant_count: i64` returned on `PoolItem` (list + detail),
      accurate vs `lp_positions WHERE shares > 0` — Phase 2, commit f105f36
- [ ] ~~`GET /liquidity-pools/:id/participants` returns
      `{ data, total_count, cursor }` envelope~~ — **dropped** (Phase 3
      cancelled; see Design Decisions → Emerged)
- [x] Sentinel pools (`created_at_ledger = 0`) excluded from all
      affected endpoints (defense-in-depth per ADR 0041)
- [x] Two canonical SQL specs updated under
      `docs/architecture/database-schema/endpoint-queries/` (18, 19;
      23 unchanged after Phase 3 drop)
- [x] `backend-overview.md` §6.3 + `frontend-overview.md` §6.13 / §6.14
      updated
- [x] OpenAPI types regenerated (CI gate `API types freshness` green)
- [x] Unit + integration tests pass; OpenAPI snapshot test passes

## Design Decisions

### From Plan

1. **`filter[asset_code]` coexists with per-leg filters.** Single-asset
   convenience for the Figma list input (`USDC` / `XLM`) without
   removing the per-leg `(code, issuer)` path that API consumers need
   for issuer disambiguation. Both modes combine additively in the
   WHERE clause.

2. **`participant_count` is snapshot-independent.** Computed via
   correlated subquery against `lp_positions` (partial index
   `idx_lpp_shares` covers it) — not part of the snapshot freshness
   window. Stale pools still get an accurate count; only the
   USD-denominated `tvl/volume/fee_revenue` stay NULL pending the
   oracle work in task 0199.

### Emerged

3. **2026-05-20 — Phase 3 dropped.** The original plan added
   `page.total_count` (envelope-side) on
   `GET /liquidity-pools/:id/participants` to back the "1,284 liquidity
   providers" KPI on the LP detail page (Figma §6.14). Senior-fresh-eye
   review during implementation flagged this as duplication:

   - The same number is already on `PoolItem.participant_count` (Phase 2)
     in the detail call that the FE makes anyway when it opens the LP
     detail page. The KPI reads from that field.
   - The participants list endpoint is only ever opened from inside
     that detail view (no current FE deep-link surface targeting
     `…/participants` directly without a prior detail call).
   - Implementing the envelope addition would have cost (a) a window
     function over the full active-participant set per request — 64×
     more rows scanned per page than the current `LIMIT 20` index walk
     on a 1,284-LP pool; (b) a canonical `PageInfo.total_count`
     extension cascading on every existing list handler in the API
     crate; (c) zero user-visible UX gain.

   Net: drop. If a future deep-link surface emerges (FE adds a route
   that lands directly on `…/participants` without a detail prefetch),
   revisit by either (i) populating `total_count` on the envelope at
   that point or (ii) keeping the detail call as the canonical source
   and prefetching it client-side. Reactive, not speculative.

   No envelope canonical change. Phase 3 reverted in working tree;
   `crates/api/src/openapi/schemas.rs::PageInfo` stays at its pre-0246
   shape.

## Notes

- Per-tx LP amounts (formerly Phase 4) extracted into **0247 RESEARCH**
  — XDR archive fetch latency on hot read path is the open architectural
  question; needs benchmark before committing to expand pattern. FE 0077
  can ship without amount column (drop column from Transactions section
  for MVP) and add it later when 0247 conclusion lands.
- No new schema migrations. Phases 2 + 3 use existing tables
  (`lp_positions`).
- Status badge in Figma detail header (Active / Stale) uses existing
  `latest_snapshot_at` field — zero backend change.
- Pool ID strkey ("L...") encoding stays frontend-side — zero backend
  change.
- USD-denominated `tvl / volume / fee_revenue` (task 0199) is **out of
  scope**; those fields remain NULL on stale pools regardless of this
  task.
