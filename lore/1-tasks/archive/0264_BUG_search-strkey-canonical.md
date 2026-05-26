---
id: '0264'
title: 'Strkey canonical everywhere — strkey-only (no legacy hex compat) + per-endpoint sweep (closes F-L-1 + F-K-4 + F-AN-8)'
type: REFACTOR
status: completed
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
  - date: '2026-05-26'
    status: active
    who: karolkow
    note: 'Activated as part of Gate B fix-first batch (0262/0263/0264 + 0265 off-band CVE) on shared branch.'
  - date: '2026-05-26'
    status: active
    who: karolkow
    note: 'Scope expansion (Phase 8 NFT route refactor) — post-activation audit + stellar.expert convention check found NFT endpoint `/v1/nfts/:i32` uses internal DB surrogate PK (`parse_nft_id` accepts only `i32`). stellar.expert addresses Soroban tokens via contract address (`/explorer/public/contract/C...`); no separate `/nft/N` route exists. To honor "strkey canonical everywhere" intent, Phase 8 is upgraded from verify-only to refactor: external NFT route changes to `/v1/nfts/:contract-strkey/:token_id` composite path. Internal `nft_id i32` PK kept as cursor/join key only. Effort +2-3h backend + +1h FE.'
  - date: '2026-05-26'
    status: completed
    who: karolkow
    note: 'Implemented in 473de2a2. Phases delivered: 1 (pool_id_strkey validator), 2 (4 pool handlers strkey input + decode), 4 (wire shape strkey via pool_id_hex_to_strkey helper), 5-7 (FE routes.pool + isPoolId strkey + LiquidityPoolDetailPage useParams strkey, poolIdStrkey.ts deleted), 8a-c (NFT route composite refactor backend + FE), 11 (docs/architecture/api/url-conventions.md created), 12 (verify + OpenAPI regen). 130 lib tests + 236 bin tests pass. Manual UI verification via Playwright MCP confirms strkey-only enforcement (hex pool 400, strkey pool 404-or-200), NFT composite route works (Punk #1 renders), pool list links all strkey form. **Deferred: Phases 3 + 9 + 10 (search endpoint) + Fala 3 (search output strkey alignment) — spawned as future-search-followup follow-up.** F-AN-8 RESOLVED in this commit. F-L-1 + F-K-4 stay OPEN, blocked on future-search-followup.'
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

## Status: Completed

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

| Entity             | Canonical strkey prefix | Internal/wire form     |
| ------------------ | ----------------------- | ---------------------- |
| Account            | `G...`                  | 32-byte ed25519 pubkey |
| Contract           | `C...`                  | 32-byte contract ID    |
| **Liquidity pool** | **`L...`**              | 32-byte SHA-256 hash   |
| Muxed account      | `M...`                  | composite              |
| Pre-auth tx        | `T...`                  | 32-byte hash           |

Stellar Expert + Horizon + Stellar Lab + Soroban CLI all use strkey
canonical. Hex = internal storage/wire detail.

**Current state in our project (per audit research 2026-05-25):**

| Endpoint                      | Accepts                                               | Industry std    | Verdict                            |
| ----------------------------- | ----------------------------------------------------- | --------------- | ---------------------------------- |
| `/v1/accounts/:id`            | strkey `G...` (`path::strkey(_, 'G', _)`)             | strkey          | ✓ canonical                        |
| `/v1/contracts/:id`           | strkey `C...` (`path::strkey(_, 'C', _)`)             | strkey          | ✓ canonical                        |
| `/v1/transactions/:hash`      | hex hash                                              | hex             | ✓ (tx hash always hex in protocol) |
| `/v1/ledgers/:seq`            | numeric                                               | numeric         | ✓                                  |
| `/v1/assets/:id`              | polymorphic (numeric ID OR C-strkey OR `code-issuer`) | mixed           | ⚠ polymorphic by design            |
| **`/v1/liquidity-pools/:id`** | **hex 64-lower ONLY**                                 | **strkey L...** | **❌ outlier — this task fixes**   |
| `/v1/nfts/:id`                | `parse_nft_id` (TBD — verified in Phase 8)            | strkey expected | ? verify                           |

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
  return /^L[A-Z2-7]{55}$/.test(value); // CAP-38 L-strkey, ~56 chars total
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

| Endpoint                              | Verify                                             | Action in this task        | Document in url-conventions.md     |
| ------------------------------------- | -------------------------------------------------- | -------------------------- | ---------------------------------- |
| `/v1/accounts/:id`                    | `path::strkey(_, 'G', _)` accepts G-strkey only    | already ✓                  | ✓ strkey G                         |
| `/v1/contracts/:id`                   | `path::strkey(_, 'C', _)` accepts C-strkey only    | already ✓                  | ✓ strkey C                         |
| `/v1/liquidity-pools/:id`             | `path::pool_id_strkey` (this task)                 | fixed in Phase 1-2         | ✓ strkey L                         |
| `/v1/nfts/:contract-strkey/:token_id` | composite path (was `/v1/nfts/:i32` surrogate PK)  | **refactor (Phase 8a-8c)** | ✓ strkey C + numeric token_id      |
| `/v1/assets/:id`                      | polymorphic: numeric ID / C-strkey / `code-issuer` | OK (Wave 5 1.2 accepted)   | ⚠ polymorphic — document each form |
| `/v1/transactions/:hash`              | `path::parse_hash` accepts hex 64-lower            | OK (tx hash protocol)      | ✓ hex 64-lower                     |
| `/v1/ledgers/:seq`                    | numeric                                            | OK (ledger seq counter)    | ✓ numeric u32                      |

**Phase 8a — Backend NFT route refactor**

**File:** `crates/api/src/nfts/handlers.rs`, `crates/api/src/nfts/queries.rs`,
`crates/api/src/lib.rs` (route registration)

Rationale: stellar.expert addresses Soroban tokens via contract URL
`/explorer/public/contract/{C-strkey}`. No separate NFT route with
numeric ID exists in the ecosystem. Our explorer has individual NFT
records keyed by `(contract_id, token_id)` in DB (NFT-instance level,
not collection-level). Internal surrogate `nft_id i32` PK is fine for
cursors/joins but MUST NOT leak to external URL.

Changes:

1. Replace `parse_nft_id(raw: &str) -> Result<i32, ...>` with
   `parse_nft_path(contract: &str, token_id: &str) -> Result<(String, String), ...>`
   accepting C-strkey + token*id (validate strkey via existing
   `path::strkey('C', *)` helper, validate token_id as opaque string —
   token_id is contract-defined and may be u64 or string).
2. Update route registration: `/v1/nfts/:id` → `/v1/nfts/:contract_id/:token_id`.
3. Update `get_nft_detail` handler: lookup by `(contract_id, token_id)`
   composite, not `nft_id i32`. Internal query may still join via PK.
4. Update `list_nft_transfers` handler: same composite path param.
5. Keep `nft_id i32` in cursor payload (internal — opaque to clients)
   for stable pagination across composite paths.

```rust
// Before
fn parse_nft_id(raw: &str) -> Result<i32, Response> {
    raw.parse::<i32>().map_err(...)
}

// After
fn parse_nft_path(
    contract: &str,
    token_id: &str,
) -> Result<(String, String), Response> {
    path::strkey_str(contract, 'C')?;
    if token_id.is_empty() || token_id.len() > 128 {
        return Err(...);
    }
    Ok((contract.to_string(), token_id.to_string()))
}
```

**Phase 8b — FE NFT route + URL builder**

**File:** `web/src/router/routes.ts`, `web/src/pages/NftDetailPage.tsx`,
`web/src/router/AppRouter.tsx` (or whichever owns NFT route declaration)

Changes:

1. `routes.nft(contractId: string, tokenId: string)` returns
   `/nfts/${contractId}/${tokenId}`. Drop any `routes.nft(id: number)`
   form.
2. AppRouter route path: `/nfts/:id` → `/nfts/:contractId/:tokenId`.
3. `NftDetailPage.tsx`: read both `contractId` + `tokenId` from
   `useParams`; pass both to `useNftDetail` hook.
4. `useNftDetail` API hook: accept composite, call
   `/v1/nfts/${contractId}/${tokenId}`.
5. All `routes.nft(...)` callsites — grep + update to pass composite.

**Phase 8c — FE NFT list table → composite link**

**File:** `web/src/pages/Nfts*` (NFT list page) + any cross-entity
references (e.g. account NFT holdings, contract-issued NFT list).

Each NFT row's Link wraps composite `(contract_id, token_id)`. If the
row entity is `NftItem`, both fields are already present.

Asset polymorphic exception (Phase 8 footnote): if asset polymorphic
acceptance reveals C-strkey path is buggy → add to scope. Otherwise
documentation-only for assets.

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

| Endpoint                          | Path param      | Form                     | Validator              | Rationale                                                                                                                                                                                                      |
| --------------------------------- | --------------- | ------------------------ | ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/v1/accounts/:id`                | account ID      | strkey `G...`            | `path::strkey('G')`    | CAP-38 canonical                                                                                                                                                                                               |
| `/v1/contracts/:id`               | contract ID     | strkey `C...`            | `path::strkey('C')`    | CAP-38 canonical                                                                                                                                                                                               |
| `/v1/liquidity-pools/:id`         | pool ID         | strkey `L...`            | `path::pool_id_strkey` | CAP-38 canonical                                                                                                                                                                                               |
| `/v1/nfts/:contract_id/:token_id` | NFT instance    | strkey `C...` + token_id | `parse_nft_path`       | NFT = (contract, token_id) composite. stellar.expert addresses Soroban tokens via contract URL; no numeric NFT route exists in ecosystem. Internal `nft_id i32` surrogate PK is internal-only (cursor, joins). |
| `/v1/assets/:id`                  | asset ID        | polymorphic              | `parse_asset_id`       | numeric `assets.id` (internal) OR strkey `C...` (SAC) OR `code-issuer` composite (classic). Documented as exception.                                                                                           |
| `/v1/transactions/:hash`          | tx hash         | hex 64-lower             | `path::parse_hash`     | Tx hash is raw bytes per Stellar protocol; no strkey form exists.                                                                                                                                              |
| `/v1/ledgers/:seq`                | ledger sequence | numeric u32              | direct parse           | Ledger seq is a counter, not an identifier.                                                                                                                                                                    |

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

- [x] `crates/api/src/common/path.rs` exposes `pool_id_strkey` validator
      (strkey-only, returns hex internal)
- [x] All 4 pool handlers (`get_pool`, `get_pool_chart`,
      `list_pool_transactions`, `list_pool_participants`) use the new validator
- [ ] Backend search classifier dispatches `L...` strkey input to pool lookup (deferred to future-search-followup — Phase 3)
- [x] Backend wire response `pool_id` field returns strkey form
- [x] FE `routes.pool(...)` callers pass strkey
- [x] FE `isPoolId` validator updated to check strkey (not hex)
- [x] `LiquidityPoolDetailPage.tsx` `useParams` consumes strkey
- [ ] FE search empty-state hint lists `L...` alongside `G...` and `C...` (deferred to future-search-followup — Phase 10)
- [x] **Per-endpoint sweep:** every endpoint with an `:id` / `:hash`
      param verified canonical (or documented exception) — see
      `docs/architecture/api/url-conventions.md`
- [x] **Phase 8a NFT backend:** route refactored from `/v1/nfts/:i32`
      to `/v1/nfts/:contract_id/:token_id`; `parse_nft_path` validates
      C-strkey + opaque token_id; `get_nft_detail` + `list_nft_transfers`
      lookup by composite; `nft_id i32` surrogate kept internal-only
      (cursor, joins)
- [x] **Phase 8b NFT FE:** `routes.nft(contractId, tokenId)` composite;
      AppRouter `/nfts/:contractId/:tokenId`; `NftDetailPage` reads
      both useParams; `useNftDetail` hook accepts composite; all
      callsites updated
- [x] **Phase 8c NFT FE list:** NFT list rows + cross-entity NFT
      references Link to composite path
- [x] `docs/architecture/api/url-conventions.md` created with full
      per-endpoint table + rationale + ADR-0032 cross-link
- [x] `cargo test -p api` regression cases for strkey accept + hex
      reject + garbage reject (path validator unit tests + integration
      tests via tests_integration.rs updated to L-strkey form)
- [ ] FE manual paste-strkey-into-search test passes (deferred to future-search-followup
      — Phase 3 brake)
- [ ] Audit branch `research/0257_frontend-comprehensive-audit` rebased
      onto develop (post-merge)
- [ ] Findings `F-L-1` (deferred to future-search-followup) + `F-K-4` (deferred to future-search-followup) + `F-AN-8` (RESOLVED in `473de2a2`, post-merge SHA reference
      pending) marked in audit findings docs
- [ ] **Docs updated** — `docs/architecture/api/url-conventions.md`
      created (Phase 11) + `docs/architecture/frontend/frontend-overview.md`
      cross-link added. Per ADR 0032.
- [ ] **API types regenerated** — `crates/api/**` changed (validator +
      handlers + response shape); run
      `npx nx run @rumblefish/api-types:generate` and commit
      `libs/api-types/src/openapi.json` + `libs/api-types/src/generated/*`
      in same commit.

## Notes

- Effort: ~3-5h backend pool (validator + 4 handlers + response shape +
  search classifier) + ~2-3h backend NFT refactor (Phase 8a) + ~1-2h FE
  pool (URL builder + 3 callsites + validator + useParams + empty-state
  hint) + ~1h FE NFT (Phase 8b-8c) + ~1h docs + ~30min API types regen =
  **~9-13h total** (revised 2026-05-26 from ~6-9h after Phase 8 NFT scope
  upgrade).
- Pre-launch: hex rejected with informative error. No transition period
  needed.
- Pairs cleanly with 0262 (composite NotFound) + 0263 (pool detail Link
  wraps) — all three touch pool detail surface. Consider single PR with
  sub-commits, or 2 PRs (backend + FE).
- Phase 8 NFT scope upgrade decision: stellar.expert convention check
  confirmed Soroban tokens addressed via contract URL only; our
  `/v1/nfts/:i32` numeric surrogate violates "strkey canonical
  everywhere" intent. Composite `(contract_id, token_id)` matches
  CAP-46-6 token contract interface (each NFT instance is identified
  by token_id within a contract).
- Phase 11 evergreen doc codifies convention to prevent future drift
  (same anti-pattern that produced F-AN-8 in the first place).
- 14 spawn candidates from Phase 3 (Out-of-scope follow-ups in task
  README) include `XXXX_DOCS_api-url-conventions` which is now folded
  into this task as Phase 11 — drop from Phase 3 spawn list.

## Implementation Notes

Landed in commit `473de2a2` (Gate B batch). **Phases delivered: 1, 2, 4, 5-7, 8a-c, 11, 12. Deferred: 3, 9, 10, Fala 3 — spawned as future-search-followup.**

**Backend** (~10 files):

- `crates/api/Cargo.toml` — added `stellar-strkey = "0.0.16"` dep.
- `crates/api/src/common/path.rs` — `pool_id_strkey(value, param) -> Result<String, Response>`: decodes L-strkey via `stellar_strkey::LiquidityPool::from_string`, returns 64-char lowercase-hex via `hex::encode(bytes)`. CRC-strict (unlike `strkey()` for G/C which is shape-only — pool internal form is the hash, not the strkey itself). Hex input rejected with informative envelope (`hint: use the strkey (L...) returned by /v1/liquidity-pools`). 7 unit tests added.
- `crates/api/src/common/strkey.rs` — `pool_id_hex_to_strkey(hex_str) -> String`: inverse of `pool_id_strkey`. Uses `hex::decode` + `stellar_strkey::LiquidityPool([u8; 32]).to_string().to_string()` (double `.to_string()` intentional: inherent returns `heapless::String<56>`, second call via `Display` bridges to `std::String`). Panics on malformed input — DB-trusted, invariant-protected. 3 unit tests (round-trip + short-input panic).
- `crates/api/src/liquidity_pools/handlers.rs` — 4 handlers (`list_participants`, `get_pool`, `list_pool_transactions`, `get_pool_chart`) decode strkey input via `pool_id_strkey`, pass `pool_id_hex` to DB queries. `map_pool_item` encodes `row.pool_id_hex` → strkey for wire via `pool_id_hex_to_strkey`. utoipa param descriptions updated to "SEP-23 strkey (`L...`, 56 chars)".
- `crates/api/src/transactions/handlers.rs` — `OperationItem.pool_id` field encoded to strkey via `pool_id_hex_to_strkey` (operations carry pool refs in tx detail responses).
- `crates/api/src/nfts/handlers.rs` — `parse_nft_id(raw)` → `parse_nft_path(contract, token_id)`. Validates C-strkey via existing `path::strkey('C')`; token_id non-empty, ASCII, ≤128 chars. `get_nft_detail` + `list_nft_transfers` rewrote to take `Path<(String, String)>`. `list_nft_transfers` resolves composite to internal `nft_id i32` once for transfers JOIN.
- `crates/api/src/nfts/queries.rs` — `fetch_by_id(i32)` → `fetch_by_composite(&str, &str)`; `nft_exists(i32) -> bool` → `nft_exists_by_composite(&str, &str) -> Option<i32>`. WHERE `sc.contract_id = $1 AND n.token_id = $2`. JOIN `soroban_contracts sc` to resolve C-strkey to BIGINT FK.
- `crates/api/src/main.rs` — `api_docs_json_contains_nfts_paths` doc-test updated to assert composite paths.
- `crates/api/src/tests_integration.rs` — 5 NFT integration tests + 6 LP integration tests updated. All hex pool ID literals (`"0".repeat(64)`) replaced with L-strkey form `LAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABLIR` (zero-payload L-strkey).
- `crates/api/src/assets/handlers.rs` — deduplicated local `is_strkey_shape` copy; imports from `crate::common::strkey::is_strkey_shape` (single source of truth).

**Frontend** (~10 files):

- `web/src/router/routes.ts` — `routes.pool(strkey)` (1-arg, doc updated to strkey); `routes.nft(contractId, tokenId)` (2-arg composite, encodes only token_id since C-strkey is URL-safe by construction).
- `web/src/router/index.tsx` — route `nfts/:id` → `nfts/:contractId/:tokenId`.
- `web/src/api/hooks/useNftDetail.ts`, `useNftTransfers.ts` — signatures `(contractId, tokenId, ...)`; pass `{ path: { contract_id, token_id } }` to api-types generated client.
- `web/src/pages/NftDetailPage.tsx` — reads `contractId` + `tokenId` from `useParams`. `tokenId` is taken **directly from useParams** (react-router-dom v6+ already URL-decodes path params — earlier manual `decodeURIComponent` removed to prevent double-decode of `%` literals). Validates via `isContractId(contractId)` + `0 < tokenId.length <= 128`.
- `web/src/pages/nft-detail/NftTransfers.tsx` — props `{ contractId, tokenId }`, useCursorPagination resetKey composite.
- `web/src/pages/nfts/NftNameCell.tsx` — list cell `routes.nft(row.contract_id, row.token_id)`.
- `web/src/pages/liquidity-pools/PoolsTable.tsx` — dropped `poolIdHexToStrkey` import + call (backend now returns strkey directly); `IdentifierDisplay value={row.pool_id}` consumes wire strkey.
- `web/src/pages/pool-detail/{PoolSummary,PoolDetailHeader}.tsx` — dropped `poolIdHexToStrkey` calls.
- `web/src/pages/LiquidityPoolDetailPage.tsx` — `useParams` strkey-only.
- `libs/ui/src/identifiers/validators.ts` — `isPoolId` regex `/^L[A-Z2-7]{55}$/` (was 64-char lowercase hex).

**Deleted**: `web/src/utils/poolIdStrkey.ts` (92 LOC manual SEP-23 encoder) — no longer needed since backend returns strkey directly. Moved to `.trash/` per project policy.

**Codegen**: `libs/api-types/src/{openapi.json,generated/}` regen committed. NFT path types now `{ contract_id: string; token_id: string }`; pool path/response shapes use strkey.

**Doc**: `docs/architecture/api/url-conventions.md` (76 lines) created. Per-endpoint table covering 7 endpoints (accounts, contracts, liquidity-pools, nfts, assets, transactions, ledgers) with canonical form + validator + rationale. ADR 0032 cross-link.

**Tests**: 130 lib tests + 236 bin tests pass. `cargo check -p api` clean.

## Issues Encountered

- **Heapless vs std String bridge in `pool_id_hex_to_strkey`**: `stellar_strkey::LiquidityPool::to_string()` returns `heapless::String<56>` (no_std, inherent method takes precedence over `Display::to_string` trait method). Single `.to_string()` returns heapless, function signature requires `std::String` — double `.to_string()` needed to bridge. Comment in code locks the invariant against well-meaning "fix" attempts (CodeRabbit AI review actually flagged this and recommended single call — verified manually that single call breaks compilation with E0308).

- **`cargo lambda watch` requires `/lambda-url/<fn-name>/` prefix for HTTP routing** when running multi-function packages (api + extract_openapi bins). Discovered during local stack verification — initial curl to `/v1/...` returned a "default function disabled" message. Worked around by setting `VITE_API_BASE_URL='http://localhost:9000/lambda-url/api'` for FE. Documented in `crates/api/Cargo.toml` is which functions get bundled.

- **lint-staged stash race during commit**: first attempt to commit the activate-task changes lost the staged content because `git mv` only stages the rename, not the in-place edits made via Edit before mv. lint-staged "Restoring unstaged changes" wiped the YAML status flip + history append back to the working tree, leaving the commit with only the rename. Workaround: explicit `git add` before commit + verify via `git diff --cached`. Documented in this task's history entries so future sessions don't trip on the same flow.

- **Integration tests held literal hex pool IDs** (`"0".repeat(64)`) for invalid-request envelope tests. After Phase 1 (strkey-only validator) those tests started returning the new "must be L-strkey" envelope on the **first** validation step, masking the actual `?interval=1m` / `?from=after?to=` validations they were exercising. Fixed by replacing 6 occurrences of the hex literal with the zero-payload L-strkey (`LAAA...BLIR`) so requests pass path validation and reach the param/range validators.

- **Search endpoint scope deferred mid-PR**: 4 in-flight subagent commits' worth of search work (Phase 3 classifier L-decode, Phase 9 no-op confirm, Phase 10 FE hint, Fala 3 search output strkey alignment for pool + NFT composite) was reverted from working tree per user decision to keep the Gate B batch focused. Lost some staged content along the way (mishandled `git checkout HEAD --` instead of `git restore --staged`); recovered via dangling-blob retrieval (`git fsck --lost-found` + blob content matching) and committed only the minimum required to keep the batch compiling (the `pool_id_hex_to_strkey` helper). Full search work captured in future-search-followup follow-up with recovery hints.

## Design Decisions

### From Plan

1. **Strkey-only on path validator, no legacy hex compat**: per task body — project is pre-deploy, no existing hex bookmarks to preserve. Backend rejects hex with informative 400 (`hint: use the strkey (L...) returned by /v1/liquidity-pools`). Cleaner than maintaining a transition period.

2. **CRC-strict pool validator, shape-only G/C validator**: pool internal DB form is the hash (BYTEA(32)), so a wrong-CRC L-strkey can't decode and must reject at validator. G/C path validators stay shape-only per ADR 0037 — wrong-CRC G/C strkeys fall through to DB miss → 404 (same UX as non-existent address). Asymmetry intentional and documented in `common/path.rs` doc comments.

3. **Phase 8 NFT route refactor (composite path)**: stellar.expert convention check confirmed Soroban tokens addressed via contract URL (`/explorer/public/contract/C...`) — no separate `/nft/N` route exists in ecosystem. Internal `nft_id i32` surrogate stays as DB PK + cursor key, but external route becomes `/v1/nfts/:contract_id/:token_id` composite. Matches CAP-46-6 token contract interface.

4. **`hex` crate over manual loops**: `hex::encode(bytes)` + `hex::decode(s)` replace manual `for b in &bytes { write!(&mut s, "{b:02x}") }` and `u8::from_str_radix(pair, 16)` loops. `hex` already a workspace dep, no new cost.

### Emerged

5. **`docs/architecture/api/url-conventions.md` created** (not in original Phase 11 scope): per-endpoint table + rationale formalising the strkey-canonical convention. Cross-links ADR 0032 (evergreen docs gate). Replaces what was loosely planned as inline doc comments — promoted to standalone evergreen doc because the convention spans 7 endpoints + has 3 documented exceptions (assets polymorphic, transactions hex, ledgers numeric).

6. **`assets/handlers.rs::is_strkey_shape` deduped against `common/strkey::is_strkey_shape`**: pre-existing duplicate, surfaced during sweep. Removed local copy + imported from common. Not in original task scope but landed in same commit because the change is trivial + lowered drift surface.

7. **Search portion deferred mid-batch to future-search-followup**: see Issues §5. User decision after first round of subagent commits — Phase 3 + 9 + 10 + Fala 3 collectively form a coherent "search endpoint strkey + output alignment" unit that can ship independently. Cleaner to land 85% of 0264 with the deferred portion explicitly tracked than to keep the in-flight changes hanging.

8. **`pool_id_hex_to_strkey` panics on malformed input** (rather than `Result`): the helper operates on values flowing from the DB through `BYTEA(32)` columns, which Postgres enforces. A panic here surfaces a DB invariant violation loudly — preferable to silently mapping a corrupted row to an empty wire field. Tests cover the panic explicitly.

9. **`P0264 Phase 8 (NFT)` scope upgrade from "verify-only" to "refactor"**: task body originally listed Phase 8 as "verify each endpoint accepts canonical form". Post-activation audit + stellar.expert convention check found NFT external URL used a numeric DB surrogate (`/v1/nfts/:i32`) — violates "strkey canonical everywhere" spirit. Upgraded to a full route refactor adding ~2-3h backend + ~1h FE effort to the task. Captured as a separate history entry on the task body.

10. **vite `^7.0.0` semver line in `package.json` unchanged**: task 0265 ships in the same batch and explicitly says `package.json` line stays `^7.0.0`; only `package-lock.json` pins 7.3.3. Initial `npm install vite@^7.3.3 --save-dev` bumped the semver floor; reverted to `^7.0.0` per AC (semver still allows 7.3.3 resolution).

## Future Work

**Spawned as backlog tasks**:

- **future-search-followup** — Search strkey canonical + output gaps. Covers Phases 3, 9, 10 (deferred from this task) + Fala 3 (search output strkey alignment for pool + NFT composite) + a handful of UX gaps surfaced during senior review (M-strkey muxed → G resolution, asset composite redirect, ledger numeric redirect). F-L-1 + F-K-4 stay OPEN until future-search-followup lands.

**Architectural follow-up not yet spawned** (consider if scope grows):

- Refactor `IdentifierDisplay` in `libs/ui` to accept `href` as a prop, drop the duplicated route table in `libs/ui/src/identifiers/routes.ts`. Single source of truth for URL conventions in `web/src/router/routes.ts`. Identified during senior review during 0263 / 0264 review pass — kept as future task to avoid blowing scope.
