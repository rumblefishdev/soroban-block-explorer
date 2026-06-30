---
id: '0336'
title: 'BUG: classic↔SAC asset duplication — one economic asset stored as two `assets` rows (classic_credit + sac) + non-deterministic by-code-issuer resolver'
type: BUG
status: superseded
related_adr: []
related_tasks: ['0154', '0323', '0339']
tags: [clickhouse, sac, assets, api, layer-data, priority-medium, effort-medium]
links: []
history:
  - date: '2026-06-29'
    status: backlog
    who: claude
    note: >
      Spawned from a SAC/asset modeling analysis session. Concretizes the
      "XLM ↔ XLM SAC link gap" that task 0154 explicitly deferred
      (0154 README, Out of scope). Sibling of 0323 (un-deployed SAC → asset).
  - date: '2026-06-30'
    status: superseded
    who: claude
    by: ['0339']
    note: >
      Superseded by 0339 (SAC = facet of classic_credit). The read-collapse here was a
      band-aid for the classic↔SAC duplication symptom; 0339 root-fixes it by collapsing
      the entity (one asset row, contract_id as a property), removing the duplication at
      source. Archived as superseded before implementation (root-fix is the chosen path).
---

# BUG: classic↔SAC asset duplication

## Summary

A classic credit asset and its Stellar Asset Contract (SAC) are the **same
economic asset**, but the ClickHouse `assets` table stores them as **two rows**
(`asset_type=1 classic_credit` + `asset_type=2 sac`, same `asset_code`+`issuer`).
This double-lists the asset in `/v1/assets` and makes the by-`CODE-ISSUER`
detail resolver **non-deterministic**. Fix: collapse the two on read (prefer the
`contract_id`-bearing row). This is the deferred 0154 "link gap", now concrete.

## Context

### Root cause — CH dropped a PG uniqueness invariant

- **PG:** `uidx_assets_classic_asset (asset_code, issuer_id)` was UNIQUE
  **across asset_type**, so a `(code,issuer)` had exactly one row; learning the
  SAC `contract_id` *upgraded* it `1→2` in place.
- **CH:** `assets` is `ReplacingMergeTree ORDER BY (asset_type, asset_code,
  issuer_id, contract_id)` (`crates/db-clickhouse/schema/init.sql`). `asset_type`
  is in the sort key, so a `type=1` (contract_id=0) and a `type=2` (contract_id
  set) row for the same `(code,issuer)` have **different keys → never merged →
  two rows coexist**.
- Two independent writers, no coordination:
  - `detect_classic_credit_assets` (`crates/xdr-parser/src/state.rs:1003`) writes
    `type=1` for **every** trustline asset, unconditionally (`contract_id: None`).
  - `detect_assets` (deploy) + the SAC-override path write `type=2` with the
    `contract_id`. So any classic asset that also has a SAC gets both rows.

### The system already treats them as one economic asset

`asset_aggregates` is keyed `(asset_code, issuer_id)` with `asset_type IN (1,2)`
(`init.sql`) — supply/holders are a **single shared figure** for the classic and
SAC representations. So the model already knows they are one asset; only `assets`
storage + reads split them.

### Real harm (not just cosmetics)

1. **Double-listing:** `/v1/assets` (read is `FROM assets a FINAL`, no
   `(code,issuer)` collapse, paginates the full 4-tuple — `assets/queries_ch.rs`)
   returns the same economic asset twice: once as id `CODE-ISSUER` (classic),
   once as id `C…` (SAC).
2. **Non-deterministic detail resolver:** the by-code-issuer lookup is
   `… WHERE a.asset_code = ? AND iss.account_id = ? LIMIT 1`
   (`crates/api/src/assets/queries_ch.rs:376`) with no tiebreak across the two
   rows → `/v1/assets/{CODE-ISSUER}` may return the classic row (no `contract_id`)
   OR the SAC row (with `contract_id`) arbitrarily.

### Prod evidence (2026-06-29)

- `asset_type` distribution: `0→1`, `1→316,193`, `2→13,730`, `3→4,073`.
- `type=2` split by deploy: **3,769 deployed / 10,096 un-deployed** (the
  un-deployed bulk comes from the legacy `derive_sac_overrides_from_assets` path
  that 0323 removes). Most of these shadow a `type=1` classic counterpart.

## Design options (ranked by effort)

CH is append-only RMT, so write-time merge is awkward (the two writers are
independent and order-varies).

1. **Read-side collapse (RECOMMENDED).** List + by-code-issuer resolver dedup
   `(asset_code, issuer_id)` preferring the `contract_id`-bearing (SAC) row.
   No schema change, no migration, no enum/DTO/frontend change. Fixes both the
   double-listing and the resolver non-determinism. Cost: a small permanent
   read-side dedup (the asset list is already paginated; collapsing changes the
   page key — handle in the cursor).
2. **Re-key + version supersede.** Restructure the `assets` key so classic+SAC
   dedup to one row (SAC version wins). True single-row storage, but needs a
   migration of the ~13.7k `type=2` rows + write-side version logic + key
   carve-outs for native (code='') and soroban `type=3` (keyed by contract_id).
3. **Full merge + drop `sac` asset_type** (SAC becomes a facet — `contract_id`
   column — of the classic asset, not a separate type). Cleanest model, biggest
   blast radius: Rust enum, API DTO (`asset_type`/`asset_type_name`), the
   frontend "SAC" filter, api-types regen, ADR-0032 docs. Overkill here.

## Implementation Plan (option 1 — read-side collapse)

### Step 1 — list collapse

In `assets/queries_ch.rs` list query, dedup `(asset_code, issuer_id)` for
classic-backed assets (types 1,2), preferring the row with `contract_id != 0`.
Preserve native (type=0) and soroban (type=3) as-is. Keep pagination stable
(the cursor currently keys the full 4-tuple — re-derive a stable post-collapse
key).

### Step 2 — resolver determinism

Make the by-code-issuer resolver (`queries_ch.rs:376`) deterministic: when both
a classic and a SAC row exist for `(code,issuer)`, always return the
`contract_id`-bearing one (add an explicit `ORDER BY (contract_id != 0) DESC`
tiebreak before `LIMIT 1`, or an equivalent prefer-SAC rule).

### Step 3 — regression tests

An asset with both a `type=1` and a `type=2` row for the same `(code,issuer)`:
(a) appears exactly once in the list, as the SAC representation; (b) resolves to
the same (SAC) row via both `/{C…}` and `/{CODE-ISSUER}`.

## Acceptance Criteria

- [ ] `/v1/assets` lists each economic asset once — a `(code,issuer)` with both
      a classic and a SAC row yields a single entry (the `contract_id`-bearing one).
- [ ] By-code-issuer resolver is deterministic: `/v1/assets/{CODE-ISSUER}` always
      returns the SAC row when one exists (never the row-less classic variant).
- [ ] Regression test covers the dual-row case (list collapse + resolver).
- [ ] Native (type=0) and soroban-native (type=3) assets are unaffected.
- [ ] **Docs updated** — update `docs/architecture/database-schema/*` to note the
      assets read-collapse semantics (classic+SAC present one economic asset on
      read). `N/A` only if the read model is not documented there — verify.
- [ ] **API types regenerated** — `N/A` expected: read-only query change, no
      DTO/openapi shape change. Verify no `crates/api/**` DTO change before close.

## Notes

- **Relation to 0323:** read-collapse is consistent with 0323 (un-deployed SAC is
  an asset) and makes its `type=2` rows present cleanly under the classic
  identity. 0323 stays its own scope; if option 2 (re-key) is ever chosen,
  sequence it **after** 0323 (same writers).
- **Relation to 0154:** this is the concrete realization of the deferred
  "XLM ↔ XLM SAC link gap" (0154 README, Out of scope).
- **Sibling (separate task, not here):** un-deployed-SAC **frontend** UX — the
  `Contract ID` link (asset detail + list) pointing at a non-existent contract
  page for un-deployed SACs; guard on `deployed_at_ledger` (+ optional re-derive
  of the `C…` strkey). Different layer, its own dependency — spin separately.
- **Surfaced via** the `/v1/assets/{id}/transactions` quota incident (a sparse
  SAC asset, zkSync, triggering full `operations_appearances` scans) — unrelated
  fix (skip-index) but the same SAC modeling thread surfaced this duplication.
