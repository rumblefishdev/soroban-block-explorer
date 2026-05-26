---
id: '0264'
title: 'Strkey canonical everywhere — strkey-only (no legacy hex compat) + per-endpoint sweep (closes F-L-1 + F-K-4 + F-AN-8)'
type: REFACTOR
status: backlog
related_adr: ['0008', '0032']
related_tasks: ['0257', '0060', '0077']
tags:
  [
    'frontend',
    'backend',
    'audit-blocker',
    'priority-high',
    'effort-medium',
    'phase-bug',
    'cross-cutting',
    'pre-launch',
  ]
links:
  - 'Finding F-L-1: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/L-search-functional.md (search pool strkey 0 results)'
  - 'Finding F-K-4: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/K-cross-entity-links.md (empty-state hint omits L...)'
  - 'Finding F-AN-8: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/AN-stellar-domain.md (cross-cutting strkey convention)'
  - 'Audit context: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/triage-gate-B.md'
  - 'Originating tasks: 0060 (search), 0077 (LP detail), CAP-38 (Strkey RFC)'
history:
  - date: '2026-05-25'
    status: backlog
    who: karolkow
    note: 'Spawned from 0257 Gate B (F-L-1 + F-K-4, Class B 🟠 HIGH) and post-Gate-B research (F-AN-8 cross-cutting Stellar convention). Audit-blocker: must land before Wave 6 — closes 3 audit findings + aligns project with Stellar ecosystem strkey canonical convention. Scope widened per user senior calls 2026-05-25: (1) strkey-only (no legacy hex backwards-compat — project is pre-deploy, no existing user bookmarks to preserve); (2) per-endpoint sweep formal verification + evergreen docs.'
---

# Strkey canonical everywhere — strkey-only + per-endpoint sweep

## Summary

Backend `/v1/liquidity-pools/:id` currently accepts hex 64-char lowercase
only, breaking Stellar ecosystem convention (strkey `L...` is canonical
per CAP-38). FE displays strkey but uses hex in URL bar and search.

**Pre-launch — no legacy compat needed. Project is pre-deploy; nobody has
hex bookmarks. Going strkey-only everywhere.** Backend rejects hex input
with informative error citing strkey requirement.

This task ships canonical strkey across the full surface:
- Pool endpoint accepts strkey only; internal DB lookup uses hex
- Backend search classifier dispatches `L...` to pool lookup
- FE URLs canonical = strkey
- Per-endpoint sweep documents accepted format for every entity (G/C/L/numeric/hex-hash/polymorphic)
- Evergreen doc `docs/architecture/api/url-conventions.md` codifies convention

## Status: Backlog

**Audit-blocker for task 0257 (FE comprehensive audit).** Must land
before Wave 6 (Track 2 visual + UX). Closes:
- F-L-1 🟠 (search pool strkey returns 0 results)
- F-K-4 🟠 (empty-state hint omits L from supported formats)
- F-AN-8 🟠 (cross-cutting strkey convention drift)

Cascade compression: Wave 6 2.0 Playwright + 2.1 Figma + 2.5 a11y clean
for pool routes; ~5-10 duplicate Wave 6 findings avoided. Evergreen doc
prevents future drift.

## Context

**CAP-38 / Stellar SDK strkey convention:**

| Entity | Canonical strkey prefix | Internal/wire form |
|---|---|---|
| Account | `G...` | 32-byte ed25519 pubkey |
| Contract | `C...` | 32-byte contract ID |
| **Liquidity pool** | **`L...`** | 32-byte SHA-256 hash |
| Muxed account | `M...` | composite |
| Pre-auth tx | `T...` | 32-byte hash |

Stellar Expert + Horizon + Stellar Lab + Soroban CLI all use strkey
canonical. Hex = internal storage/wire detail.

**Current state in our project (per audit research 2026-05-25):**

| Endpoint | Accepts | Industry std | Verdict |
|---|---|---|---|
| `/v1/accounts/:id` | strkey `G...` (`path::strkey(_, 'G', _)`) | strkey | ✓ canonical |
| `/v1/contracts/:id` | strkey `C...` (`path::strkey(_, 'C', _)`) | strkey | ✓ canonical |
| `/v1/transactions/:hash` | hex hash | hex | ✓ (tx hash always hex in protocol) |
| `/v1/ledgers/:seq` | numeric | numeric | ✓ |
| `/v1/assets/:id` | polymorphic (numeric ID OR C-strkey OR `code-issuer`) | mixed | ⚠ polymorphic by design |
| **`/v1/liquidity-pools/:id`** | **hex 64-lower ONLY** | **strkey L...** | **❌ outlier — this task fixes** |
| `/v1/nfts/:id` | `parse_nft_id` (TBD — verified in Phase 8) | strkey expected | ? verify |

**FE inconsistency** (will be fixed by this task):
```
PoolsTable.tsx:  const strkey = poolIdHexToStrkey(row.pool_id);
                 href={routes.pool(row.pool_id)}                   // hex URL today
PoolSummary.tsx: value={poolIdHexToStrkey(pool.pool_id)}           // display strkey
                 href={routes.pool(pool.pool_id)}                  // hex URL today
```

## Implementation Plan

### Phase 1 — Backend: pool ID validator (strkey-only)

**File:** `crates/api/src/common/path.rs`

Replace existing `pool_id_hex` with strkey-only validator. Returns hex
for internal DB lookup:

```rust
/// Validates that `value` is a CAP-38 strkey starting with `L`
/// (~56 chars, base32 with checksum). Returns the canonical hex form
/// (32-byte SHA-256 hash, lowercase) for internal DB lookup.
///
/// Symmetric with `path::strkey(_, 'G' | 'C', _)` for accounts and
/// contracts.
///
/// Pre-launch project: no legacy hex acceptance.
pub fn pool_id_strkey(value: &str, param: &str) -> Result<String, Response> {
    use stellar_strkey::Strkey;
    match Strkey::from_string(value) {
        Ok(Strkey::LiquidityPool(pool)) => Ok(hex::encode(pool.0)),
        Ok(_) => Err(errors::bad_request_with_details(
            errors::INVALID_POOL_ID,
            "pool_id must be a CAP-38 strkey starting with 'L'",
            serde_json::json!({ "param": param, "received": value, "got_type": "non-pool strkey" }),
        )),
        Err(_) => Err(errors::bad_request_with_details(
            errors::INVALID_POOL_ID,
            "pool_id must be a CAP-38 strkey starting with 'L' (e.g. 'L<base32-encoded-pool-hash>'). Hex pool IDs are no longer accepted.",
            serde_json::json!({ "param": param, "received": value }),
        )),
    }
}
```

Drop or deprecate `pool_id_hex` (callers all migrated in Phase 2).

### Phase 2 — Backend: handler boundary conversion (4 handlers)

**File:** `crates/api/src/liquidity_pools/handlers.rs`

For each: `get_pool`, `get_pool_chart`, `list_pool_transactions`,
`list_pool_participants`:

```rust
// Before
pub async fn get_pool(State(state): State<AppState>, Path(pool_id): Path<String>) -> Response {
    if let Err(resp) = path::pool_id_hex(&pool_id, "pool_id") {
        return resp;
    }
    let row = fetch_pool_by_id(&state.db, &pool_id).await;
    ...
}

// After
pub async fn get_pool(State(state): State<AppState>, Path(pool_id_strkey): Path<String>) -> Response {
    let pool_id_hex = match path::pool_id_strkey(&pool_id_strkey, "pool_id") {
        Ok(hex) => hex,
        Err(resp) => return resp,
    };
    let row = fetch_pool_by_id(&state.db, &pool_id_hex).await;  // internal = hex
    ...
}
```

Storage stays hex (efficient, raw bytes). Boundary converts.

### Phase 3 — Backend: search classifier `L...` prefix

**File:** `crates/api/src/search/queries.rs` (or wherever input
classification lives — find by grep `G` prefix detection)

Add symmetric case:
```rust
// Find existing G/C detection; add L
if let Ok(Strkey::LiquidityPool(pool)) = Strkey::from_string(input) {
    return SearchClassification::Pool { hex: hex::encode(pool.0), strkey: input.to_string() };
}
```

Backend response returns canonical strkey form in redirect hits and
matched rows.

### Phase 4 — Backend: response shape — pool_id field

**File:** `crates/api/src/liquidity_pools/handlers.rs` + response builders

Currently `LiquidityPoolItem.pool_id` (and similar response fields)
return hex. Change to strkey:

```rust
// In map_pool_item or response builder
LiquidityPoolItem {
    pool_id: hex_to_strkey(row.pool_id_hex),  // wire form = strkey
    ...
}
```

OR keep internal hex but add `pool_strkey` alongside `pool_id` — depends
on team preference. Recommended: rename wire field to strkey form, drop
hex from wire entirely (consumers should never need hex client-side).

Update OpenAPI schemas (utoipa derive macros) accordingly.

### Phase 5 — FE: URL builder canonical = strkey

**File:** `web/src/router/routes.ts`

```ts
// Before
pool: (id: string) => `/liquidity-pools/${encodeURIComponent(id)}`,

// After — explicit: takes strkey only
pool: (strkey: string) => `/liquidity-pools/${encodeURIComponent(strkey)}`,
```

Type signature change documents intent.

### Phase 6 — FE: all `routes.pool(...)` callsites pass strkey

Grep all callers:
- `web/src/pages/liquidity-pools/PoolsTable.tsx` — wire `href={routes.pool(<strkey>)}`. If API returns hex `pool_id`, convert at call site (or fix API per Phase 4 to return strkey directly).
- `web/src/pages/pool-detail/PoolSummary.tsx` — same.
- Any other `routes.pool(` call site in `web/src/`.

If Phase 4 changes wire shape to strkey, no client conversion needed —
callsites pass `pool.pool_id` (now strkey) directly. Cleaner.

### Phase 7 — FE: useParams strkey-only

**File:** `web/src/pages/LiquidityPoolDetailPage.tsx`

```ts
const { id: poolStrkey } = useParams<{ id: string }>();
// Validate up-front; pre-launch no hex acceptance
if (!isValidStrkey(poolStrkey, 'L')) {
  return <NotFoundState />;
}
// Pass strkey to TanStack hooks; cache keys = strkey (consistent across app)
```

Update `isValidIdentifier(pool, value)` in
`libs/ui/src/identifiers/validators.ts` to check strkey, not hex:

```ts
// Before
export function isPoolId(value: string): boolean {
  return HEX_64_LOWER.test(value.toLowerCase());
}

// After
export function isPoolId(value: string): boolean {
  return /^L[A-Z2-7]{55}$/.test(value);  // CAP-38 L-strkey, ~56 chars total
  // (or use stellar-strkey JS package isValidStrkey('L', value))
}
```

Drop `poolIdStrkeyToHex` / `poolIdHexToStrkey` from client surface if
backend Phase 4 returns strkey wire form (utils only needed if hex
appears anywhere FE side, which after this task it shouldn't).

### Phase 8 — Per-endpoint formal sweep

For each backend endpoint with an `:id` / `:hash` path param, verify
canonical form acceptance:

**File:** `crates/api/src/{accounts,assets,contracts,nfts,transactions,ledgers,liquidity_pools}/handlers.rs`

| Endpoint | Verify | If outlier → fix in this task | Document in url-conventions.md |
|---|---|---|---|
| `/v1/accounts/:id` | `path::strkey(_, 'G', _)` accepts G-strkey only | already ✓ | ✓ strkey G |
| `/v1/contracts/:id` | `path::strkey(_, 'C', _)` accepts C-strkey only | already ✓ | ✓ strkey C |
| `/v1/liquidity-pools/:id` | `path::pool_id_strkey` (this task) | fixed in Phase 1-2 | ✓ strkey L |
| `/v1/nfts/:id` | read `parse_nft_id` — accepts C-strkey for SAC contract addresses? | if hex-only → fix to strkey | ✓ strkey C (SAC) |
| `/v1/assets/:id` | polymorphic: numeric ID / C-strkey / `code-issuer` | OK (Wave 5 1.2 accepted polymorphism) | ⚠ polymorphic — document each form |
| `/v1/transactions/:hash` | `path::parse_hash` accepts hex 64-lower | OK (tx hash = hex per Stellar protocol) | ✓ hex 64-lower (tx hash is bytes, not strkey-eligible) |
| `/v1/ledgers/:seq` | numeric | OK (ledger seq is u32, no strkey) | ✓ numeric u32 |

If NFT (Phase 8b) found hex-only → add to scope; convert to strkey
acceptance same pattern as pool.

If asset polymorphic acceptance reveals C-strkey path is buggy → add
to scope.

Otherwise: pure documentation phase for already-correct endpoints.

### Phase 9 — Backend: search empty-state response includes L

**File:** `crates/api/src/search/handlers.rs`

If search returns an empty-state-context hint, ensure response payload
exposes all valid input prefixes including `L...`. (If FE owns the hint
list, see Phase 11.)

### Phase 10 — FE: search empty-state hint update

**File:** `web/src/search/SearchResultsView.tsx` (or wherever empty
state hint lives — find by grep "supported" or "G..." in JSX)

Add `L...` (liquidity pool) to supported formats list alongside `G...`
(account), `C...` (contract), hash prefixes for transactions/ledgers.

### Phase 11 — Evergreen doc

**File:** `docs/architecture/api/url-conventions.md` (CREATE)

Markdown doc codifying convention:

```markdown
# API URL Conventions

Per CAP-38, all Stellar/Soroban entity identifiers in user-facing
contexts use **strkey canonical form**. Hex / numeric / polymorphic
forms are documented exceptions where the entity type is not
strkey-eligible by protocol.

## Per-endpoint path parameter formats

| Endpoint | Path param | Form | Validator | Rationale |
|---|---|---|---|---|
| `/v1/accounts/:id` | account ID | strkey `G...` | `path::strkey('G')` | CAP-38 canonical |
| `/v1/contracts/:id` | contract ID | strkey `C...` | `path::strkey('C')` | CAP-38 canonical |
| `/v1/liquidity-pools/:id` | pool ID | strkey `L...` | `path::pool_id_strkey` | CAP-38 canonical |
| `/v1/nfts/:id` | NFT ID (SAC contract) | strkey `C...` | `parse_nft_id` | NFTs are SAC contracts |
| `/v1/assets/:id` | asset ID | polymorphic | `parse_asset_id` | numeric `assets.id` (internal) OR strkey `C...` (SAC) OR `code-issuer` composite (classic). Documented as exception. |
| `/v1/transactions/:hash` | tx hash | hex 64-lower | `path::parse_hash` | Tx hash is raw bytes per Stellar protocol; no strkey form exists. |
| `/v1/ledgers/:seq` | ledger sequence | numeric u32 | direct parse | Ledger seq is a counter, not an identifier. |

## FE URL builder conventions

`web/src/router/routes.ts` — every URL builder takes the canonical form
of its target entity. No hex inputs except for transaction hashes.

## Cross-entity link integrity

Every clickable identifier in the UI MUST link to its detail page using
the canonical form. See task 0257 audit Wave 3 1.7 cross-entity link
integrity for the audit baseline.

## Why this matters

Stellar Expert, Horizon, Stellar Lab, Soroban CLI, and the broader
Stellar SDK ecosystem use strkey for all human-facing identifiers.
External user paste-from-explorer scenarios must work without manual
conversion. Project alignment with ecosystem convention reduces
onboarding friction and avoids surprising users.
```

Add to ADR-0032 evergreen docs gate scope.

### Phase 12 — Verify

- Backend test: `cargo test -p api -- liquidity_pools` — add cases:
  - strkey `L...` valid → 200 OK
  - hex 64-lower → 400 with informative error citing strkey requirement
  - garbage → 400
- Backend search test: paste `L...` → resolves pool
- FE manual: paste strkey from stellar.expert URL into search → finds pool
- FE manual: navigate `/liquidity-pools/L<strkey>` → renders pool detail
- FE manual: every cross-entity link from pool detail (asset reserves,
  participants Since-ledger) uses canonical form (depends on task 0263
  if landed in parallel)
- OpenAPI regen: `npx nx run @rumblefish/api-types:generate` → diff
  shows `pool_id` field shape changed from hex string → strkey string;
  commit regen artifacts in same commit
- `nx typecheck` + `nx lint` green

## Acceptance Criteria

- [ ] `crates/api/src/common/path.rs` exposes `pool_id_strkey` validator
      (strkey-only, returns hex internal)
- [ ] All 4 pool handlers (`get_pool`, `get_pool_chart`,
      `list_pool_transactions`, `list_pool_participants`) use the new validator
- [ ] Backend search classifier dispatches `L...` strkey input to pool lookup
- [ ] Backend wire response `pool_id` field returns strkey form
- [ ] FE `routes.pool(...)` callers pass strkey
- [ ] FE `isPoolId` validator updated to check strkey (not hex)
- [ ] `LiquidityPoolDetailPage.tsx` `useParams` consumes strkey
- [ ] FE search empty-state hint lists `L...` alongside `G...` and `C...`
- [ ] **Per-endpoint sweep:** every endpoint with an `:id` / `:hash`
      param verified canonical (or documented exception); NFT endpoint
      verified strkey-compatible (Phase 8) — fix if hex-only found
- [ ] `docs/architecture/api/url-conventions.md` created with full
      per-endpoint table + rationale + ADR-0032 cross-link
- [ ] `cargo test -p api` regression cases for strkey accept + hex
      reject + garbage reject
- [ ] FE manual paste-strkey-into-search test passes
- [ ] Audit branch `research/0257_frontend-comprehensive-audit` rebased
      onto develop post-merge
- [ ] Findings `F-L-1` + `F-K-4` + `F-AN-8` marked `RESOLVED in <SHA>`
- [ ] **Docs updated** — `docs/architecture/api/url-conventions.md`
      created (Phase 11) + `docs/architecture/frontend/frontend-overview.md`
      cross-link added. Per ADR 0032.
- [ ] **API types regenerated** — `crates/api/**` changed (validator +
      handlers + response shape); run
      `npx nx run @rumblefish/api-types:generate` and commit
      `libs/api-types/src/openapi.json` + `libs/api-types/src/generated/*`
      in same commit.

## Notes

- Effort: ~3-5h backend (validator + 4 handlers + response shape + search
  classifier + per-endpoint sweep) + ~1-2h FE (URL builder + 3 callsites
  + validator + useParams + empty-state hint) + ~1h docs + ~30min API
  types regen = **~6-9h total**.
- Pre-launch: hex rejected with informative error. No transition period
  needed.
- Pairs cleanly with 0262 (composite NotFound) + 0263 (pool detail Link
  wraps) — all three touch pool detail surface. Consider single PR with
  sub-commits, or 2 PRs (backend + FE).
- Phase 8 NFT verify is cheap insurance. If `parse_nft_id` already
  accepts strkey, scope unchanged.
- Phase 11 evergreen doc codifies convention to prevent future drift
  (same anti-pattern that produced F-AN-8 in the first place).
- 14 spawn candidates from Phase 3 (Out-of-scope follow-ups in task
  README) include `XXXX_DOCS_api-url-conventions` which is now folded
  into this task as Phase 11 — drop from Phase 3 spawn list.
