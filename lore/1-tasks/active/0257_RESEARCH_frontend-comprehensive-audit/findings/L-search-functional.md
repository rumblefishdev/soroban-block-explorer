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

| Query | Type | Expected | Actual | Verdict |
|---|---|---|---|---|
| `7b9bacc894c4580b684692d82e03cc63d2185d3ff09ead8746736e88b2d92089` | full tx hash | redirect to `/transactions/:hash` | redirected — stub renders (Wave 1 A1) | ✓ redirect works |
| `GAHHHEIDIBOTXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX` | account strkey | redirect to `/accounts/:id` | redirected → full account detail | ✓ |
| `CUSDCSACXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX` | contract strkey | redirect to `/contracts/:id` | redirected → contract detail | ✓ |
| `LD5MMO5JQEACR6KEAPI4ON4P6XKDIRP6KIIA42YOYIQX2EX4JZO6T2DO` | pool strkey | redirect to `/liquidity-pools/:id` OR list filter | **0 results** "No results for ..." | ✗ |
| `USD` | asset code partial | list filter / results | Token tab shows 2 hits (USDC + USDCOIN) | ✓ |
| `` (empty) | no query | placeholder | "Type to search transactions, accounts, contracts, tokens, NFTs, and liquidity pools." | ✓ |
| `aaaa...` ×1000 | very long | graceful error | "Search request failed. Try again..." (API 400) | ✓ graceful |
| `<script>alert(1)</script>` | XSS | escaped | rendered as text inside `"No results for \"<script>alert(1)</script>\""`, no `<script>` injected to DOM | ✓ safe |

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

| Class | Count |
|---|---:|
| A | 0 |
| B | 1 (L-1) |
| C | 1 (L-2) |
| D | 1 (L-6) |
| E | 0 |
| ✓ pass | 3 (L-3, L-4, L-5) |

## Severity breakdown

| Severity | Count |
|---|---:|
| 🔴 CRITICAL | 0 |
| 🟠 HIGH | 1 (L-1) |
| 🟡 MEDIUM | 2 (L-2, L-6) |
| 🟢 LOW | 0 |

## Post-merge update 2026-05-25 — develop @ 6b7fb558 (FilipDz tx-detail PR #215)

**Search → tx detail redirect:** Previously redirected to stub (table
row 1, "✓ redirect works" but landing was empty). Now redirects to a
real page with hash validation + loading skeleton + error states +
data sections. Search result-row first-hit click on a tx hit lands at
`/transactions/:hash` and renders Filip's page. No L-finding changed.

**F-L-1, F-L-2, F-L-3, F-L-4, F-L-5, F-L-6:** STILL STAND. Filip didn't
touch search.
