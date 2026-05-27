# E0 — Shell (AppShell / header / footer / nav) — Wave 6 Playwright re-pass

**Date:** 2026-05-27
**Wave:** 6 / Track 2 — 2.0
**Baseline:** post-Gate-B (commits `473de2a2`, `9e88114b`, `a5f15166`, `047ce51e`, `6421d3d7`)

## Method

Single Playwright MCP session. Browser stays open. Captured snapshot at `/` then traversed all 14 routes. Console-error count baseline: 1 (`favicon.ico 404`, benign asset miss).

## Findings

### F-W6-E0-1 [Class C, Severity 🟠 HIGH] Footer Resources + Terms/Privacy/Cookies STILL dead spans

CA-1 + CA-2 fix-first task #4 from `triage-gate-B.md` **has NOT landed**. Footer evaluated on home + every other route:

```js
footerDeadSpans: [
  'GitHub',
  'Stellar docs',
  'Soroban docs',
  'Stellar dashboard',
  'Terms of Service',
  'Privacy Policy',
  'Cookies',
];
```

Each rendered as `<div>` / `<span>`, NOT `<a href=…>`. Confirms by DOM evaluation `el => !el.closest('a')`. Footer Explorer column (Home/Transactions/Ledgers/Assets/NFTs/Pools) IS linked — only Resources + Legal columns are dead.

**Evidence:** `findings/screenshots/wave6-E1-home-mobile-375.png` (visible bottom of mobile screenshot — same dead state at desktop).
**Cross-cite:** CA-1, CA-2 (`quick-wins-DM-DN-CA.md`); decision `triage-gate-B.md §4`.
**Severity confirmed unchanged** from Wave 2. Pre-launch must-fix per Gate B rationale (legal liability).

### F-W6-E0-2 [Class A, Severity 🟠 HIGH] Footer "All systems operational" STILL hardcoded

DM-1 unchanged. Footer renders the badge as plain text `<div>All systems operational</div>` with no aria-live, no JS health-probe binding, no styling cue distinguishing it from other text on the page. Visible on every route.

**Cross-cite:** DM-1 (`quick-wins-DM-DN-CA.md`); F-V-1 below (2.3).
**Decision per Gate B §"DM-1":** accept baseline, defer Phase 3 spawn `XXXX_FEATURE_footer-status-health-probe`. Wave 6 positive-confirms intent.

### F-W6-E0-3 [Class C, Severity 🟡 MEDIUM] No hamburger menu at mobile (<768px); nav links shrink to fit and overflow

At viewport 375px (iPhone SE-ish): `<nav>` shows all 6 links (Home / Transactions / Ledgers / Assets / NFTs / Pools) inline — but text wraps and the parent container forces document scrollWidth = 802px against viewport 364px. Page-level horizontal scroll on every route. Touch-friendliness: nav links measure ~50-100px wide but only ~22px tall (below 44×44 a11y target).

**Evidence:** see also `R-responsive-matrix.csv` mobile column.
**Cross-cite:** new Wave 6 — no prior matching ID.

### F-W6-E0-4 [Class C, Severity 🟡 MEDIUM] Header search field placeholder enumerates 4 entity types, page-search no-results hint enumerates 5

`HeroSearch.tsx:22` + `libs/ui/src/layout/SearchInput.tsx:26`:

> `'Search by TX hash, accounts, contract, token'`

`SearchResultsView.tsx:99` no-results hint:

> `'Try a full transaction hash, account address (G…), contract address (C…), liquidity pool (L…), or token code.'`

Header copy omits NFTs and liquidity pools (5 of 6 categories the search tabs support). User won't discover the new L… support unless they paste an unsupported value first.

**Cross-cite:** F-K-4 (Wave 1) — F-K-4 fix added L… to the no-results path but did NOT update the header/page placeholder. Partial fix.

### F-W6-E0-5 [Class B, Severity 🟢 LOW] Header polling duplicates home polling

Network capture after 30 s on `/`:

```
GET /v1/network/stats     ×4
GET /v1/transactions?limit=10  ×4
GET /v1/ledgers?limit=10  ×4
```

Wave 1 finding F-I-3 already flagged TanStack cache-key drift; here the header `HeaderStatsStrip` (`/network/stats`) AND the home `LiveLatestTransactions` (`/transactions?limit=10`) share the same effective endpoint but distinct query keys → no dedup. 12s × 4 polls in 30s ≈ correct interval but doubled requests. Bandwidth not critical at this scale; cross-cite F-I-3 (no new finding required, just confirms scope).

### Console state

Across all routes happy path: 1 `favicon.ico 404` (benign). All other console messages clean. **No React duplicate-key warnings, no deprecated-lifecycle warnings, no strict-mode side effects observed.**

### Positive verifications (post-Gate-B baseline confirms)

| Item                                                 | Status                                                                                                          |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| F-L-1 (search L-strkey paste → redirect)             | ✅ confirmed Wave 6 (see `E14-search.md`)                                                                       |
| F-K-4 (no-results hint lists L…)                     | ✅ partial (no-results path only; header placeholder + empty-state generic hint still don't enumerate prefixes) |
| F-D-2 (composite NotFound single-block on E6/E9/E13) | ✅ confirmed (see `E6-`, `E9-`, `E13-` files)                                                                   |
| F-K-2 (pool reserve labels linked)                   | ✅ confirmed (see `E13-liquidity-pools-detail.md`)                                                              |
| F-K-3 (pool "Since ledger" linked)                   | ✅ confirmed (see `E13-`)                                                                                       |
| NFT route composite + search-click                   | ✅ confirmed (see `E11-`, `E14-`)                                                                               |
| bare digit → /ledgers/<seq>                          | ✅ confirmed (see `E14-`)                                                                                       |

No regressions.
