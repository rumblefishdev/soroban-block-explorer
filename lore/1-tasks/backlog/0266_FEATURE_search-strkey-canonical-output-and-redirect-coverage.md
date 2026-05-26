---
id: '0266'
title: 'Search strkey canonical output + redirect coverage gaps (deferred from 0264 + senior review)'
type: FEATURE
status: backlog
related_adr: ['0032']
related_tasks: ['0264', '0262', '0263', '0265', '0257']
tags:
  [
    'backend',
    'frontend',
    'audit-blocker',
    'priority-high',
    'effort-medium',
    'cross-cutting',
    'phase-search',
  ]
links:
  - 'Parent batch: lore/1-tasks/archive/0264_BUG_search-strkey-canonical.md'
  - 'Finding F-L-1 (OPEN): lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/L-search-functional.md'
  - 'Finding F-K-4 (OPEN): lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/K-cross-entity-links.md'
  - 'PR #219 (parent batch): https://github.com/rumblefishdev/soroban-block-explorer/pull/219'
history:
  - date: '2026-05-26'
    status: backlog
    who: karolkow
    note: 'Spawned from 0264 deferred scope + senior review redirect-coverage gaps. Merges T1 (Phase 3/9/10 + Fala 3 search-output strkey alignment) and T3 (Gap A/B/D redirect coverage: muxed M→G, asset composite, ledger numeric). Also closes the routing regression introduced by 0264 Phase 8a: NFT search hits currently route to `/nfts/<surrogate>` which React Router cannot match (composite route lands in Phase 8a, but `routeForHit` / `SearchHit` do not yet carry composite payload — explicit NFT short-circuit was reverted in 4716d5f3 to defer the proper fix here). F-L-1 + F-K-4 stay OPEN until this task lands.'
---

# Search strkey canonical output + redirect coverage gaps

## Summary

Deferred portion of task 0264 (strkey canonical everywhere) merged with
senior-review redirect-coverage gaps. Parent 0264 shipped 85% of the
strkey-canonical sweep — pool path validator, wire-shape strkey, NFT
route composite refactor, FE consumption, docs — but **dropped the
entire search endpoint scope** to keep PR #219 focused.

This task picks all of it up plus three usability gaps surfaced during
senior review on the same surface (muxed-account decode, asset
composite redirect, ledger numeric redirect). Bundling because the
classifier + `fetch_redirect` + `SearchHit` mapping all live in
`crates/api/src/search/` and any one of them changes the shape of the
in-flight diff for the others.

## Context

### Findings still OPEN

- **F-L-1** (Class B 🟠 HIGH) — search by full pool strkey returns 0
  results. Classifier does not know `L...` is a pool id.
- **F-K-4** (Class B 🟠 HIGH) — empty-state hint omits `L...` from the
  list of supported input formats.

### Cascading runtime breaks introduced by parent 0264 (must fix here)

- **NFT search hits route to a 404.** Phase 8a refactored `/v1/nfts/:id`
  → `/v1/nfts/:contract_id/:token_id`. `routeForHit` is on HEAD shape
  (single-segment `getIdentifierHref('nft', surrogate_id)`), which now
  emits `/nfts/<surrogate>` — React Router can't match the composite
  route, so the user gets a hard 404. An NFT-list soft-fallback was
  briefly committed in 9c3db048 (`return routes.nfts`) and then
  reverted in 4716d5f3 per user decision to land the proper composite
  fix here.
- **Pool search redirect returns hex.** `fetch_redirect` pool branch
  currently emits `entity_id: id` where `id` is the hex form. FE
  navigates to `/liquidity-pools/<hex>` which the strkey-only path
  validator (landed in 0264 Phase 1) rejects with 400. The proper fix
  is to wrap the redirect with `pool_id_hex_to_strkey` (helper landed
  with 0264 Phase 4).

### Usability gaps surfaced during senior review

- **Gap A — Muxed account M-strkey.** stellar.expert decodes
  `M...` paste to the underlying ed25519 + searches as G. Exchange
  users routinely paste M-addresses; our classifier doesn't know what
  to do with them and falls through to text search → empty results.
- **Gap B — Asset composite redirect.** Paste `USDC-GAB...` →
  classifier returns nothing → broad search finds via `asset_code`
  ILIKE, but **no redirect short-circuit** even though the path
  validator (`parse_asset_id`) already accepts the composite form.
- **Gap D — Ledger numeric redirect.** User types `12345` (ledger
  sequence) → 0 results (classifier has no ledger branch, broad
  search has no ledger CTE). Most common numeric user-facing
  identifier — should redirect to `/ledgers/12345` if the sequence
  exists.

### Out of scope (still deferred — separate tasks)

- **Gap E** pool L-prefix partial autocomplete — requires denormalized
  strkey column on `liquidity_pools` (DB stores only the raw hash);
  larger refactor.
- **Gap F** NFT composite paste (`CCCR.../5`) — format isn't an
  ecosystem-standard user paste source.
- **Architectural refactor** of `libs/ui/identifiers/routes.ts` →
  drop the duplicate route table, have `IdentifierDisplay` accept
  `href` prop. Spawn separately if undertaken.

## Implementation Plan

### Phase 3 — Backend classifier: full L-strkey decode

**File:** `crates/api/src/search/classifier.rs`

Insert an L-strkey decode branch before the G/C prefix probe. Use
`stellar_strkey::LiquidityPool::from_string(&q.to_ascii_uppercase())`.
On success, populate `hash_bytes` with the decoded 32-byte payload —
existing pool dispatch keys off `hash_bytes` via the `pool_hits` CTE
(BYTEA(32) match), so no SQL or handler changes are required.

```rust
let upper = q.to_ascii_uppercase();
if let Ok(stellar_strkey::LiquidityPool(bytes)) =
    stellar_strkey::LiquidityPool::from_string(&upper)
{
    out.hash_bytes = Some(bytes.to_vec());
    out.is_fully_typed = true;
    return out;
}
```

Add unit test `classifies_full_l_strkey_as_pool_hash_bytes` mirroring
the existing G/C tests.

### Phase 3+A — Muxed M-strkey → underlying G

Same file. Decode `M...` via
`stellar_strkey::ed25519::MuxedAccount::from_string`, extract
`.ed25519`, encode as `PublicKey` → underlying G-strkey, populate
`strkey_prefix` with the G form. Account lookup runs normally.

```rust
if let Ok(muxed) =
    stellar_strkey::ed25519::MuxedAccount::from_string(&upper)
{
    let g = stellar_strkey::ed25519::PublicKey(muxed.ed25519)
        .to_string()
        .to_string();
    out.strkey_prefix = Some(g);
    out.is_fully_typed = true;
    return out;
}
```

Test: paste a known mainnet M-address, classifier returns the
underlying G-strkey.

### Phase 3+B — Asset composite redirect

**Files:** `crates/api/src/search/classifier.rs`,
`crates/api/src/search/queries.rs`,
`crates/api/src/search/dto.rs` (if a new dispatch channel is needed).

Two implementation paths — pick one:

- **Path 1 (preferred, no new dispatch channel):** classifier detects
  `CODE-GAB...` shape (split on last `-`, validate issuer as G-strkey),
  populates a new optional `Classified.asset_composite: Option<(String, String)>`.
  `fetch_redirect` reads it, looks up via `assets WHERE asset_code = $1 AND issuer = $2`,
  returns redirect.
- **Path 2:** add a `text_search` channel to `Classified` that the
  asset CTE keys off. More general but heavier refactor.

Test: paste `USDC-GAB...` valid → redirect to `/assets/USDC-GAB...`;
paste valid composite but unknown issuer → fall through to broad
search.

### Phase 3+D — Ledger numeric redirect

**Files:** `crates/api/src/search/classifier.rs`,
`crates/api/src/search/queries.rs`.

Classifier detects all-digits input that parses to `u32`. Add a new
`Classified.ledger_sequence: Option<u32>` field. `fetch_redirect`
runs `SELECT sequence FROM ledgers WHERE sequence = $1`; redirects on
hit, falls through otherwise.

Edge: also numeric IDs collide with asset numeric surrogate +
NFT surrogate. Decision: redirect to ledger first (most common user
intent for a bare number); fall through to broad search if no ledger
row. Document the priority order in `fetch_redirect` doc comment.

### Phase 9 — Backend search empty-state response

**No-op.** `SearchResponse` is `Redirect | Results{groups}` — no
"supported formats" hint payload in the wire response. FE owns the
empty-state hint copy. Confirmed in 0264 senior review.

### Phase 10 — FE search empty-state hint adds `L...`

**File:** `web/src/search/SearchResultsView.tsx:99`

Update the empty-state copy to include the liquidity-pool prefix:

> "Try a full transaction hash, account address (G…), contract address
> (C…), **liquidity pool (L…)**, or token code."

Closes F-K-4.

### Fala 3a — Pool hit identifier hex → strkey

**File:** `crates/api/src/search/queries.rs`

- Import `use crate::common::strkey::pool_id_hex_to_strkey;`
- `fetch_redirect` pool branch: change `entity_id: id` to
  `entity_id: pool_id_hex_to_strkey(&id)` so the redirect target
  matches the strkey-only path validator from 0264 Phase 1.
- `fetch_search` pool_hits CTE: the column projection stays hex
  (CTE-internal), but the Rust row mapper that builds `SearchHit`
  converts `identifier` via `pool_id_hex_to_strkey`.

Add a unit test in `dto.rs` that asserts a synthetic pool hit
serializes with the strkey form on the wire.

### Fala 3b — NFT hit composite payload

**File:** `crates/api/src/search/dto.rs`

Extend `SearchHit` with two optional fields:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub contract_id: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub token_id: Option<String>,
```

Two tests: `nft_hit_serializes_composite_fields` (NFT carries both)
and `non_nft_hit_omits_composite_fields` (others serialize without
the keys).

**File:** `crates/api/src/search/queries.rs`

- `nft_hits` CTE: `JOIN soroban_contracts sc ON sc.id = n.contract_id`,
  project `sc.contract_id AS contract_id, n.token_id AS token_id`.
- All other CTEs project `NULL::varchar AS contract_id` +
  `NULL::varchar AS token_id` to keep the UNION column list aligned.
- UNION SELECT column lists extend with the two new columns.
- Rust row mapper pulls `row.get("contract_id")` + `row.get("token_id")`
  into `SearchHit`.

### Fala 3c — FE routeForHit NFT composite branch

**File:** `web/src/search/routeForHit.ts`

Add NFT-composite short-circuit before the unified dispatch:

```ts
if (hit.entity_type === 'nft' && hit.contract_id && hit.token_id) {
  return routes.nft(hit.contract_id, hit.token_id);
}
return getIdentifierHref(hit.entity_type, idForUrl);
```

Imports `routes` from `'../router/routes.js'` to use the composite
2-arg builder landed in 0264 Phase 8b. Closes the NFT-search-404
regression — `routeForHit.ts` was at HEAD shape in PR #219 (revert
`4716d5f3` after a soft-fallback attempt was discussed and rejected
in favour of the proper composite payload landing here).

### Fala 3d — libs/ui routes.ts NFT decision

**File:** `libs/ui/src/identifiers/routes.ts`

Pick one, document in the diff:

- **(a) keep current** `nft: (id) => /nfts/${encodeURIComponent(id)}`
  — still broken for `IdentifierDisplay type="nft"` direct calls
  (zero callers today per earlier grep). Acceptable if it stays
  unreachable; document that the unified dispatch is not the right
  API for composite-routed entities.
- **(b) throw** on `'nft'` like the earlier attempt in this batch —
  defensive but dead branch.
- **(c) function overload** `getIdentifierHref(type: 'nft', c, t)` +
  the existing single-arg signature for other types — type-safe.
- **(d) drop unified dispatch entirely** and have
  `IdentifierDisplay` accept `href` prop. Cleanest but cross-cutting
  (58 callsites) — **out of scope here**, spawn separately.

Senior recommendation captured during the parent PR: **(c) overload**
for now, with **(d)** as the long-term architectural follow-up.

## Q5 nits worth folding in (low effort)

- **Q5(a) classifier comment** — the L-decode block's "Must run before
  generic G/C prefix probe so `L...` isn't misclassified as a
  partial-strkey prefix" comment is misleading. `is_strkey_prefix`
  filters by `bytes[0] == 'G' | 'C'`, so an `L...` input never
  collides with the partial-prefix branch. Rewrite to reflect the real
  reason (full L-strkey is unambiguous pool id; partial L-prefix would
  be ambiguous with autocomplete — but L-prefix autocomplete is itself
  out of scope, see Gap E).
- **Q5(b) bad-CRC L-strkey UX** — paste `L...` with a typo currently
  produces silent 0 results. Better: classifier flag
  `malformed_pool_strkey`, handler returns 400 with hint "checksum
  doesn't match — likely a typo". Optional, ship if time allows.

## Acceptance Criteria

### Backend

- [ ] `classifier::classify` decodes a full L-strkey into the shared
      `hash_bytes` channel; unit test added
      (`classifies_full_l_strkey_as_pool_hash_bytes`)
- [ ] `classifier::classify` decodes an M-strkey to the underlying
      G-strkey via `MuxedAccount::from_string`; unit test added
- [ ] `classifier::classify` detects asset `code-issuer` composite
      with G-strkey issuer; unit test added
- [ ] `classifier::classify` detects numeric ledger sequence; unit test
      added
- [ ] `fetch_redirect` pool branch returns strkey `entity_id` via
      `pool_id_hex_to_strkey`
- [ ] `fetch_redirect` runs the asset-composite + ledger-numeric
      branches; redirect priority documented
- [ ] `fetch_search` pool_hits identifier converted to strkey in the
      Rust mapper
- [ ] `SearchHit` extended with `contract_id` + `token_id` optional
      fields
- [ ] `nft_hits` CTE projects `(sc.contract_id, n.token_id)`; UNION
      column list extended with `NULL` projections for non-NFT CTEs
- [ ] Rust mapper populates the new fields from row
- [ ] `nft_hit_serializes_composite_fields` +
      `non_nft_hit_omits_composite_fields` tests added
- [ ] Q5(a) misleading classifier comment rewritten
- [ ] Q5(b) bad-CRC L-strkey hint (optional, ship if time allows)

### Frontend

- [ ] `SearchResultsView` empty-state hint includes `L...` (closes F-K-4)
- [ ] `routeForHit` NFT-composite short-circuit produces
      `/nfts/:contractId/:tokenId` via `routes.nft(c, t)` —
      **closes the NFT-search-404 regression introduced by 0264
      Phase 8a + reverted in 4716d5f3**
- [ ] `libs/ui/identifiers/routes.ts` NFT decision landed (Path c
      overload preferred); documented in commit message
- [ ] Manual: paste pool L-strkey into search → result row, click →
      navigates to `/liquidity-pools/L…`
- [ ] Manual: paste pool hex 64 → redirect navigates to
      `/liquidity-pools/L…` (strkey via Fala 3a conversion)
- [ ] Manual: paste M-strkey → redirect to `/accounts/G…` (underlying)
- [ ] Manual: paste `USDC-GAB...` → redirect to `/assets/USDC-GAB...`
- [ ] Manual: type `12345` (existing ledger) → redirect to
      `/ledgers/12345`
- [ ] Manual: NFT name search → result row, click → navigates to
      `/nfts/:c/:t`

### Cross-cutting

- [ ] OpenAPI regen committed (`SearchHit` composite fields appear in
      `libs/api-types/src/{openapi.json,generated/types.gen.ts}`)
- [ ] `cargo test -p api` + `nx run web:typecheck` + lint all green
- [ ] Finding `F-L-1` in `L-search-functional.md` marked
      `RESOLVED in <SHA>`
- [ ] Finding `F-K-4` in `K-cross-entity-links.md` marked
      `RESOLVED in <SHA>`
- [ ] **Docs updated** — `docs/architecture/api/url-conventions.md`
      table extended with the new redirect priority order
      (hash → L-strkey → muxed → asset composite → ledger numeric →
      G/C prefix → text). Per ADR 0032.
- [ ] **API types regenerated** — `crates/api/src/search/**` changed;
      run `npx nx run @rumblefish/api-types:generate` and commit the
      regen.

## Effort estimate

| Phase                                           | Effort                                     |
| ----------------------------------------------- | ------------------------------------------ |
| Phase 3 (L-decode)                              | ~30 min                                    |
| Phase 3+A (muxed M→G)                           | ~30 min                                    |
| Phase 3+B (asset composite redirect)            | ~1h (Path 1, classifier + redirect branch) |
| Phase 3+D (ledger numeric redirect)             | ~45 min                                    |
| Phase 10 (FE hint)                              | ~10 min                                    |
| Fala 3a (pool hex→strkey)                       | ~30 min                                    |
| Fala 3b (SearchHit composite + nft_hits JOIN)   | ~1.5h                                      |
| Fala 3c (routeForHit NFT branch)                | ~15 min                                    |
| Fala 3d (libs/ui NFT decision, Path c overload) | ~20 min                                    |
| Q5(a) comment                                   | ~5 min                                     |
| Q5(b) bad-CRC hint (optional)                   | ~30 min                                    |
| Tests (unit + integration)                      | ~1h                                        |
| Manual UI verification                          | ~30 min                                    |
| OpenAPI regen + docs update                     | ~30 min                                    |
| **Total**                                       | **~7-8h**                                  |

## Notes

- **Recovery hint**: the parent 0264 session generated working
  implementations for Phase 3 + Fala 3 components. Captured in
  dangling git blobs (search `git fsck --lost-found` for files
  containing `classifies_full_l_strkey_as_pool_hash_bytes` or
  `nft_hits` with `sc.contract_id`). Available until next `git gc`
  (~2 weeks from 2026-05-26).
- **Pairs cleanly with** any task that touches `crates/api/src/search/`
  — single PR covering classifier + redirect + SearchHit is the
  natural unit.
- **Audit-blocker for 0257 Wave 6**: until this lands, F-L-1 + F-K-4
  stay OPEN; Wave 6 Track 2 (Playwright marathon) will re-report
  search emptiness on pool L-strkey paste + missing hint.
