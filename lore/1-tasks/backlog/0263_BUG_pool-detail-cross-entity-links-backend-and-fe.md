---
id: '0263'
title: 'Pool detail cross-entity links — PoolAssetLeg backend schema extension + FE Link wraps (reserves + Since-ledger)'
type: BUG
status: backlog
related_adr: ['0032']
related_tasks: ['0257', '0077', '0246']
tags:
  [
    'frontend',
    'backend',
    'audit-blocker',
    'priority-high',
    'effort-small',
    'phase-bug',
    'full-stack',
  ]
links:
  - 'Finding F-K-2 + F-K-3: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/K-cross-entity-links.md'
  - 'Finding F-K-9 (NEW): lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/K-cross-entity-links.md — PoolAssetLeg schema gap'
  - 'Audit context: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/triage-gate-B.md'
  - 'Originating tasks: 0077 (LP list + detail), 0246 (backend LP API extensions)'
history:
  - date: '2026-05-25'
    status: backlog
    who: karolkow
    note: 'Spawned from 0257 Gate B (F-K-2 + F-K-3, Class B 🟠 HIGH). Post-research scope-correction: reserve link fix requires backend `PoolAssetLeg` schema extension (F-K-9 NEW finding) since current type lacks linkable identifier for SAC/Soroban/native legs. Merged former 0266 backend schema task into this task per user 2026-05-25 — single full-stack feature, atomic PR, one OpenAPI regen.'
---

# Pool detail cross-entity links — backend schema + FE consume

## Summary

Pool detail page (`/liquidity-pools/:id`) breaks the cross-entity
navigation invariant in two places:

1. **Reserve asset labels** (USDCOIN/EUR/native) in pool summary render
   as **plain text**. Users cannot navigate from a pool's reserve display
   to the asset detail page (F-K-2).
2. **Participants table "Since ledger" column** renders ledger sequence
   as **plain number** instead of `<Link to={routes.ledger(seq)}>`
   (F-K-3).

Plus a NEW finding surfaced during 0263 correctness research (F-K-9):
`PoolAssetLeg` response shape (`libs/api-types/src/generated/types.gen.ts:1155-1166`)
**lacks a linkable asset identifier** (no `id`, no `contract_id` —
only `asset_code`/`asset_type`/`asset_type_name`/`issuer`). Asset endpoint
accepts numeric `assets.id` OR contract `C...` strkey OR `code-issuer`
composite — none of these can be reliably constructed from `PoolAssetLeg`
for all leg types:

- **Classic credit**: `code` + `issuer` → `code-issuer` composite works ✓
- **Native (XLM)**: no `issuer` → can't form `code-issuer`, no `id` either ✗
- **SAC / Soroban contract token**: needs `contract_id` (C-strkey) →
  field missing in `PoolAssetLeg` ✗

This task ships the full fix as a single full-stack feature:

- **Backend**: extend `PoolAssetLeg` to include linkable identifier
- **FE**: wrap reserve labels in `<RouterLink>`, wrap Since-ledger in `<RouterLink>`

## Status: Backlog

**Audit-blocker for task 0257 (FE comprehensive audit).** Must land
before Wave 6 (Track 2 visual + UX). Without fix:

- Wave 6 2.0 Playwright re-walks pool detail cross-entity links →
  re-reports the same broken-link findings
- Wave 6 2.5 a11y flags non-link text styled as link
- Pre-launch UX: user clicks "USDCOIN" in pool reserve display → nothing
  happens. Frustration.

Cascade compression: ~5-8 duplicate Wave 6 Track 2 findings avoided.

## Context

- Audit findings: `F-K-2` + `F-K-3` in
  [K-cross-entity-links.md](../active/0257_RESEARCH_frontend-comprehensive-audit/findings/K-cross-entity-links.md)
- NEW finding from 0263 correctness research: `F-K-9` (PoolAssetLeg
  schema gap) — added to same findings file post-Wave-5
- Live evidence: Wave 3 Playwright session 2026-05-25 — inspected DOM
  on `/liquidity-pools/<known-id>`, confirmed plain `<span>`/`<td>`
  renders for both targets
- Helper names per `web/src/router/routes.ts:8,13`:
  `routes.asset(id)` + `routes.ledger(seq)`
- PoolAssetLeg schema confirmed at `libs/api-types/src/generated/types.gen.ts:1155-1166`

## Implementation Plan

### Phase 1 — Backend: extend PoolAssetLeg schema

**File:** `crates/api/src/openapi/schemas.rs` or wherever `PoolAssetLeg`
is defined (utoipa derive site)

Add linkable identifier field. Two options:

**Option A — `asset_id: i64` (preferred if backend stores asset row ID per leg):**

```rust
pub struct PoolAssetLeg {
    pub asset_id: Option<i64>,   // NEW — numeric assets.id for direct lookup
    pub asset_code: String,
    pub asset_type: String,
    pub asset_type_name: String,
    pub issuer: Option<String>,
}
```

**Option B — `contract_id: Option<String>` (canonical for SAC/Soroban):**

```rust
pub struct PoolAssetLeg {
    pub contract_id: Option<String>,  // NEW — C-strkey for SAC/Soroban tokens, None for native+classic
    pub asset_code: String,
    pub asset_type: String,
    pub asset_type_name: String,
    pub issuer: Option<String>,
}
```

**Option C — both** (flexible, more wire bytes but maximum routability):

```rust
pub struct PoolAssetLeg {
    pub asset_id: Option<i64>,
    pub contract_id: Option<String>,
    pub asset_code: String,
    pub asset_type: String,
    pub asset_type_name: String,
    pub issuer: Option<String>,
}
```

Pick based on what backend can supply efficiently (lookup cost in pool
query). Option A simplest if asset row already JOINed. Option C most
future-proof.

### Phase 2 — Backend: populate field in pool handlers

**Files:**

- `crates/api/src/liquidity_pools/queries.rs` — JOIN `pools.asset_a_id`/`asset_b_id` → `assets` table (or similar — verify schema)
- `crates/api/src/liquidity_pools/handlers.rs` — `map_pool_item` or response builder populates the new field

### Phase 3 — Regen API types

```bash
npx nx run @rumblefish/api-types:generate
```

`libs/api-types/src/openapi.json` + `libs/api-types/src/generated/types.gen.ts`
will pick up new field. Commit alongside backend changes.

### Phase 4 — FE: reserve labels Link wrap

**File:** `web/src/pages/pool-detail/PoolSummary.tsx:87,97` (and
`AssetReserveCell` at lines 19-38)

Pattern (assuming Option A):

```tsx
// Before
<Typography>{leg.asset_code}</Typography>;

// After
{
  leg.asset_id != null ? (
    <RouterLink to={routes.asset(String(leg.asset_id))}>
      {leg.asset_code}
    </RouterLink>
  ) : (
    // Fallback for native or rows without ID (shouldn't happen after Phase 2)
    <Typography>{leg.asset_code}</Typography>
  );
}
```

Match link styling to other cross-entity link patterns in the codebase
(grep `RouterLink` in `web/src/pages/` for reference).

### Phase 5 — FE: Since-ledger Link wrap

**File:** `web/src/pages/pool-detail/PoolParticipants.tsx:57-61`

```tsx
// Before
<Typography>{formatAmount(row.first_deposit_ledger)}</Typography>

// After
<RouterLink to={routes.ledger(row.first_deposit_ledger)}>
  {formatAmount(row.first_deposit_ledger)}
</RouterLink>
```

`row.first_deposit_ledger` is `number` per current type; `routes.ledger`
takes `number | string` — pass as-is.

### Phase 6 — Verify

- Navigate `/liquidity-pools/<pool-id>` (use known pool with classic +
  native legs, e.g. via `curl http://localhost:9000/v1/liquidity-pools?limit=10` to find one)
- Hover reserve labels — pointer cursor + asset URL shown in browser
  status bar
- Click reserve label → navigates to `/assets/<id>` (or 404 placeholder
  if asset detail not yet implemented for that ID class)
- Hover Since-ledger — pointer + ledger URL
- Click Since-ledger → `/ledgers/<seq>`
- Visual styling matches other cross-entity link patterns
- Backend `cargo test -p api` includes new field in pool response
  golden fixture

### Phase 7 — Regression test

Add Playwright assertion (gated on 0226): pool detail reserve label has
`href="/assets/..."`, participants Since-ledger has `href="/ledgers/..."`.

## Acceptance Criteria

### Backend

- [ ] `PoolAssetLeg` extended with linkable identifier (Option A / B / C
      per team preference; documented in commit)
- [ ] `crates/api/src/liquidity_pools/queries.rs` populates new field
- [ ] `cargo test -p api -- liquidity_pools` includes assertion for new field
- [ ] OpenAPI regen committed (`libs/api-types/src/openapi.json` +
      `libs/api-types/src/generated/types.gen.ts`)

### FE

- [ ] `PoolSummary.tsx` (and `AssetReserveCell`) wraps reserve labels in
      `<RouterLink to={routes.asset(...)}>` with appropriate fallback
- [ ] `PoolParticipants.tsx` wraps Since-ledger cell in
      `<RouterLink to={routes.ledger(...)}>`
- [ ] Hover on either shows pointer cursor + URL in status bar
- [ ] Click navigates to correct detail page
- [ ] Visual styling matches other cross-entity link patterns

### Audit

- [ ] Audit branch `research/0257_frontend-comprehensive-audit` rebased onto develop post-merge
- [ ] Finding `F-K-2` + `F-K-3` in `K-cross-entity-links.md` marked `RESOLVED in <SHA>`
- [ ] Finding `F-K-9` (PoolAssetLeg schema gap) marked `RESOLVED in <SHA>`

### Docs

- [ ] **Docs updated** — `docs/architecture/api/<liquidity-pools>.md`
      (if exists) reflects new `PoolAssetLeg` field. Per ADR 0032. If no
      relevant doc exists, mark `N/A — backend schema extension matches
ADR 0032 evergreen docs gate trigger; doc to be added in Phase 3 batch task XXXX_DOCS_evergreen-architecture-sync per audit Wave 5 F-A-3`.
- [ ] **API types regenerated** — handled in Phase 3 (backend) above.

## Notes

- Effort: ~30min backend (schema extend + populate + test) + ~10min FE
  (Link wrap × 2) + ~5min OpenAPI regen = **~45min total**.
- Full-stack atomic PR: backend + FE + regen in same diff for reviewer
  efficiency.
- Native XLM (no issuer, no contract_id, may not have row in `assets`
  table) handling: confirm with backend dev whether native is row in
  `assets` (likely yes per Stellar protocol) — if yes, `asset_id` works
  for native too.
- Pairs with 0262 + 0264 — same pool detail surface. Single feature
  branch with sub-commits OR separate PRs per task; reviewer call.
- Original tasks 0263 + 0266 merged into this task per user 2026-05-25
  to avoid spawn-fragmentation when root cause is shared.
