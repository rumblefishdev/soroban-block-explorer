---
id: '0263'
title: 'Pool detail cross-entity links — PoolAssetLeg backend schema extension + FE Link wraps (reserves + Since-ledger)'
type: BUG
status: completed
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
  - date: '2026-05-26'
    status: active
    who: karolkow
    note: 'Activated as part of Gate B fix-first batch (0262/0263/0264 + 0265 off-band CVE) on shared branch.'
  - 'Finding F-K-9 (NEW): lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/K-cross-entity-links.md — PoolAssetLeg schema gap'
  - 'Audit context: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/triage-gate-B.md'
  - 'Originating tasks: 0077 (LP list + detail), 0246 (backend LP API extensions)'
history:
  - date: '2026-05-25'
    status: backlog
    who: karolkow
    note: 'Spawned from 0257 Gate B (F-K-2 + F-K-3, Class B 🟠 HIGH). Post-research scope-correction: reserve link fix requires backend `PoolAssetLeg` schema extension (F-K-9 NEW finding) since current type lacks linkable identifier for SAC/Soroban/native legs. Merged former 0266 backend schema task into this task per user 2026-05-25 — single full-stack feature, atomic PR, one OpenAPI regen.'
  - date: '2026-05-26'
    status: completed
    who: karolkow
    note: 'Implemented across 473de2a2 (backend + initial FE) + a5f15166 (FE scope expand to KpiStrip + PoolsTable per post-sweep F-K-2 correction). Backend PoolAssetLeg + contract_id field landed; 3 unit tests; OpenAPI regen. FE legHref helper hoisted to pool-detail/helpers.ts; 3 reserve-label sites + Since-ledger all Link-wrapped. Manual UI verification via Playwright MCP against local stack: USDC → /assets/<C-strkey>; XLM no link; classic credit → /assets/CODE-ISSUER composite; ledger 1006/1015 → /ledgers/X. Click navigation end-to-end works (USDC reserve → Asset detail renders). F-K-2 + F-K-3 + F-K-9 all RESOLVED.'
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

### FE — Reserve labels (3 sites, post-sweep scope correction)

- [x] `PoolSummary.tsx` (AssetReserveCell) wraps reserve labels in
      `<RouterLink to={routes.asset(...)}>` via legHref precedence
- [x] `PoolKpiStrip.tsx` wraps per-leg KPI subtitle in RouterLink (NEW
      per post-sweep — commit `a5f15166`)
- [x] `PoolsTable.tsx` (list page) wraps reserve column asset codes in
      RouterLink (NEW per post-sweep — commit `a5f15166`)
- [x] `PoolParticipants.tsx` wraps Since-ledger cell in
      `<RouterLink to={routes.ledger(...)}>`
- [x] Hover shows pointer cursor + URL in status bar (subtle styling:
      `color: inherit`, `textDecoration: none`, underline-on-hover)
- [x] Click navigates to correct detail page (verified end-to-end:
      USDC reserve → `/assets/CUSDCSAC...` renders Asset detail)
- [x] Visual styling matches — link styling matches plain text until
      hover, no visual clutter despite 3-site density

### Audit

- [ ] Audit branch `research/0257_frontend-comprehensive-audit` rebased onto develop (post-merge)
- [ ] Finding `F-K-2` + `F-K-3` in `K-cross-entity-links.md` marked `RESOLVED in <SHA>` (post-merge)
- [ ] Finding `F-K-9` (PoolAssetLeg schema gap) marked `RESOLVED in <SHA>` (post-merge)

### Docs

- [x] **Docs updated** — `N/A — schema field add not material to architecture docs scope; routes already documented under 0264 `docs/architecture/api/url-conventions.md`. Per ADR 0032.
- [x] **API types regenerated** — `libs/api-types/src/{openapi.json,generated/}` regen committed in `473de2a2`.

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

## Implementation Notes

Landed across commits `473de2a2` (Gate B batch) + `a5f15166` (scope-correction expand).

**Backend** (`473de2a2`):

- `crates/api/src/liquidity_pools/dto.rs` — `PoolAssetLeg` extended with `contract_id: Option<String>` (SAC mirror C-strkey). Field optional, omitted from wire when None.
- `crates/api/src/liquidity_pools/queries.rs` — `PoolRow` extended with `asset_a_contract_id` + `asset_b_contract_id`. `fetch_pool_list` and `fetch_pool_by_id` SQLs `LEFT JOIN assets … AND asset_type = 2 → soroban_contracts` per leg to surface the SAC mirror C-strkey. NULL when no mirror exists.
- `crates/api/src/liquidity_pools/handlers.rs::map_pool_item` — populates `contract_id` from row.
- 3 new unit tests (`map_pool_item_tests`): native leg → no contract_id; classic credit without mirror → no contract_id; SAC mirror → contract_id propagates.

**Frontend** (`473de2a2` + `a5f15166`):

- `web/src/pages/pool-detail/helpers.ts` — added `legHref()` helper exporting the precedence: native (asset_type 0) → no link; `contract_id` → `routes.asset(contract_id)`; `asset_code + issuer` → `routes.asset(${code}-${issuer})`; else → no link.
- `web/src/pages/pool-detail/PoolSummary.tsx` (AssetReserveCell) — wraps leg code in `<Link component={RouterLink}>` when `legHref(leg)` resolves.
- `web/src/pages/pool-detail/PoolKpiStrip.tsx` — `assetSubtitle()` helper renders per-leg KPI subtitle as either plain code (native / no mirror) or RouterLink to asset detail. (`a5f15166`)
- `web/src/pages/liquidity-pools/PoolsTable.tsx` — `assetCodeNode()` helper renders reserve column code as either plain text or RouterLink. (`a5f15166`)
- `web/src/pages/pool-detail/PoolParticipants.tsx` — Since-ledger cell wrapped in `<Link component={RouterLink} to={routes.ledger(...)}>`.

**OpenAPI regen**: `libs/api-types/src/{openapi.json,generated/types.gen.ts}` updated with new `contract_id` field. Field appears as `contract_id?: string | null` in generated TypeScript.

**Manual verification** (Playwright MCP against local stack):

| Site             | Element                             | Verified                               |
| ---------------- | ----------------------------------- | -------------------------------------- |
| PoolSummary      | USDC reserve (classic + SAC mirror) | → `/assets/CUSDCSAC...` ✅             |
| PoolSummary      | XLM reserve (native)                | no link ✅                             |
| PoolKpiStrip     | USDC kpi subtitle                   | → `/assets/CUSDCSAC...` ✅             |
| PoolKpiStrip     | XLM kpi subtitle                    | no link ✅                             |
| PoolsTable       | USDCOIN, EUR (classic)              | → `/assets/CODE-ISSUER` composite ✅   |
| PoolsTable       | USDC, EUR (SAC)                     | → `/assets/<C-strkey>` ✅              |
| PoolsTable       | XLM                                 | no link ✅                             |
| PoolParticipants | Since-ledger 1006/1015              | → `/ledgers/1006`, `/ledgers/1015` ✅  |
| Click navigation | USDC reserve → Asset detail         | Asset page renders with full detail ✅ |

## Issues Encountered

- **Scope wider than original Wave 3 finding (F-K-2)**: original finding cited only PoolSummary. Post-sweep audit (by sister review session 2026-05-26) surfaced 2 additional sites with the same root cause — PoolKpiStrip and PoolsTable. Both render reserve asset codes as plain text. Implementation initially closed F-K-2 against PoolSummary only; expand commit `a5f15166` extended Link wraps to all 3 sites + hoisted `legHref()` to shared `helpers.ts` for DRY.

- **Routing decision drift (intermediate)**: first FE pass routed reserves to `/contracts/...` (SAC) or `/accounts/...` (issuer) instead of `/assets/...`. Caught during senior review — task body explicitly says `routes.asset(...)`. Corrected to route all leg targets through `parse_asset_id` (polymorphic: numeric / C-strkey / `code-issuer` composite). Single `/assets/...` destination keeps the "click the asset code, see asset detail" mental model intact.

- **Native XLM unlinked — intentional**: Stellar native has no on-chain address in the classic protocol, and the SAC mirror for XLM is network-specific (`CAS3...` on mainnet, different on testnet). Backend `parse_asset_id` does not accept a `native` alias. Decision: leave native unlinked (plain text). Solscan-style WSOL routing doesn't transfer cleanly — Stellar.expert also renders native XLM as plain text in many surfaces. If desired later, add a `routes.asset('native')` alias in backend; out of scope here.

## Design Decisions

### From Plan

1. **Option B `contract_id: Option<String>` over Option A (`asset_id i64`)**: task body listed three schema options. Picked B because SAC mirror's C-strkey is the **identifier that the FE needs to build the link**; routing via numeric surrogate `asset_id` would require an extra round-trip through `/assets/:numeric` → resolve → display. C-strkey form lands directly in `/assets/${contract_id}` (polymorphic `parse_asset_id` accepts C). One backend column, one wire field, no FE conversion.

2. **Single full-stack commit, not split backend/FE PRs**: cohesive scope, OpenAPI regen needs both sides, reviewer can validate end-to-end in one diff. Lands with the broader Gate B batch in `473de2a2`.

3. **Route reserve labels to `/assets/...`, not `/contracts/...` or `/accounts/...`**: per task body Phase 4 code snippet. Even when `contract_id` is the SAC mirror address, the natural target for "click the asset code on a reserve cell" is the asset detail page (where supply, holders, transactions live), not the contract code page. `parse_asset_id` accepts C-strkey as one of three polymorphic forms — same destination row.

### Emerged

4. **`legHref()` hoisted from PoolSummary to `helpers.ts`** (commit `a5f15166`): originally inlined in PoolSummary. Post-sweep, scope expanded to 3 callsites (Summary + KpiStrip + Table) — duplicating the precedence logic in three files would drift. Moved to shared helper exported alongside `assetLegLabel` + `formatCompactAmount`. Three callsites import from one source.

5. **Subtle link styling — `color: inherit`, no default underline, underline-on-hover**: 3-site density risk = visual noise (blue links everywhere). Chose to make links visually identical to plain text until hover, with cursor pointer + status-bar URL providing affordance. Matches the "click any occurrence to navigate" UX of Stellar.expert without the "blue underline everywhere" aesthetic.

6. **Native XLM left unlinked across all 3 sites** (see Issues §3). Senior call after considering Solscan WSOL analogy.

7. **PoolSummary trailing-space bugfix**: original implementation rendered `${formatAmount(amount)} ` (template-literal trailing space) inside `<Typography>` before splitting amount and code into separate flex children. Stack `spacing={1}` provides the 8px flex gap — trailing space was dead code and would have visually collapsed. Removed.

## Future Work

None for the reserve-label / since-ledger scope. Architectural follow-up (out of scope here, spawn separately if undertaken): refactor `IdentifierDisplay` in `libs/ui` to accept `href` as a prop, removing the duplicated route table in `libs/ui/src/identifiers/routes.ts`. Single source of truth for URL conventions in `web/src/router/routes.ts`. Senior call during this batch's review — kept as future task to avoid blowing scope.
