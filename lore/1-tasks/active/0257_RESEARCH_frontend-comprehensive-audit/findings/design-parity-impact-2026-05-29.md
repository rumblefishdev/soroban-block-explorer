# design_parity ROUND 2 Impact Analysis (PR #224) — vs audit-action-queue

**Date:** 2026-05-29
**Analyst:** code inspection + lint run (no live Playwright — see live-re-verify queue)
**Branch tip analyzed:** `35ac27c0` (merge of `origin/develop` into audit branch)
**Source commits (PR #224, the 3 new develop commits):**

- `7e067d53` — Merge PR #224 from `feat/design_parity` (the full round-2 diff lives in `fce0d666^..7e067d53`)
- `39aafc49` — fix: address Copilot PR review on #224 (breadcrumb fix, events-count comment, `bodyMonoXsMedium`, HomePage header comment)
- `fce0d666` — feat: accounts list page + responsive nav/home tweaks

**Scope:** ~95 files / +3535 / −1265. Big-ticket items: real `/accounts` list page, font swap (Mona Sans/Inter → Clash Display/Satoshi, TTF→woff2), pool-detail polish (6 files), EmptyState/error-state restyle, search theme-token form refactor, AssetMetadata/AssetIcon, OperationFlowTree rewrite, contract detail breadcrumb fix.

**Maps against:** `audit-action-queue.md` (45 cards + appendix, incl. Category 11 design_parity-regression cluster) and `R-responsive-matrix.md` (RESPONSIVE-1/2 RESOLVED 2026-05-28; 3/4/5 → cards 11.5/11.6/11.7).
**Methodology:** same as round 1 (`design-parity-impact-2026-05-27.md`).

> **Caveat (unchanged from round 1).** This is a Figma-parity + responsive + feature pass authored independently of the audit. It was NOT written to close audit cards. Most apparent "closures" are partial or cosmetic-only; the round-1 regressions (F-DP-1..4) were almost entirely NOT fixed; one share-% "fix" is illusory (see §F-W6-E13-1).

---

## TL;DR (return numbers)

- **Cards flipped to DONE by round 2:** **0** (none fully closed).
- **Cards flipped to PARTIAL (newly touched / advanced):** **1 clean new closure (card 1.3 — `/accounts` half), plus partial touches to 7.1, and a breadcrumb sub-fix.** Card 1.3 advances from PARTIAL to "PARTIAL — accounts half DONE, contracts half still TODO".
- **Round-1 open items closed:** **1 of 8** (card 1.3's `/accounts` is now a REAL list page). The other 7 (F-DP-1..4, RESPONSIVE-3/4/5, /contracts stub) are NOT closed.
- **F-P-1 lint warning:** **STILL PRESENT** — `assetColor.ts:131:10 Forbidden non-null assertion` confirmed by live `nx lint` run (1 problem, 0 errors, 1 warning).
- **NetworkToggle:** **STILL FAKE** — config.ts unchanged; not wired, not removed; still invisible on `/`.
- **Accounts list:** **REAL** (`AccountsListPage` + `useAccountsList` + filters + pagination). **/contracts still PageStub.**
- **New regressions:** none structurally severe; one *illusory* fix (share %), whole-app **font swap** needs live visual re-verify, EmptyState/error-states restyled (visual re-verify).
- **Top 3 highest-value closures:** (1) real `/accounts` list page (launch-blocker half of F-A-5 Gap 1); (2) contract-detail breadcrumb fix (`Account/<deployer>` → `Contracts/<id>`, closes a round-1 lower-severity regression note); (3) font migration to woff2 (load/bundle win, ~1.15MB TTF → ~72KB woff2).

---

## 1. Summary table — card → status change

Legend: SUGGESTED status for `audit-action-queue.md`. **Apply only after review.** SHA = round-2 commit evidence.

| Card | Title | Current | Suggested | Evidence (round 2) |
|------|-------|---------|-----------|--------------------|
| 1.1 | Footer legal + external links | TODO | **unchanged** | Footer.tsx touched (4 lines, cosmetic). RESOURCES + LEGAL still dead `<span>` (no href). "All systems operational" still hardcoded. CA-1/CA-2 open. |
| 1.2 | Build SHA / version stamp | TODO | **unchanged** | No vite `define`, no SHA. Not touched. |
| 1.3 | Contracts list + nav | PARTIAL | **PARTIAL (advance)** | **`/accounts` is now a REAL list page** (`web/src/pages/AccountsListPage.tsx` + `web/src/api/hooks/useAccountsList.ts` + `accounts/AccountsTable.tsx` + `accounts/AccountsFilters.tsx`, `fce0d666`). **`/contracts` is STILL `<PageStub>`** (`router/index.tsx:66`). F-A-5 Gap 1: accounts half DONE, contracts half TODO. |
| 2.1 | Format/truncate/debounce unify | TODO | **unchanged** | No `libs/ui/src/format/`. Inline `toLocaleString('en-US')` still in TopNav.tsx:77. Duplications untouched. |
| 2.2 | Folder rationalization (PageStub) | TODO | **unchanged + still SCOPE CONFLICT** | PageStub now has **1** live consumer (`/contracts` only — `/accounts` graduated to a real page). F-AH-1 still FALSE while `/contracts` is a stub; PageStub deletion still gated behind the contracts half of card 1.3. NEW files `detail/DataListCard.tsx`, `detail/KpiCell.tsx`, `detail/PageHeader.tsx` add to the `web/src/pages/detail/` folder the card wants to hoist (mild scope growth). |
| 2.3 | EmptyState/LoadingState primitives | TODO | **unchanged** | EmptyState.tsx restyled (125 lines, variant `default`/`warning`/`error` + `py` prop) but still NO shared `<TableSkeleton>`/`<SectionSkeleton>`. `bodyMonoXsMedium` typography variant added (`39aafc49`). Visual polish, not the consolidation goal. |
| 3.1 | noUncheckedIndexedAccess + F-P-1 | TODO | **unchanged** | No tsconfig change. **F-P-1 lint warning STILL PRESENT** (assetColor.ts:131 `!`). |
| 3.2 / 3.3 | Branded IDs / assertNever | TODO | **unchanged** | Not touched. |
| 4.1 | Bundle / LP lazy / vendor split | TODO | **mixed (net slight POSITIVE on fonts)** | No manualChunks, no lazy LP chart, no visualizer. BUT font migration TTF→woff2 cuts ~1.15MB of font payload to ~72KB (MonaSans 348KB + Inter 874KB removed; Clash 29KB + Satoshi 42KB added). NEW `soroban-logo.webp` already present from round 1; `rumblefish-logo.svg`→`.webp`. |
| 5.1 | 404 `<main>` + NotFound h1 | TODO | **needs re-check (unchanged in code)** | NotFoundState.tsx got only a `py` prop (`fce0d666`) — NO `<h1>` added. F-W6-NOTFOUND-1 open. Catch-all uses `errorElement` (RouteErrorBoundary); AppShell `<main>` unchanged from round 1. F-E-3 still needs live re-verify. |
| 5.2 | URL tab state (Contract + LP chart) | TODO | **unchanged** | PoolCharts.tsx touched (49 lines) but metric/period still `useState`, not URL. F-EX-2 open. |
| 5.3 | Composite NotFound query-firing | TODO | **unchanged** | Sub-section hooks not gated with `enabled`. |
| 5.4 | Cross-entity link gaps | TODO | **unchanged** | NFT contract-id, home ledger-hash links not addressed. |
| 6.4 | ADR + doc-sync (incl. F-AN-6 toggle) | TODO | **unchanged + still complicated** | NetworkToggle still decorative (see §F-DP-1). F-AN-6 doc must still cover the non-functional toggle. |
| 7.1 | Wave 6 visual polish micro-batch | PARTIAL | **PARTIAL (slight advance)** | NEW: PoolParticipants share% *attempted* (see §F-W6-E13-1 — illusory), contract breadcrumb fixed. NOT closed: F-W6-E2-2 typo (still "All operations type", TransactionFilters.tsx:90), F-W6-E2-1 heading ("Transactions list", line 78), F-W6-CH-1 badge icon. STILL REGRESSED: F-AK-1 hex (5 hardcoded), F-AK-2 z-index. Do NOT mark DONE. |
| 7.2 | Live indicator freshness + health | TODO | **unchanged** | LiveIndicator.tsx touched (11 lines, cosmetic); still static, no `useLiveStatus`. DM-1 open. |
| 7.3 | Pool participants share % precision | TODO | **PARTIAL — illusory fix, see §** | `PoolParticipants.tsx:58` now `formatAmount(row.share_percentage, 2)`. BUT `formatAmount`'s 2nd arg is **minDecimals (padding), NOT rounding** — full precision still renders if API sends a raw float. Genuinely fixed ONLY if API pre-rounds to 2dp. NEEDS live check. |
| 7.4 | Filter slot a11y | DONE (stale) | **unchanged** | Already-fixed pre-merge; round 2 only added theme-token form. |
| 7.5 | NFT detail heading hierarchy | TODO | **unchanged** | NftMetadata.tsx/NftSummary.tsx touched but section labels still not `component="h2"`. F-W6-E11-1 open. |
| 7.6 | Header polling de-dup | TODO | **unchanged** | Not addressed. |
| 7.7 | Route transition loading indicator | TODO | **unchanged** | Not added. |
| 8.3 | Responsive redesign | PARTIAL | **unchanged** | RESPONSIVE-1/2 already RESOLVED (2026-05-28 live). Round 2 added NO hamburger, NO touch-target work, NO search-overflow fix. Residual (C11.5/11.6/11.7) all untouched. |
| 8.7 | Sort-caret designer sign-off | TODO | **unchanged (artifact already changed round 1)** | ExplorerTable touched again; caret still the round-1 circular-badge `SortableHeader`. Sign-off still pending. |
| 10.3 | Pool-id href re-enable | TODO | **unchanged** | `PoolDetailHeader.tsx:44` still `linked={false}`. F-AB-3 open. |
| 11.1 | NetworkToggle non-functional | TODO | **unchanged (NOT fixed)** | config.ts untouched; AppShell still local `useState<Network>` flowing only back to TopNav; not surfaced on `/`. See §F-DP-1. |
| 11.2 | AssetIcon hardcoded hex | TODO | **unchanged (NOT fixed)** | `AssetIcon.tsx:28` still `bg: '#724311', fg: '#fffcc2'`. 5 hardcoded hex total. See §F-DP-2. |
| 11.3 | Raw z-index → scale | TODO | **unchanged (NOT fixed)** | Raw `zIndex: 2` still in TopNav:99, SecondaryNav:35, Footer:63; `zIndex: 1` AppShell:184, HomePage:69; `zIndex: 0` HomePage:35, PageGridBackdrop:26. See §F-DP-3. |
| 11.4 | OperationFlowTree collapse/expand | TODO | **unchanged (NOT restored)** | OperationFlowTree.tsx rewritten (202 lines in merge) but renders FLAT — no `useState`/`Collapse`/chevron. `defaultExpanded` is now a dead/unused prop on the `FlowNode` interface. See §F-DP-4. |
| 11.5 | Hamburger nav <768px | TODO | **unchanged (NOT added)** | No hamburger / `MenuIcon` / `Drawer` / `aria-label="Open menu"` anywhere. SecondaryNav still `overflowX:{xs:'auto'}` scroll-nav. |
| 11.6 | Touch targets ≥44px | TODO | **unchanged** | No sizing pass. |
| 11.7 | Search page overflow <660px | TODO | **unchanged (NOT fixed)** | Search/* changes are theme-token-form refactors ONLY (`sx={{color:'text.tertiary'}}` → `sx={(t)=>({color:t.palette.text.tertiary})}`); no width/flex/min-width:0 structural change. The ~628px category card is untouched. NEEDS live re-verify. |
| All other cards (6.1–6.3, 8.1/8.2/8.4–8.8, 9.1, 10.1/10.2) | various | TODO | **unchanged** | Out of round-2 scope. |

---

## 2. Round-1 open items — did round 2 close any? (the headline check)

| Round-1 open item | Card | Round-2 verdict | Evidence |
|---|---|---|---|
| **F-DP-1 NetworkToggle non-functional** | 11.1 | **NOT closed** — still fake | `web/src/api/config.ts` unchanged (static `apiBaseUrl` from `VITE_API_BASE_URL`, no `network` read). `queryKeys.ts` `network` set is endpoint-grouping, NOT per-network namespacing. AppShell:86 `useState<Network>`; `setNetwork` flows only into TopNav rendering. TopNav still hidden on `/` (`AppShell.tsx:141 {!isHome && <TopNav`). |
| **F-DP-2 AssetIcon hardcoded hex** | 11.2 | **NOT closed** — still 5 hex | `AssetIcon.tsx:28` `sac` returns inline `'#724311'`/`'#fffcc2'` (these ARE token values per `colors.ts:91` but inlined, not bound to `theme.palette`). `ContractInterface.tsx:36` `TYPE_REF_COLOR='#155dfc'` retained. assetColor.ts touched (6 lines) but that file already uses `colorsLight.*` tokens — it is NOT the hardcoded-hex regression site. |
| **F-DP-4 OperationFlowTree collapse/expand** | 11.4 | **NOT restored** — still flat | `OperationFlowTree.tsx` (the full file): top-level export is just `renderNodeList(nodes)`; children render via static indented `borderLeft` dashed connectors. No `useState`, no `Collapse`, no chevron. `defaultExpanded?: boolean` on `FlowNode` (line 44) is now an unreferenced dead prop. (Verify-vs-Figma still required.) |
| **F-W6-RESPONSIVE-3 hamburger** | 11.5 | **NOT added** | "responsive nav tweaks" in `fce0d666` = TopNav stats `overflowX:auto` + SecondaryNav scroll-nav (already from round 1). grep for hamburger/MenuIcon/Drawer/`aria-label="Open menu"` = zero hits. |
| **F-W6-RESPONSIVE-4 touch targets** | 11.6 | **NOT addressed** | No min-height/min-width enlargement on pagination/nav. (105/106 <44px from 2026-05-28 live still stands.) |
| **F-W6-RESPONSIVE-5 search overflow** | 11.7 | **NOT fixed** | search/* diff is theme-token-form only (no layout change). Needs live re-verify of `/search` @375. |
| **/contracts real list page** | 1.3 | **NOT closed** — still stub | `router/index.tsx:66` `element: <PageStub title="Contracts" path="/contracts" />`. |
| **/accounts real list page** | 1.3 | **CLOSED** ✅ | `AccountsListPage.tsx` is a real list (cursor pagination, filters, sort, empty/error/loading states via `DataListCard`). `useAccountsList` hook over `getAccountsList` generated client. |
| **F-P-1 lint warning** | 3.1 | **NOT fixed** | Live `nx lint` → `assetColor.ts:131:10 warning Forbidden non-null assertion`. |

**Net: 1 of 8 round-1 open items closed** (the `/accounts` half of card 1.3).

---

## 3. NEW closures / advances introduced by round 2

| Item | Finding(s) | Verdict | Evidence |
|---|---|---|---|
| **Real `/accounts` list page** | F-A-5 Gap 1 (accounts half), Archaeology Rec 2 (accounts half) | **DONE (half of card 1.3)** | `fce0d666`: `AccountsListPage.tsx`, `useAccountsList.ts`, `accounts/AccountsTable.tsx`, `accounts/AccountsFilters.tsx`, route wired `router/index.tsx:48`. |
| **Contract-detail breadcrumb fix** | round-1 lower-severity regression note ("breadcrumb became `Account/<deployer>`") | **RESOLVED** | `39aafc49`: ContractDetailPage breadcrumb now `Contracts / <id>` (was `Account / <deployer>` once the contract loaded — matched neither route nor id). Closes a round-1-introduced bug. |
| **Font migration** | F-AI-* adjacent (load weight) | **net positive (not a tracked finding)** | `fonts.css`: Mona Sans/Inter (TTF, 348KB+874KB) → Clash Display/Satoshi (woff2, 29KB+42KB). Whole-app typography changed → live visual re-verify warranted. |
| **`bodyMonoXsMedium` typography variant** | round-1 latent bug | **fix** | `39aafc49`: variant was declared in type augmentation + used by EmptyState but had no actual style (meta lines fell back to MUI defaults). Now defined (12px/500). |
| **EmptyState / error-states restyle** | cosmetic (B-figma parity) | **visual, not a card closure** | EmptyState 125 lines + Generic/NotFound/RateLimit/Transient error states each +3. Visual polish; does NOT add the shared skeleton primitives card 2.3 wants. |

---

## 4. Illusory / questionable fix — read carefully

### F-W6-E13-1 (card 7.3) — Pool participants share % precision: **NOT genuinely fixed**

`PoolParticipants.tsx:58` now calls `formatAmount(row.share_percentage, 2)`. But `formatAmount(value, minDecimals)` (`web/src/pages/format.ts:12`) treats the 2nd arg as **minimum decimal padding**, NOT rounding — it trims trailing zeros and pads UP to `minDecimals`, but never caps precision. So a raw `33.3333333333` still renders `33.3333333333%` (the exact bug F-W6-E13-1 reported). It only looks fixed.

- **If** the API serializes `share_percentage` as a pre-rounded 2dp string → effectively fine.
- **If** the API returns a raw float / high-precision decimal → still broken.
- **Action:** confirm the live value at `/liquidity-pools/:id` participants table, or switch to `.toFixed(2)` / a true max-decimals formatter. Card 7.3 stays open until live-confirmed.

### ContractDetailPage events count — still a known mislabel

`39aafc49` left the Events tab count pinned to `recent_unique_callers` (callers ≠ events) and added a code comment + flagged it in the FE→API gaps doc. Not fixed (no real events-count metric on `ContractStats`). The round-1 "possible mislabel" note is now an explicitly-acknowledged placeholder — track under card 6.3 (backend coordination) or card 7.1.

---

## 5. NEW regressions introduced by round 2

**None structurally severe.** Round 2 did not re-break anything the audit had closed. Notes:

1. **Whole-app font swap (Clash Display + Satoshi).** Not a regression per se, but every text surface changed font. Layout/line-height/truncation across all 14 routes should be live re-verified (heading metrics differ from Mona Sans; mono/body from Inter). woff2 is a load win.
2. **EmptyState + 4 error-state components restyled.** Visual re-verify on empty/error/404/rate-limit/transient states.
3. **`web/src/pages/detail/` folder grew** (NEW `DataListCard.tsx`, `KpiCell.tsx`, `PageHeader.tsx`). Mildly enlarges card 2.2's hoist scope (more files to move to `libs/ui`), but these are genuinely shared detail primitives — arguably they belong in the same hoist.
4. **No F-DP-5+** — the round-1 F-DP-1..4 regressions persist unchanged; no brand-new debt items rise to the F-DP-* bar.

---

## 6. Cards that should flip STATUS (for review — DO NOT auto-apply)

1. **Card 1.3** PARTIAL → **PARTIAL (annotate "accounts half DONE")**. Add note: "`/accounts` is now a REAL list page (`AccountsListPage` + `useAccountsList`, `fce0d666`). F-A-5 Gap 1 accounts half DONE; `/contracts` half still TODO (router/index.tsx:66 still `<PageStub>`). Card stays PARTIAL until `/contracts` real list ships." Sub-checklist: mark the accounts-related items done; contracts items remain.
2. **Card 2.2** — keep TODO; **update note**: PageStub now has **1** consumer (`/contracts` only; `/accounts` graduated). F-AH-1 still FALSE while contracts is a stub; PageStub deletion still gated behind the contracts half of card 1.3. ALSO add NEW `detail/DataListCard.tsx`/`KpiCell.tsx`/`PageHeader.tsx` to the hoist target list.
3. **Card 7.1** PARTIAL → **PARTIAL (annotate)**. Add: "Round 2 (`fce0d666`/`39aafc49`): breadcrumb regression fixed; share% *attempted* (illusory — see impact doc §4). Still NOT closed: F-W6-E2-2 typo, F-W6-E2-1 heading, F-W6-CH-1. Still REGRESSED: F-AK-1 hex, F-AK-2 z-index."
4. **Card 7.3** TODO → **PARTIAL (annotate "illusory — verify live")**. `formatAmount(x, 2)` uses minDecimals, not rounding; needs live confirm or `.toFixed(2)`.
5. **Card 11.4** — keep TODO; confirm rewrite kept it FLAT (still needs Figma sign-off / restore decision). `defaultExpanded` is now dead.
6. **Appendix:** annotate **F-A-5 Gap 1** "accounts half DONE via `fce0d666`; contracts half open". **F-W6-E13-1** "attempted `fce0d666` but minDecimals ≠ rounding — verify live". No appendix row should flip to RESOLVED purely on round 2.

**No card flips to DONE.**

---

## 7. Live-Playwright re-verify queue (REQUIRED before any DONE claim)

Round-2-specific (judged from code only):

1. **`/accounts` list page** — load `/accounts` at 1280/768/375: rows render, filters (search/sort/with-domain) work, pagination prev/next, empty + error + loading states. Confirms card 1.3 accounts half.
2. **F-W6-E13-1 share %** — `/liquidity-pools/:id` participants table: read an actual fractional `share_percentage` value. If it shows >2 decimals, card 7.3 is NOT fixed (minDecimals trap).
3. **Font swap visual sweep** — all 14 routes: heading/body/mono render correctly with Clash Display + Satoshi; no overflow/clipping/truncation regressions from the metric change.
4. **EmptyState + error states** — trigger empty `/accounts?q=zzz`, a 404 detail route, a forced error: confirm restyled states render and NotFound still has no h1 (card 5.1 open).
5. **F-W6-RESPONSIVE-5 search overflow** — `/search?q=test` at 375: confirm `documentElement.scrollWidth` vs `clientWidth` — round 2 did NOT change search layout, so expect STILL overflowing (~644px). Confirms 11.7 open.
6. **NetworkToggle no-op** — non-home route, click Testnet: confirm zero data change (still decorative). Confirms 11.1 open.
7. **OperationFlowTree** — a tx with nested Soroban invocations: confirm flat render (no collapse affordance). Confirms 11.4 + feeds Figma sign-off.
8. **F-E-3 catch-all 404 `<main>`** — still open from round 1 (AppShell `<main>` unchanged); re-verify landmark.

Carry-over still-open from round 1 (unchanged by round 2): hamburger (11.5), touch targets (11.6).

---

## 8. Verdict counts (return)

- **Cards flipped DONE:** 0
- **Cards flipped / advanced PARTIAL:** 1 clean advance (card 1.3 accounts half) + annotations on 7.1, 7.3, 2.2.
- **Round-1 open items now closed:** 1 of 8 — only `/accounts` real list page (card 1.3 accounts half). NOT closed: F-DP-1 (NetworkToggle), F-DP-2 (AssetIcon hex), F-DP-4 (OperationFlowTree), RESPONSIVE-3/4/5, /contracts stub.
- **F-P-1 lint warning:** PRESENT (assetColor.ts:131, confirmed live `nx lint`).
- **NetworkToggle:** STILL FAKE (not wired, not removed, still invisible on `/`).
- **Accounts list:** REAL. **Contracts list:** still PageStub.
- **New regressions:** none severe; 1 illusory fix (share %), whole-app font swap + EmptyState/error restyle → visual re-verify; `web/src/pages/detail/` folder grew (mild card-2.2 scope creep).
- **Top 3 highest-value closures:** (1) real `/accounts` list page; (2) contract-detail breadcrumb fix (closes round-1 regression); (3) font→woff2 migration (load win).
