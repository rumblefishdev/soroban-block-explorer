---
id: '0241'
title: 'Backend: liquidity pool API extensions for FE detail/list (0077)'
type: FEATURE
status: backlog
related_adr: ['0027', '0029', '0031', '0032', '0041']
related_tasks: ['0077', '0199', '0215']
tags: [priority-high, effort-medium, layer-api, layer-docs, milestone-2]
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
      Spawned from 0077 deep-dive (FE liquidity pools list + detail vs Figma + backend reality).
      Verified non-conflicting vs ADR 0027 (LP schema), 0029 (no per-op stroops in DB →
      Phase 4 must use read-time XDR fetch), 0031 (pool_id), 0032 (evergreen docs),
      0041 (sentinel filter). Orthogonal to 0199 (LP analytics, blocked-on-oracle) and 0215
      (FE impact catalog) — these extensions live outside the oracle path.
---

# Backend: liquidity pool API extensions for FE detail/list (0077)

## Summary

Four additive backend extensions to the liquidity pool API surface, needed
to unblock FE task 0077 (frontend liquidity pools list + detail). Three are
small DB-only additions (single-asset filter, participant counts, envelope
total). The fourth is a heavier XDR-archive expand for per-transaction LP
amounts (deposit / withdraw / trade amount_a, amount_b, direction). Each
extension verified against existing ADRs + canonical SQL specs.

## Status: Backlog

**Current state:** Not started. Spawned from 0077 deep-dive on 2026-05-20.

## Context

FE task 0077 deep-dive revealed four backend API gaps that block 1:1 Figma
implementation:

1. **Asset filter.** Figma list filter is a single text input ("Filter by
   asset pair"). API today requires per-leg exact match
   (`filter[asset_a_code]` + `filter[asset_a_issuer]` +
   `filter[asset_b_code]` + `filter[asset_b_issuer]`, all-or-nothing per
   leg). UX requires a simple single-asset variant.

2. **Participant count in list.** Figma list shows a "Participants" column
   per pool. `PoolItem` response has no participant count field.

3. **Participant total on detail.** Figma detail KPI shows
   "Participants: 1,284 liquidity providers". `GET /liquidity-pools/:id/participants`
   paginates the list, does not return total count.

4. **Per-transaction LP amounts.** Figma "Recent transactions" section
   shows per-row LP-specific amounts: trades as `100 XLM → 40 USDC`,
   deposits as `5,000 XLM + 2,000 USDC`, withdrawals as
   `10,000 XLM + 4,000 USDC`. `PoolTransactionItem` only returns
   `operation_types[]` (op-name array), no amount fields. Per ADR 0029,
   per-op stroop amounts are not stored in the DB — must be decoded from
   the XDR archive at read time.

This task scopes all four. Frontend 0077 depends on this landing.

## Scope vs 0199 (LP analytics, blocked-on-oracle)

Task 0199 owns `tvl, volume, fee_revenue` per snapshot (USD-denominated,
depends on Oskar's price oracle). This task is **orthogonal**:

- 0199 = aggregate per snapshot, USD-denominated, blocked on oracle.
- This task = scoped helpers (counts, asset-code filter, per-tx amounts in
  source asset units, no USD multiplication).

The four extensions here do not touch the oracle path and can ship without 0199. Per 0215, the participants and per-tx transactions endpoints are
"fully populated, no oracle dependency" — these extensions stay in that
lane.

## Implementation Plan

### Phase 1 — `filter[asset_code]` on list endpoint (small)

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
- Compute via `COUNT(*) OVER()` window inside paginated CTE, or a separate
  COUNT query (prefer window — single round-trip).
- 404 unchanged when pool does not exist. Empty pool returns
  `total_count = 0`.
- This is one of the first envelope-style responses; the wrapper sets
  precedent. Document in OpenAPI.

### Phase 4 — `expand=lp_op_details` on transactions endpoint (XDR fetch)

- **Endpoint:** `GET /liquidity-pools/:id/transactions`
- **Files:** `crates/api/src/liquidity_pools/handlers.rs::list_pool_transactions`,
  `crates/api/src/liquidity_pools/dto.rs::PoolTransactionItem`,
  `docs/architecture/database-schema/endpoint-queries/20_get_liquidity_pools_transactions.sql`
- **Pattern:** ADR 0029 (read-time XDR fetch). Reuse E3 archive-fetch
  infrastructure (`crates/api/src/transactions/handlers.rs::get_transaction`
  → `extract_e3_heavy` → `xdr_parser::extract_operations`).
- Add optional query param `expand: Option<String>` accepting
  `"lp_op_details"` (later: comma-separated list if more expands appear).
- When set: server batches archive fetches per unique `ledger_sequence`
  in the result set (at most one S3 GET per ledger), parses both
  `envelope_xdr` and `result_meta_xdr` per tx, extracts LP-specific
  amounts. Without `expand`: response shape unchanged (backward
  compatible).
- New optional field on `PoolTransactionItem`:

  ```rust
  pub struct PoolTransactionItem {
      // ... existing fields ...
      pub lp_op_details: Option<LpOpDetails>,  // populated only when expand=lp_op_details
  }

  pub struct LpOpDetails {
      pub lp_op_type: LpOpType,                // "trade" | "deposit" | "withdrawal"
      pub amount_a: NumericString,             // pool's asset_a side
      pub amount_b: NumericString,             // pool's asset_b side
      pub direction: Option<TradeDirection>,   // Some only for trades
      pub details_status: ExpandStatus,        // "available" | "unavailable"
  }
  ```

- **Classification rules:**
  - Tx contains `liquidity_pool_deposit` op for this pool → `deposit`.
    `amount_a/b` from envelope op's `reserves_deposited_a/b`.
  - Tx contains `liquidity_pool_withdraw` op for this pool → `withdrawal`.
    `amount_a/b` from envelope op's `reserves_received_a/b`.
  - Tx contains `path_payment_strict_*` whose `result_meta_xdr.claimedOffers[]`
    references this `pool_id` → `trade`. Derive `amount_a/b/direction`
    from `claimedOffers[].amount_sold` + `amount_bought` filtered to this
    pool's leg.
  - **Multi-op tx:** aggregate per LP op kind. Conflict resolution if both
    deposit+trade in same tx (rare): deposit/withdraw classification wins
    over trade.
- **Graceful degradation:** archive miss / parse error →
  `details_status: "unavailable"`, amount fields NULL. Same pattern as
  E3 `heavy_fields_status`.
- **Batching:** group rows by `ledger_sequence` before fetching; one S3
  GET + one decode pass per distinct ledger; map back to rows.
- **Latency budget:** target alignment with E3 — ~50–150 ms per ledger
  fetch (S3 + zstd + parse). Worst case (20-row page hitting 20 distinct
  ledgers) = 1–3 s; typical (5–10 ledgers) = 250–750 ms. Document SLA in
  endpoint comment.
- **Caching:** reuse existing E3 archive-fetch cache if present; else add
  per-ledger LRU keyed by `ledger_sequence`.

### Phase 5 — Docs (per ADR 0032)

- `docs/architecture/database-schema/endpoint-queries/18_*.sql` — add
  `filter[asset_code]` clause + `participant_count` projection.
- `docs/architecture/database-schema/endpoint-queries/19_*.sql` — add
  `participant_count` projection.
- `docs/architecture/database-schema/endpoint-queries/20_*.sql` — add
  `expand=lp_op_details` semantics + XDR fetch reference to ADR 0029.
- `docs/architecture/database-schema/endpoint-queries/23_*.sql` — add
  `total_count` window function + envelope shape.
- `docs/architecture/backend-overview.md` — update §6.3 (E18, E19), §6.4
  (E20), §6.13 / §6.14 frontend impact tables.
- OpenAPI regenerated:
  `npx nx run @rumblefish/api-types:generate` (CI gate
  `API types freshness`).

### Phase 6 — Tests

- Handler-level unit tests: query param parsing, validation, error mapping
  per phase.
- DB integration tests (seeded): `filter[asset_code]` matches either leg,
  case-insensitive; `participant_count` accurate for 0 / 1 / many positions;
  `total_count` matches `data.len()` when no pagination, exceeds it on page 2.
- Phase 4: integration test with archive mock — `details_status: "available"`
  for happy path; `"unavailable"` on archive miss; classification correctness
  for deposit / withdraw / trade fixtures.
- OpenAPI snapshot test (drift detection).

## Acceptance Criteria

- [ ] `filter[asset_code]` on `GET /liquidity-pools` matches either leg
      (case-insensitive)
- [ ] Existing per-leg filters (`asset_a_*`, `asset_b_*`) unchanged + still
      work; both modes can combine
- [ ] `participant_count: i64` returned on `PoolItem` (list + detail),
      accurate vs `lp_positions WHERE shares > 0`
- [ ] `GET /liquidity-pools/:id/participants` returns
      `{ data, total_count, cursor }` envelope
- [ ] `expand=lp_op_details` populates `lp_op_details` on
      `PoolTransactionItem` per row
- [ ] Classification matches Figma badge intent: deposit / withdrawal /
      trade
- [ ] `amount_a`, `amount_b` are NUMERIC decimal strings preserving
      precision (no f64 round-trip)
- [ ] `direction` is `Some` only for trades, NULL for deposit / withdrawal
- [ ] Archive miss → `details_status: "unavailable"`, amount fields NULL;
      no 500
- [ ] Sentinel pools (`created_at_ledger = 0`) excluded from all five
      endpoints (defense-in-depth per ADR 0041)
- [ ] All four canonical SQL specs updated under
      `docs/architecture/database-schema/endpoint-queries/`
- [ ] `backend-overview.md` §6.3 / §6.4 / §6.13 / §6.14 updated
- [ ] OpenAPI types regenerated (CI gate `API types freshness` green)
- [ ] Unit + integration tests pass; OpenAPI snapshot test passes

## Notes

- Phase 4 is the heaviest. If under time pressure: ship Phases 1–3 first
  and split Phase 4 into a follow-up task. Frontend can render the
  Transactions section without the amount column (drop column, keep
  type / hash / account / time) and add it back when Phase 4 lands.
- ADR 0029 is the binding constraint for Phase 4 — no per-op stroop
  storage in DB; read-time XDR fetch is the canonical pattern.
- No new schema migrations. Phases 2 + 3 use existing tables
  (`lp_positions`, `liquidity_pool_snapshots`); Phase 4 reuses E3 XDR
  fetch infrastructure if present.
- Status badge in Figma detail header (Active / Stale) uses existing
  `latest_snapshot_at` field — zero backend change.
- Pool ID strkey ("L...") encoding stays frontend-side — zero backend
  change.
- USD-denominated `tvl / volume / fee_revenue` (task 0199) is **out of
  scope**; those fields remain NULL on stale pools regardless of this
  task.
