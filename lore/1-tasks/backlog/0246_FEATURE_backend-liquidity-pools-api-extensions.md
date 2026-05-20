---
id: '0246'
title: 'Backend: liquidity pool API extensions for FE list/detail (0077)'
type: FEATURE
status: backlog
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

### Phase 3 — `total_count` on participants envelope

- **Endpoint:** `GET /liquidity-pools/:id/participants`
- **Files:** `crates/api/src/liquidity_pools/handlers.rs::list_participants`,
  `crates/api/src/liquidity_pools/dto.rs`,
  `docs/architecture/database-schema/endpoint-queries/23_get_liquidity_pools_participants.sql`
- Introduce wrapper DTO:
  ```rust
  pub struct ParticipantsResponse {
      pub data: Vec<ParticipantItem>,
      pub total_count: i64,
      pub cursor: Option<String>,
  }
  ```
- Compute via `COUNT(*) OVER()` window inside paginated CTE, or a
  separate COUNT query (prefer window — single round-trip).
- 404 unchanged when pool does not exist. Empty pool returns
  `total_count = 0`.
- This is one of the first envelope-style responses; the wrapper sets
  precedent. Document in OpenAPI.

### Phase 4 — Docs (per ADR 0032)

- `docs/architecture/database-schema/endpoint-queries/18_*.sql` — add
  `filter[asset_code]` clause + `participant_count` projection.
- `docs/architecture/database-schema/endpoint-queries/19_*.sql` — add
  `participant_count` projection.
- `docs/architecture/database-schema/endpoint-queries/23_*.sql` — add
  `total_count` window function + envelope shape.
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
  positions; `total_count` matches `data.len()` when no pagination,
  exceeds it on page 2.
- OpenAPI snapshot test (drift detection).

## Acceptance Criteria

- [ ] `filter[asset_code]` on `GET /liquidity-pools` matches either leg
      (case-insensitive)
- [ ] Existing per-leg filters (`asset_a_*`, `asset_b_*`) unchanged +
      still work; both modes can combine
- [ ] `participant_count: i64` returned on `PoolItem` (list + detail),
      accurate vs `lp_positions WHERE shares > 0`
- [ ] `GET /liquidity-pools/:id/participants` returns
      `{ data, total_count, cursor }` envelope
- [ ] Sentinel pools (`created_at_ledger = 0`) excluded from all
      affected endpoints (defense-in-depth per ADR 0041)
- [ ] Three canonical SQL specs updated under
      `docs/architecture/database-schema/endpoint-queries/`
- [ ] `backend-overview.md` §6.3 / §6.13 / §6.14 updated
- [ ] OpenAPI types regenerated (CI gate `API types freshness` green)
- [ ] Unit + integration tests pass; OpenAPI snapshot test passes

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
