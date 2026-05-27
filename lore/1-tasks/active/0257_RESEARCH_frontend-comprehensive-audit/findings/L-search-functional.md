# L — Search functional (1.14, Wave 3)

Read-only Playwright MCP. Target: `/search?q=...` page and the top-nav
search (textbox shown in the banner reads "Search by TX hash, accounts,
contract, token"). Tested via direct URL navigation (top-nav textbox
also feeds `/search?q=`).

Source: `web/src/search/useSearchResults.ts` + `web/src/pages/SearchResultsPage.tsx`.
Debounce 300ms (`useSearchResults.ts:66, 69`). Policy `searchPolicy`
(`staleTime: 0, gcTime: 0` — no client cache, every distinct query
roundtrips).

## Per-query matrix

| Query                                                              | Type               | Expected                                          | Actual                                                                                                  | Verdict          |
| ------------------------------------------------------------------ | ------------------ | ------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | ---------------- |
| `7b9bacc894c4580b684692d82e03cc63d2185d3ff09ead8746736e88b2d92089` | full tx hash       | redirect to `/transactions/:hash`                 | redirected — stub renders (Wave 1 A1)                                                                   | ✓ redirect works |
| `GAHHHEIDIBOTXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX`         | account strkey     | redirect to `/accounts/:id`                       | redirected → full account detail                                                                        | ✓                |
| `CUSDCSACXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX`         | contract strkey    | redirect to `/contracts/:id`                      | redirected → contract detail                                                                            | ✓                |
| `LD5MMO5JQEACR6KEAPI4ON4P6XKDIRP6KIIA42YOYIQX2EX4JZO6T2DO`         | pool strkey        | redirect to `/liquidity-pools/:id` OR list filter | **0 results** "No results for ..."                                                                      | ✗                |
| `USD`                                                              | asset code partial | list filter / results                             | Token tab shows 2 hits (USDC + USDCOIN)                                                                 | ✓                |
| `` (empty)                                                         | no query           | placeholder                                       | "Type to search transactions, accounts, contracts, tokens, NFTs, and liquidity pools."                  | ✓                |
| `aaaa...` ×1000                                                    | very long          | graceful error                                    | "Search request failed. Try again..." (API 400)                                                         | ✓ graceful       |
| `<script>alert(1)</script>`                                        | XSS                | escaped                                           | rendered as text inside `"No results for \"<script>alert(1)</script>\""`, no `<script>` injected to DOM | ✓ safe           |

## Findings

### F-L-1 🟠 HIGH `[Class B, Severity HIGH]` — Pool strkey not recognized by search

Pasting a liquidity-pool strkey (`L...`) into search yields zero
results across all 6 tabs. The hint banner on the empty-state also
omits pool strkeys: "Try a full transaction hash, account address
(G…), contract address (C…), or token code." — pool prefix `L...` is
missing from the supported-format list.

Backend `/v1/search?q=L...` either rejects pool strkeys or the FE
classifier in `useSearchResults` / backend doesn't decode them. Either
way: the URL `/liquidity-pools/:id` accepts the hex form on direct
navigation, so the user-facing strkey is a dead identifier in the
search flow.

Confirms Wave 1 B1 / K-4 "strkey ↔ hex strategy" inconsistency at the
search-input boundary.

### F-L-2 🟡 MEDIUM `[Class C, Severity MEDIUM]` — Hint banner enumerates 4 of 6 entity types

Empty-results hint (above): mentions transactions, accounts, contracts,
tokens. Omits NFTs and liquidity-pool addresses. The tab list at top
(`Transactions / Accounts / Contract / Token / NFT / Liquidity Pool`)
implies all six are searchable. Doc drift between hint copy and
implementation surface.

### F-L-3 ✓ PASS — XSS escaped

React JSX text escaping handles `<script>alert(1)</script>` safely.
Verified: zero `<script>` element with `alert(1)` body present in DOM
after navigation. The user-entered string round-trips as text inside
the "No results for ..." string only.

### F-L-4 ✓ PASS — Debounce confirmed by code

`useSearchResults.ts:66` `debounceMs = 300` + `useDebounced(q,
debounceMs)`. Top-nav textbox + URL-driven `/search?q=` both flow
through the same hook. Not live-tested with rapid typing because the
top-nav textbox is in `AppShell` and feeds `?q=` via the same path —
behaviour is identical to direct URL navigation in this regard.

### F-L-5 ✓ PASS — Long query handled gracefully

API rejects (400) on excessively long input; FE displays "Search
request failed. Try again in a moment, or refine your query." No
crash, no console error beyond the underlying fetch failure (which is
expected for that scenario).

### F-L-6 🟡 MEDIUM `[Class D, Severity MEDIUM]` — `treatRedirectAsResult` flag exposed but consumer impact unclear

`useSearchResults.ts:67-95`: when `treatRedirectAsResult=false`
(default) and backend returns `{ type: 'redirect', entity_id, entity_type }`,
the FE auto-navigates to the entity. Otherwise the response is
mapped to a single-hit `results` shape. Both code paths exist; whether
the top-nav uses the redirect path or the `/search` page uses the
hits-rendering path is not visually distinct in the audit. Verified
behaviour from the URL flow: pasting a hash to `/search?q=<hash>`
redirects (so the `/search` route opts into redirect-as-action).
Cataloguing only — no bug, but the dual-mode hook will surprise a
junior reader.

## Class breakdown for L (Wave 3 1.14)

| Class  |             Count |
| ------ | ----------------: |
| A      |                 0 |
| B      |           1 (L-1) |
| C      |           1 (L-2) |
| D      |           1 (L-6) |
| E      |                 0 |
| ✓ pass | 3 (L-3, L-4, L-5) |

## Severity breakdown

| Severity    |        Count |
| ----------- | -----------: |
| 🔴 CRITICAL |            0 |
| 🟠 HIGH     |      1 (L-1) |
| 🟡 MEDIUM   | 2 (L-2, L-6) |
| 🟢 LOW      |            0 |

## Post-merge update 2026-05-25 — develop @ 6b7fb558 (FilipDz tx-detail PR #215)

**Search → tx detail redirect:** Previously redirected to stub (table
row 1, "✓ redirect works" but landing was empty). Now redirects to a
real page with hash validation + loading skeleton + error states +
data sections. Search result-row first-hit click on a tx hit lands at
`/transactions/:hash` and renders Filip's page. No L-finding changed.

**F-L-1, F-L-2, F-L-3, F-L-4, F-L-5, F-L-6:** STILL STAND. Filip didn't
touch search.

## 0270 merge resolution 2026-05-27 — develop @ cb2fa80a (PR #220)

### F-L-1 — **RESOLVED** in `047ce51e` + `6421d3d7`

Search by full pool L-strkey now decodes + redirects to `/liquidity-pools/L…`. Implementation:

- **Backend** (`crates/api/src/search/classifier.rs`): classifier L-strkey decode branch via `stellar_strkey::LiquidityPool::from_string` feeds `hash_bytes` for pool exact-match CTE (same BYTEA(32) shape as tx hash path). `commit 047ce51e`.
- **Backend wire** (`crates/api/src/search/queries.rs`): `fetch_redirect` pool branch wraps `entity_id` via `pool_id_hex_to_strkey`; `fetch_search` pool_hits row mapper converts to canonical strkey at boundary. Redirect target matches strkey-only path validator landed in 0264 Phase 1.
- **Frontend** (`web/src/search/directRouteFor.ts` NEW): bare-digit u32 → `/ledgers/<seq>` runs BEFORE API call (FE-side classifier — backend search has no ledger bucket). Closes Gap D ledger numeric redirect at the optimal layer.
- **Frontend** (`web/src/search/routeForHit.ts` removed; `libs/ui/src/identifiers/routes.ts` consolidates entityRoutes): NFT composite short-circuit produces `/nfts/:contractId/:tokenId` via `routes.nft(c, t)`. Closes NFT-search-404 regression that was carry-over from 0264 Phase 8a (reverted in `4716d5f3`, properly fixed here).

Tests: `cargo test -p api` 132 passed (incl. `classifies_full_l_strkey_as_pool_hash_bytes` + `nft_hit_serializes_composite_fields` + `non_nft_hit_omits_composite_fields`). `nx run-many -t typecheck/lint/build` all green.

### Out of scope — deferred to 0271 (NEW spawned task)

- Asset broad-search by name (`asset_code` ILIKE)
- NFT broad-search by `collection_name`
- Pool L-strkey **prefix** matching (partial autocomplete — Gap E; requires denormalised strkey text column on `liquidity_pools`)

### Considered + REJECTED per scope discipline (with documented rationale)

- **Muxed M-strkey decode** → "no ecosystem precedent for asset auto-redirect" (per 0270 completion note). Worth research follow-up if Stellar Expert decoding pattern desired pre-launch.
- **Asset `code-issuer` composite redirect** → same scope discipline rationale.
- **Drop `SearchResponse::Redirect` anti-pattern** (move classifier FE-only, industry-canonical) → separate refactor PR; out of 0270 scope.
- **Q5(b) bad-CRC L-strkey hint** (silent 0 results on typo) → minor UX nit; defer Phase 3.

### Bonus simplifications (not in original plan, shipped under same PR)

- `tx_hits` broad CTE **dropped** — dead code (redirect path covers tx hash already).
- Classifier base64 32-byte branch **dropped** — no ecosystem caller.
- `is_fully_typed` **derived** from `(hash_bytes, strkey_prefix)` instead of stored field — single source of truth, less drift surface.
- `web/src/search/routeForHit.ts` **deleted entirely**; replaced by `libs/ui/src/identifiers/routes.ts` entityRoutes consolidation (broader than original "Path c overload" plan).
