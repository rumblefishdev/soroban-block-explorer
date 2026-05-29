# Audit 0257 — Action Queue

**Date generated:** 2026-05-27
**Total cumulative findings:** ~281 (incl. resolved + skipped)
**Audit-blocker scope:** CLEARED (Gate A + Gate B + 0270 done)
**Closure approach:** Single elastic task `0XXX_FEATURE_audit-0257-closing` references this queue.

## How to use

This is the master action queue for closing audit 0257. Structure:

1. **Master cards (~50)** — clustered implementation units. Per 0251 batch model, one card = one PR scope. Cards organized per audit category (10).
2. **Per-card sub-checklist** — every finding the card closes, listed inline for verification during impl.
3. **Appendix — 281-finding STATUS table** — every finding the audit produced, with cluster cross-reference and STATUS. Catch-all for catalog-only / cosmetic findings not promoted to own card.

### Per-session workflow

1. Open this file
2. Scroll to next `STATUS: TODO` card (priority order: Category 1 first)
3. Read card Rationale + Scope + Sub-checklist
4. Implement
5. Mark `STATUS: DONE` (optionally `STATUS: IN-PROGRESS` if multi-session)
6. Commit citing card N.M + closed F-IDs
7. Auto-mark sub-checklist items (or manually verify each)
8. Update appendix STATUS column for all closed F-IDs (bulk-mark on commit)
9. Repeat

### Status values

- `TODO` — not started
- `IN-PROGRESS` — actively being worked
- `PARTIAL` — partially closed (e.g. one half of a finding landed, or code-verified but awaiting live verification); remaining scope still open. Do NOT mark `DONE` until the residual sub-items + any live re-verify clear.
- `DONE` — landed on develop
- `SKIP` — explicit user decision to skip, rationale in Notes
- `DEFER-M2` — push to milestone 2 (or `-M3`, `-LATER`)
- `RESOLVED` (appendix only) — already-shipped in pre-queue Gate A/B/0270 batches; cite SHA

> **design_parity merge note (2026-05-27, commit `06ab34cc`, merge `62c988d4`).** The `feat/design_parity` branch (Figma-parity + responsive pass) was merged into `research/0257_frontend-comprehensive-audit`. It was NOT authored to close audit cards — several apparent closures are PARTIAL, four findings REGRESSED, and one Wave 6 finding it appears to fix (filter a11y, card 7.4) was already fixed pre-merge (stale). Full per-card/per-finding verdicts: `design-parity-impact-2026-05-27.md`. Every PARTIAL flagged below requires live Playwright re-verify — see **"Pending live verification (design_parity)"** block below — before promotion to DONE.

## Pending live verification (design_parity)

These items were judged from **code inspection only** (no live Playwright run in the design_parity impact analysis). They are marked `PARTIAL` in cards/appendix and MUST be confirmed in a live run at **375px + 768px** viewports before any flip to `DONE`. Source: `design-parity-impact-2026-05-27.md` §Live-Playwright re-verify queue.

- [ ] **All 14 routes × 375/768 responsive cells** — confirm `document.documentElement.scrollWidth === clientWidth` (no page-level horizontal scrollbar) on every route. THE gating check for the responsive matrix (F-W6-RESPONSIVE-1). **Partial live re-verify 2026-05-29: `/search` @375 VERIFIED PASS (scrollWidth 364 ≤ 375 — last remaining route-overflow, F-W6-RESPONSIVE-5, now mitigated); desktop 1280 sweep VERIFIED no regressions (9 routes clean, scrollWidth 1269).**
- [ ] **Embedded/list table overflow** — confirm tables scroll within their own container and do NOT push page width on E1–E8/E10/E12/E13 (F-W6-RESPONSIVE-2).
- [ ] **Touch targets ≥44px** — measure nav, copy buttons, pagination prev/next at 375 (F-W6-RESPONSIVE-4).
- [x] **Catch-all 404 `<main>` landmark** — **VERIFIED BROKEN (live 2026-05-29)** — catch-all 404 (`/foobar`) has NO `<main>` (`hasMain: false`) AND no h1 after the AppShell `<main>` restructure. F-E-3 + F-W6-NOTFOUND-1 confirmed open. Card 5.1 stays TODO.
- [ ] **Home KPI grid** — confirm KPI 2×2 grid + hero wrap render without overflow at 375; confirm TopNav hidden-on-home does not break header search/network affordance expectations.
- [ ] **SecondaryNav scroll-nav** — confirm horizontal-scroll nav is usable at 375, and decide whether it substitutes for the hamburger (F-W6-RESPONSIVE-3 / 0059).
- [x] **NetworkToggle no-op confirm** — **VERIFIED FAKE (live 2026-05-29)** — on `/transactions`, clicking Testnet flips `aria-pressed` only; no URL/banner/refetch; only request is the LiveIndicator poll to the same Mainnet host. Pure decorative; still invisible on `/`. F-DP-1 / card 11.1 stays TODO.

### Pending live verification — design_parity ROUND 2 (PR #224, merge `35ac27c0`, 2026-05-29)

Added from `design-parity-impact-2026-05-29.md` §7 (live re-verify queue). **ALL VERIFIED live 2026-05-29** — verdicts below; see `design-parity-impact-2026-05-29.md` §Live re-verify 2026-05-29 for full evidence.

- [x] **/accounts list page functionality** — **VERIFIED PASS (live 2026-05-29)** — 20 rows, sort/search/with-domain filters, cursor pagination (`?cursor=20`), row→detail links, empty state all work. Confirms card 1.3 accounts half DONE.
- [x] **share-% actual precision at `/liquidity-pools/:id`** — **VERIFIED ILLUSORY (live 2026-05-29, stays open)** — pool `LD5MMO2Q…` renders `33.3333333333333333%` raw; `formatAmount(_, 2)` minDecimals ≠ rounding. Card 7.3 STAYS TODO.
- [x] **font swap visual sweep across 14 routes** — **VERIFIED PASS (live 2026-05-29)** — Clash Display / Satoshi / JetBrains Mono all `status: "loaded"` on every route sampled; no FOUT/missing-glyph/fallback; no overflow from the metric change (desktop scrollWidth 1269 ≤ 1280).
- [x] **EmptyState + 404 state restyle visual check** — **VERIFIED (live 2026-05-29)** — EmptyState + 404 restyle render styled, but NotFound still has NO h1 (see card 5.1, stays open).
- [x] **OperationFlowTree flat render vs Figma** — **VERIFIED flat (live 2026-05-29, data-limited)** — confirmed flat render (0 expand/collapse, no chevron); nested-tree verify blocked by local data (0 soroban/multi-op txs). See card 11.4; Figma sign-off still pending.
- [x] **/search @375 page overflow** — **VERIFIED PASS (live 2026-05-29)** — `documentElement.scrollWidth = 364 ≤ 375`, NO page overflow (RESPONSIVE-5 reclassified RESOLVED — page overflow gone; category-card row scrolls within `overflow-x:auto` container). See card 11.7.
- [x] **Desktop (1280) regression sweep** — **VERIFIED none (live 2026-05-29)** — 9 routes clean (home, transactions, tx-detail, accounts, ledgers, nfts, nft-detail, pool-detail, search); no page overflow (scrollWidth 1269), fonts loaded, no breakage from R2's heavy pool/nft/ledger touches.

> **Live re-verify 2026-05-29 — general note.** No new regressions from design_parity R1+R2 confirmed live (desktop sweep 9 routes clean). **Data limitation:** local dataset has 0 soroban / 0 multi-op txs (all 38 single-op) — blocks full OperationFlowTree nested-tree verify (card 11.4). Full evidence + verdict table in `design-parity-impact-2026-05-29.md` §Live re-verify 2026-05-29.

## Excluded from this queue (background only)

- Already-resolved on develop (Gate A + Gate B + 0270): F-D-2, F-AE-5, F-K-2/3/9, F-AN-8, F-CO-1, F-L-1, F-K-4, NFT search-404 regression, F-E-1, F-E-8 — all marked `RESOLVED` in appendix.
- Already-resolved via 0254 merge: F-D-1 (live API stale binary).
- Already-resolved via FilipDz tx-detail PR #215 (a2c1b205): A1 (TxDetail stub), F-K-1 (tx detail outbound links).
- User-dropped: F-E-2 (URL wire contract), Muxed M→G + Asset composite redirect (no precedent) — marked `SKIP` in appendix. SearchResponse::Redirect refactor — was SKIP "deferred future PR" but **shipped by 0271 commit `5d7484b1`** (FE owns singleton classification; wire collapsed to `Results` only). Re-classified `RESOLVED` in appendix.
- Already-spawned tasks on develop: 0262/0263/0264/0265 (Gate B batch archived), 0270 (search canonical archived), 0271 (search broad enhancement active).

---

## Category 1 — Pre-launch must-fix (cannot ship without)

### 1.1 Footer legal + external links wiring

- **Type:** FEATURE
- **Effort:** ~2h (if hrefs available) / ~30min (if hiding)
- **Severity / Class:** 🟠 C
- **Pre-launch:** MUST
- **STATUS:** TODO

**Rationale.** The footer renders Terms of Service, Privacy Policy, Cookies, and external Resources links (GitHub, Stellar docs, Soroban docs, Stellar dashboard) as plain `<span>` elements with no `href`. Shipping a public block explorer with non-functional Terms/Privacy is a legal/compliance liability. Resources are a discoverability gap. Even the project's own GitHub link is missing. This was already flagged as Gate B fix-first but deferred to this queue.

**Scope.** Edit `libs/ui/src/layout/Footer.tsx`. Either (a) fill in real hrefs for all 7 items — needs legal team content for Terms/Privacy/Cookies — or (b) hide dead `<span>` items entirely until content ready. External links must use `target="_blank" rel="noopener noreferrer"` per F-H-5 pattern.

**Findings closed (sub-checklist):**

- [ ] CA-1 — Terms of Service / Privacy Policy / Cookies render as dead `<span>` (no href)
- [ ] CA-2 — Resources (GitHub / Stellar docs / Soroban docs / Stellar dashboard) render as dead `<span>` (no href)
- [ ] CA-3 — When wiring external links, ensure `target="_blank" rel="noopener noreferrer"`
- [ ] F-W6-E0-1 — Wave 6 re-confirmed dead spans across all 14 routes

**Notes:** User decision required between path (a) and (b).

---

### 1.2 Build SHA / version stamp in UI

- **Type:** FEATURE
- **Effort:** ~1h
- **Severity / Class:** 🟠 D
- **Pre-launch:** MUST
- **STATUS:** TODO

**Rationale.** No build version or commit SHA is displayed anywhere in the UI. Post-launch debugging ("which build is live?") becomes impossible. Standard practice for public explorers: a small footer line or tooltip with `vX.Y.Z @ <short-sha>` injected at build time.

**Scope.** Add `define: { __BUILD_SHA__: JSON.stringify(process.env.GITHUB_SHA ?? 'dev') }` to `web/vite.config.ts`. Surface in `libs/ui/src/layout/Footer.tsx` near copyright line. Wire `package.json.version` similarly. Update CI workflow to pass `GITHUB_SHA` env var to the build step.

**Findings closed (sub-checklist):**

- [ ] DN-1 — no build version / SHA displayed in UI
- [ ] DN-2 — no vite `define` block to inject build metadata

**Notes:** **\_**

---

### 1.3 Contracts list page + `/contracts` nav entry

- **Type:** FEATURE
- **Effort:** ~1d
- **Severity / Class:** 🔴 (launch-blocker per archaeology A3 / F-A-5 Gap 1)
- **Pre-launch:** MUST
- **STATUS:** PARTIAL
- **design_parity note:** `/contracts` + `/accounts` nav entries (`NAV_LINKS` in routes.ts) AND stub routes landed in `06ab34cc` (design_parity). F-A-5 Gap 1 **nav-link half DONE**; the real list-page half is still TODO — both routes currently render via `<PageStub>` placeholder, not a real list. PageStub is now the stub renderer for these two routes (see card 2.2 scope conflict).
- **design_parity R2 note (2026-05-29, PR #224, `fce0d666` / merge `35ac27c0`):** **`/accounts` is now a REAL list page** — `web/src/pages/AccountsListPage.tsx` + `web/src/api/hooks/useAccountsList.ts` + `accounts/AccountsTable.tsx` + `accounts/AccountsFilters.tsx` (cursor pagination, filters, sort, empty/error/loading states), route wired `router/index.tsx:48`. **`/contracts` STILL `<PageStub>`** (`router/index.tsx:66`). F-A-5 Gap 1 **accounts half DONE; contracts half still TODO.** Card stays **PARTIAL** until `/contracts` real list ships. **live re-verify 2026-05-29:** `/accounts` PASS (20 rows, sort/search/with-domain filters, cursor pagination `?cursor=20`, row→detail links, empty state all functional — accounts half now live-VERIFIED DONE); `/contracts` confirmed live stub ("implementation pending" PageStub, no h1/table). Card OVERALL stays PARTIAL. Source: `design-parity-impact-2026-05-29.md` §1, §2, §3, §Live re-verify 2026-05-29.

**Rationale.** Contract detail pages exist at `/contracts/:id` but are reachable only by deep link — no list page, no nav entry. Users browsing the explorer cannot discover any contract. Per Wave 1 archaeology this is a launch-blocker carried over from 0075's Future Work. F-A-5 spec/source consistency audit also flagged it.

**Scope.** Create `web/src/pages/ContractsListPage.tsx` mirroring the patterns of TransactionsListPage / AssetsListPage. Add route in `web/src/router/index.tsx`. Add nav entry to `libs/ui/src/layout/TopNav.tsx` `NAV_LINKS`. Wire `useContractsList` hook over the appropriate generated client method (verify endpoint exists; if not, spawn backend task first).

**Findings closed (sub-checklist):**

- [~] Archaeology Recommendation 2 — Contracts list + nav missing (nav added `06ab34cc`; **accounts list DONE `fce0d666`**; contracts list still missing)
- [~] F-A-5 Gap 1 — Contract detail unreachable by browsing (nav entry `06ab34cc`; **accounts list page DONE `fce0d666`**; contracts list page still TODO — partial)
- [~] 0075 Future Work — Contracts list page + `Contracts` entry in NAV_LINKS (NAV_LINKS entry DONE `06ab34cc`; **accounts list page DONE `fce0d666`**; contracts list page TODO)
- [x] **Accounts list page** — `/accounts` REAL list page (`AccountsListPage` + `useAccountsList` + `AccountsTable` + `AccountsFilters`, `fce0d666`, PR #224). **live re-verify 2026-05-29: /accounts PASS — 20 rows, sort/search/domain filters, cursor pagination (?cursor=20), row→detail links, empty state all work.** DONE (live-verified).
- [ ] **Contracts list page** — `/contracts` STILL `<PageStub>` (`router/index.tsx:66`). TODO — confirmed live stub 2026-05-29 ("Page implementation pending. Routing skeleton only.", no h1/table).

**Notes:** Verify backend endpoint exists. If not, spawn backend task as prereq. design_parity `06ab34cc` landed the nav-entry + stub-route half; **R2 `fce0d666` (PR #224) shipped the real `/accounts` list page** (PageStub → real `AccountsListPage`); remaining scope is the real `ContractsListPage` (PageStub still live for `/contracts`).

---

## Category 2 — Atomic refactor batches (per 0251 batch model)

### 2.1 Format + truncate + debounce unification batch

- **Type:** REFACTOR
- **Effort:** ~1d
- **Severity / Class:** 🟠 C
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** Per audit's #1 maintenance-cost finding (F-AD-1): a single "change how addresses truncate" today requires editing 6 files. Plus 2 STROOPS_PER_XLM constants, 2 formatFee implementations, 10 inline toLocaleString sites, 4 toFixed bypasses, 4 debounce-pattern reimplementations. All are organic accretion across feature task boundaries that each got self-consistency but no cross-task DRY check. Phase 3 single-PR consolidation cuts ~10 audit findings in one atomic change. Junior maintenance cost drops from "moderate" to "low".

**Scope.** Create `libs/ui/src/format/` directory with: `stroops.ts` (single `STROOPS_PER_XLM_BIGINT` + `stroopsToXlmString` + canonical `formatFee` + `formatStroops`), `numbers.ts` (`formatInteger`, `formatTps`, `formatPercent`). Extend `libs/ui/src/identifiers/truncate.ts` to expose all 6 ad-hoc truncate variants via canonical `truncateMiddle(value, type)`. Extract `useDebouncedDraft<T>(value, onChange, delay)` from existing `useDebounced.ts`. Migrate all consumers: 6 truncation sites + 10 toLocaleString sites + 4 toFixed sites + 2 STROOPS + 2 formatFee + 4 debounce sites. Delete duplicated impls.

**Findings closed (sub-checklist):**

- [ ] F-U-3 — 6 truncation re-impls (shortId/shortStr/shortHash/shortenStrKey/truncateHex + inline)
- [ ] F-U-4 — 2 STROOPS_PER_XLM constants (number + bigint variants)
- [ ] F-U-2 — 10 inline toFixed/toLocaleString sites bypass formatter
- [ ] F-J-2 — 10 `toLocaleString('en-US')` sites bypass formatAmount
- [ ] F-J-3 — 4 toFixed bypasses canonical formatter
- [ ] F-J-4 — STROOPS_PER_XLM constant single site no shared util (drift risk realised)
- [ ] F-J-7 — 6 truncation re-impls (cross-cite F-U-3)
- [ ] F-J-16 — Duplicate `formatFee` BigInt vs Number, 2 implementations
- [ ] F-J-17 — `formatStroops` introduced as 3rd entry point for stroop display
- [ ] F-Y-2 — Debounce pattern duplicated 4× across filter components
- [ ] F-Y-6 — Cross-cite formatter/truncation findings (recap)
- [ ] F-AB-5 — 6 cross-task formatter/truncation duplications (recap symptom)
- [ ] F-AD-1 — Leaked-concern bug fixes requiring 5+ files
- [ ] F-AN-7 — Stroop/XLM conversion in 2 places (recap of F-U-4)
- [ ] F-Z-1 — Multiple formatter homes (recap)
- [ ] J-3 — TopNav.formatNumber duplicate of formatCompactAmount

**Notes:** **\_**

---

### 2.2 Folder structure rationalization batch

- **Type:** REFACTOR
- **Effort:** ~3h
- **Severity / Class:** 🟡 C
- **Pre-launch:** SHOULD
- **STATUS:** TODO
- **design_parity note:** SCOPE CONFLICT — `06ab34cc` (design_parity) **revives `PageStub`** as the render target for the new `/accounts` + `/contracts` stub routes. F-AH-1 ("PageStub dead orphan, delete") is now **FALSE** — PageStub has 2 live consumers. The "Delete `web/src/pages/PageStub.tsx` dead orphan" line is removed from scope below; PageStub deletion is now **gated behind card 1.3** shipping real `ContractsListPage` / `AccountsListPage`. but are used universally — they belong in `libs/ui`. The `web/src/search/` parallel top-level folder is structural debt. `web/src/utils/` is a single-file folder. `assetLegLabel` cross-folder reach couples sibling page folders. One refactor PR fixes all five.
- **design_parity R2 note (2026-05-29, PR #224, `fce0d666` / merge `35ac27c0`):** PageStub now has **1** live consumer (`/contracts` only — `/accounts` graduated to a real page, `fce0d666`); F-AH-1 stays FALSE while `/contracts` is a stub, and PageStub deletion remains gated behind the contracts half of card 1.3. R2 also **grew the `web/src/pages/detail/` folder** — NEW `detail/DataListCard.tsx`, `detail/KpiCell.tsx`, `detail/PageHeader.tsx` — mildly enlarging this card's hoist target (more shared detail primitives to move to `libs/ui`). Scope still valid, slightly larger. Source: `design-parity-impact-2026-05-29.md` §1 (card 2.2), §5.3.

**Scope.** Hoist `web/src/pages/detail/{SectionCard,PageBreadcrumb,SummaryRow}.tsx` → `libs/ui/src/layout/`. Add to `libs/ui/src/index.ts` barrel. Update 16+ consumer imports. Delete the `web/src/pages/detail/` folder. Move `assetLegLabel` + `classifyLpTx` to `web/src/pages/liquidity-pools/shared/`. Hoist `GlobalSearchBar` to `libs/ui/src/layout/` and move `web/src/search/` body into `web/src/pages/search/`. Move `web/src/pages/{cursorParams,format,url}.ts` to `web/src/pages/_shared/`. ~~Delete `web/src/pages/PageStub.tsx` (dead orphan post tx-detail merge).~~ **[design_parity `06ab34cc`: PageStub REVIVED as `/accounts` + `/contracts` stub — no longer deletable; deletion gated behind card 1.3 real pages.]**

**Findings closed (sub-checklist):**

- [ ] F-U-1 — SectionCard wrong home (web/src/pages/detail/ instead of libs/ui)
- [ ] F-AH-3 — Same as F-U-1, restated
- [ ] ~~F-AH-1 — `web/src/pages/PageStub.tsx` dead orphan post-tx-detail merge~~ **STALE/FALSE post-`06ab34cc`: PageStub now has 2 live consumers (`/accounts`, `/contracts` stubs). Deletion gated behind card 1.3.**
- [ ] F-AH-2 — Folder asymmetry across feature areas
- [ ] F-AH-5 — `web/src/pages/detail/` only has 3 files (resolves with hoist)
- [ ] F-AH-7 — `web/src/search/` parallel top-level folder
- [ ] F-AH-8 — Page-root helpers (`cursorParams`/`format`/`url`) mixed with `*Page.tsx`
- [ ] F-AH-4 — `web/src/utils/` single-file folder (`poolIdStrkey.ts`)
- [ ] F-X-1 — `assetLegLabel` cross-folder reach `liquidity-pools/` → `pool-detail/`
- [ ] F-X-2 — `web/src/pages/detail/` single-file folder (recap F-U-1)
- [ ] F-X-5 — `web/src/utils/` 1-file (recap F-AH-4)
- [ ] F-U-2 (partial) — EmptyState reimplemented locally per page (covered by component reuse)

**Notes:** PageStub deletion uses `mv .trash/` per project policy — **but only after card 1.3 ships real `ContractsListPage` / `AccountsListPage`** (design_parity `06ab34cc` revived PageStub as the live stub for those two routes; deleting it now would break `/accounts` + `/contracts`).

---

### 2.3 Component reuse — EmptyState + LoadingState primitives

- **Type:** REFACTOR
- **Effort:** ~3h
- **Severity / Class:** 🟡 C
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** Empty / loading / retry states are reimplemented per page rather than consumed from `libs/ui`. Wave 6 loading-pattern audit identified inconsistent skeleton-vs-spinner choices, no shared `<TableSkeleton>` / `<SectionSkeleton>` primitive, silent polling refresh, no distinct retry state. Consolidating into shared primitives reduces cross-page visual drift.

**Scope.** Extend `libs/ui/src/states/` with: `<TableSkeleton rows={N}>`, `<SectionSkeleton>`, `<LoadingState variant="inline|overlay|full">`, `<RetryingState attempt={N} max={N}>`. Migrate consumers. Add subtle polling-refresh pulse to `LIVE` pills (paired with card 7.2).

**Findings closed (sub-checklist):**

- [ ] F-U-5 — Minor component-reuse violation
- [ ] F-W6-AP-1 — Loading pattern inconsistency: skeleton vs spinner choice not codified
- [ ] F-W6-AP-3 — Error retry has no distinct "retrying" state
- [ ] F-W6-AP-4 — Inline vs overlay vs full-page loading not standardised

**Notes:** **\_**

---

## Category 3 — Type-safety

### 3.1 `noUncheckedIndexedAccess` flag enable + fix-up

- **Type:** REFACTOR
- **Effort:** ~3h
- **Severity / Class:** 🟠 A
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** The single biggest type-safety upgrade missing from `tsconfig.base.json`. Today `arr[i]` returns `T` instead of `T | undefined`, allowing index-out-of-bounds bugs to compile. The lint warning at `assetColor.ts:131` (forbidden non-null assertion) exists precisely because the author reached for `!` to silence an ambiguity this flag would have caught honestly. Defer-during-audit decision lifts now that audit is closed.

**Scope.** Enable `noUncheckedIndexedAccess: true` in `tsconfig.base.json`. Expect 10-50 new errors; most are 1-line `?? fallback` additions. Fix the `assetColor.ts:131` non-null assertion as bonus. Verify `nx typecheck` green.

**Findings closed (sub-checklist):**

- [ ] F-AQ-1 — `noUncheckedIndexedAccess` disabled
- [ ] F-AQ-2 — `exactOptionalPropertyTypes` disabled (bundle in same PR if cheap)
- [ ] F-P-1 — Lint warning at `assetColor.ts:131` forbidden non-null assertion

**Notes:** May reveal hidden bugs — review each fix-up site carefully.

---

### 3.2 Branded ID types via type-guarded validators

- **Type:** REFACTOR
- **Effort:** ~3h
- **Severity / Class:** 🟠 D
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** `AccountId`, `ContractId`, `AssetId`, `LedgerSequence`, `PoolId`, `TransactionHash`, `NftId` are all plain `string` at type level. Today `routes.account(contractId)` compiles even though the URL would be malformed. Bumping existing validators in `libs/ui/src/identifiers/validators.ts` to type-guards (`is AccountId`) retrofits nominal typing without code rewrite.

**Scope.** Define `Brand<T, B>` helper in `libs/ui/src/identifiers/branded.ts`. Bump each validator from `(v: string): boolean` to `(v: string): v is XxxId` form. Thread branded types through `routes.ts`, `useParams` consumers, hook arg signatures. Add `isAssetId` / `isNftId` shape-aware validators (currently fall through to `value.length > 0`).

**Findings closed (sub-checklist):**

- [ ] F-AQ-4 — Zero branded / nominal types for ID strings
- [ ] C-5 — Missing `isAssetId` / `isNftId` validator (asset polymorphic gap)

**Notes:** Pairs with future router-param-validation work (0067 deferred AC).

---

### 3.3 Switch exhaustiveness + assertNever helper

- **Type:** REFACTOR
- **Effort:** ~1h
- **Severity / Class:** 🟡 D
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** Project has 4 switch statements over string-literal unions; none use an `assertNever` exhaustiveness assertion. Currently saved by `noImplicitReturns` for return-typed switches, but a future void switch over a new union member would silently miss a branch.

**Scope.** Add `libs/ui/src/utils/assertNever.ts` exporting `assertNever(x: never): never`. Adopt in the 4 existing switches over string-literal unions (`useSearchResults.ts`, `usePoolChart.ts`, `OperationFlowTree.tsx`, `validators.ts`) plus Filip's 2 new switches in `HighlightedJson.tsx` + `humanizeOp.ts`.

**Findings closed (sub-checklist):**

- [ ] F-AQ-3 — 4 switches, 3 exhaustive, 1 implicit-fallback; no `assertNever`

**Notes:** **\_**

---

## Category 4 — Performance

### 4.1 Bundle size + LP chart lazy + vendor split

- **Type:** REFACTOR
- **Effort:** ~1d
- **Severity / Class:** 🟠 D
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** Main bundle is 583KB / ~184KB gz — above Vite's 500KB warning. LP detail chunk is 300KB / ~94KB gz (eager-loads `@mui/x-charts` for TVL/Volume/Fees charts that the user may never click). No vendor chunk split → cache efficiency suffers across deploys. `@mui/utils` is triple-versioned (7.3.9, 9.0.0, 9.0.1). Bundle visualizer not installed, so per-source attribution requires manual work.

**Scope.** Add `rollup-plugin-visualizer` dev-dep + CI artifact upload. Add `manualChunks` in `web/vite.config.ts` for `react-vendor`, `mui-vendor`, `tanstack-vendor`. Lazy-load `PoolCharts` inside `LiquidityPoolDetailPage` so chart code loads on tab activation. Coordinated MUI 7→9 bump to eliminate `@mui/utils` triplication (separate concern — see card 10.2).

**Findings closed (sub-checklist):**

- [ ] F-AI-1 — Main bundle 594KB / 189KB gz exceeds Vite 500KB warning
- [ ] F-AI-2 — `LiquidityPoolDetailPage` 313KB / 95KB gz, chart heavy
- [ ] F-AI-3 — `SearchOutlined-*.js` 67KB stand-alone chunk (anomaly worth visualizer)
- [ ] F-AI-7 — No `vite-bundle-visualizer` in deps; no CI bundle gate
- [ ] F-AI-8 — No vendor chunk split
- [ ] F-AI-10 — TxDetail chunk 29.97KB / 9.13KB gz (Filip's PR; baseline)
- [ ] F-W6-AG-1 — Main bundle still > 500KB / Vite warn (recap)
- [ ] F-W6-AG-2 — LP detail chunk still 300KB (recap)

**Notes:** F-AI-4 / F-AI-9 / F-AI-11 informational; do not need own fix. **design_parity R2 (2026-05-29, PR #224, merge `35ac27c0`) — font swap to woff2 (net POSITIVE):** R2 migrated fonts TTF→woff2 (Mona Sans 348KB + Inter 874KB removed; Clash Display ~29KB + Satoshi ~42KB added) — **~1.08MB load reduction** (~1.15MB TTF → ~72KB woff2). No manualChunks / lazy LP chart / visualizer added (those remain TODO). Positive, but **needs visual re-verify across 14 routes** (Clash Display headings + Satoshi body/mono — metrics differ, watch overflow/clipping/truncation). Source: `design-parity-impact-2026-05-29.md` §1 (4.1), §3, §5.1.

---

## Category 5 — Routing leftovers

### 5.1 Catch-all 404 `<main>` landmark + NotFound h1 normalization

- **Type:** BUG
- **Effort:** ~1h
- **Severity / Class:** 🟡 C
- **Pre-launch:** SHOULD
- **STATUS:** TODO
- **design_parity note:** `06ab34cc` restructured AppShell's `<main>` (now wraps `<Outlet/>` inside a relative Box). Catch-all 404 routing itself was NOT touched in the diff, and NotFound h1 was NOT touched. **Re-verify F-E-3 landmark still holds after the AppShell refactor** (see "Pending live verification" block). No status change yet — scope unchanged, but live re-check required before DONE. but bypasses the `AppShell` `<main>` landmark — screen readers skip the page main, selector tests break. Additionally, NotFound pages on 4 of 5 detail routes lack an `<h1>` element (only `/contracts/<invalid>` has one). SR users navigating by heading shortcut land mid-content. Two small a11y fixes in one PR.

**Scope.** Wrap catch-all 404 in `AppShell` `<main>` landmark. Update `libs/ui/src/states/errors/NotFoundState.tsx` to render an `<h1>` (entity-typed). Verify all detail-route NotFound paths use the canonical state component.

**Findings closed (sub-checklist):**

- [ ] F-E-3 — Catch-all 404 `<main>` landmark gap
- [ ] F-W6-NOTFOUND-1 — NotFound missing h1 on 4 of 5 detail routes
- [ ] F-W6-E3-3 — NotFound h1 inconsistency (cross-cite)
- [ ] F-W6-E5- — NotFound h1 inconsistency (cross-cite)
- [ ] F-W6-E6-2 — NotFound has no h1
- [ ] F-W6-E9-3 — h1 inconsistent on NotFound across detail routes
- [ ] F-W6-E13-2 — Pool NotFound has no h1
- [ ] F-D-3 — Detail page H1 heading inconsistency (partial — covers NotFound variant)

**Notes:** **live re-verify 2026-05-29:** catch-all 404 (`/foobar`) has NO `<main>` landmark (F-E-3) AND NO h1 (F-W6-NOTFOUND-1) — both confirmed broken post-AppShell restructure (`hasMain: false`, `headings: []`); account-404 also has no heading. EmptyState/404 restyle is visually styled but heading-less. Card STAYS TODO.

---

### 5.2 URL state for tabs (Contract + LP chart)

- **Type:** FEATURE
- **Effort:** ~1h
- **Severity / Class:** 🟡 D
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** `/contracts/:id` tabs (Interface/Invocations/Events) and `/liquidity-pools/:id` chart tabs (TVL/Volume/Fees) plus period (1D/7D/30D/1Y) don't persist in URL — refresh drops back to default. `useTabUrlState` exists in `libs/ui` already (used by ContractDetailPage:43 per the AL sweep, possibly partial). Pool chart `metric` and `period` are still useState. Deep-linking "show me this pool's volume over 7 days" should work.

**Scope.** Migrate `web/src/pages/pool-detail/PoolCharts.tsx` state (`metric`, `period`) to `useTabUrlState` or `useSearchParams` (whichever pattern is established). Confirm Contract tabs are fully URL-state (per AL sweep evidence). Document the convention in `lore/3-wiki/`.

**Findings closed (sub-checklist):**

- [ ] F-E-7 — No URL state for tabs (Contract Interface/Invocations/Events + LP chart)
- [ ] F-EX-2 — Pool chart metric/period in useState, not URL
- [ ] F-AL-1 (defer) — `selectedIndex` in tx-detail useState (defer; this is borderline-deliberate per F-AL-1 trade-off)

**Notes:** F-AL-1 left out of scope unless user confirms moving op-picker index to URL.

---

### 5.3 Composite NotFound — stop sub-section queries firing (Wave 6 partial-fix follow-up)

- **Type:** BUG
- **Effort:** ~1h
- **Severity / Class:** 🟡 A
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** Gate B fix closed the visual side of composite NotFound (sub-section render gated on `!parent.isError`), but Wave 6 confirmed sub-section queries STILL FIRE — producing extra 404 entries in the network panel. Move from render-gate to `enabled: !!parentData` on each sub-section hook to prevent the request entirely.

**Scope.** Update each detail-page sub-section hook (`useAccountTransactions`, `useContractInterface`, `useContractInvocations`, `useContractEvents`, `usePoolCharts`, `usePoolParticipants`, `usePoolTransactions`) to accept `enabled` arg and gate via parent query status. Update consumer pages to pass `enabled: !parentQuery.isError`.

**Findings closed (sub-checklist):**

- [ ] F-W6-NOTFOUND-2 — Sub-section queries fire on parent 404, console noise
- [ ] F-W6-E6-1 — Sub-section queries still fire on 404
- [ ] F-W6-E9-1 — Same on contract detail
- [ ] F-W6-E13- (Network requests) — Same on pool detail

**Notes:** **\_**

---

### 5.4 Cross-entity link gaps (Wave 6 remainder)

- **Type:** BUG
- **Effort:** ~30min
- **Severity / Class:** 🟡 B
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** Wave 6 identified a handful of remaining unlinked identifiers not closed by the F-K-2/3 Gate B batch: NFT row contract ID, NFT detail contract ID in Details section, home table ledger hash, possibly E3 tx-detail ledger link. Plus account self-link (cosmetic) and Soroban call tree destination account routing verification.

**Scope.** Wrap remaining identifier renderings in `<RouterLink>` per the canonical `IdentifierDisplay` pattern. Verify `IdentifierDisplay type="ledger"` on E3 emits an `<a href="/ledgers/:seq">`. Confirm `OperationFlowTree` exposes destination account as clickable.

**Findings closed (sub-checklist):**

- [ ] F-W6-E10-3 — NFT row Contract ID is plain text
- [ ] F-W6-E11-3 — Contract ID in NFT detail Details section is plain text
- [ ] F-W6-E1-4 — Ledger hash on home table not a link
- [ ] F-K-7 — E3 tx-detail ledger sequence link verification
- [ ] F-K-8 — Soroban call tree destination account routing verification
- [ ] F-EX-1 — NFT minted_at_ledger plain text (revisit Figma intent)

**Notes:** F-EX-1 needs Figma confirmation — comment says "Plain Satoshi text per Figma."

---

## Category 6 — Forward-linked / cross-system

### 6.1 Lore drift fix-up — 0066 triple drift + status sweep

- **Type:** DOCS
- **Effort:** ~1h
- **Severity / Class:** 🟠 D
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** Task 0066 (TanStack scaffold) has frontmatter `status: active` but body says `Status: Backlog` / "Not started" — yet history shows it was implemented 2026-05-11. Triple-drift (frontmatter ↔ body ↔ reality) plus empty `related_adr` + `related_tasks`. Suggests other `status: active` FE tasks may carry similar staleness.

**Scope.** Fix 0066 frontmatter + body + cross-refs (related_adr ['0008'], related_tasks ['0063']). Write a Phase-3 walker script in `scripts/` that diffs `status: active` frontmatter against body `## Status:` heading across all FE tasks and reports drift. Spot-fix any other drift surfaced.

**Findings closed (sub-checklist):**

- [ ] A2 — 0066 task body drift
- [ ] Q-4 — 0066 triple-drift confirmed + expanded

**Notes:** **\_**

---

### 6.2 Spawn 23 un-spawned Future Work items as backlog tasks

- **Type:** DOCS / CHORE
- **Effort:** ~3h
- **Severity / Class:** 🟠 D
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** Wave 1 archaeology found 25 of 28 Future Work items from archived FE tasks have NO spawned backlog task. Per 0251 model: cluster small ones into batch tasks (e.g. `XXXX_FEATURE_frontend-libs-ui-hoist-batch`, `XXXX_DOCS_frontend-tx-detail-followups-batch`); spawn dedicated tasks only for items that don't naturally cluster.

**Scope.** Walk the 23 GAP rows in `00-archaeology.md` Future Work table. Spawn backlog tasks under `lore/1-tasks/backlog/XXXX_*.md` with `related_tasks: ['0257']`, severity-tagged. Cluster format/style nits into 1-2 batch tasks. Notable gaps: contracts list page (covered by card 1.3 — skip), responsive nav (covered by card 8.3 — skip), validators → libs/domain migration, IdentifierDisplay router Link audit, table sorting once API exposes sort, tx Amount on PoolTransactions (gated on 0247), per-leg icon_url, SAC SEP-41 stub, B4 fake-XLM disambig design redo, ScVal decoder for Contract Events, Searchable Autocomplete for ops dropdown, etc.

**Findings closed (sub-checklist):**

- [ ] A3 — 25/28 Future Work items un-spawned (23 still remain after card 1.3)
- [ ] AC-13 — Each unchecked AC has spawned task (cross-cite A3)
- [ ] Q-7 — Forward-link expectation mismatch 0254 ↔ 0257 (testing baseline cross-link)

**Notes:** Cards 1.3 + 7.2 + 8.3 close some of these; remainder bulk-spawns here.

---

### 6.3 Backend / infra coordination — 4 follow-ups

- **Type:** RESEARCH / CHORE
- **Effort:** ~2h (mostly comms)
- **Severity / Class:** 🟠 E
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** Several findings need backend or infra cooperation, not FE-only edits. Bundle into a coordination ticket so the team can decide who owns what.

**Scope.** Open 4 cross-team tickets / comms:

1. **CORS infra question** — ping backend/infra owner: "Production CORS — does API GW / ALB terminate, or do we need `tower_http::cors::CorsLayer` added to API?" (C-17).
2. **`wasm_interface_metadata` JSONB schema doc + OpenAPI surface** — backend should add discriminated union schema for `XdrOperationDto.details` per op_type (resolves F-AQ-7 root cause; covers 0075 #6 Emerged hand-typed interface_metadata risk).
3. **`results_meta_xdr` OpenAPI codegen drift** — F-AQ-8: field exists in API but not in generated TS shape. Backend should expose the field properly in utoipa schema; FE regen will catch.
4. **Operation type enum codegen from OpenAPI** — backend should expose op_type as OpenAPI enum so `@hey-api/openapi-ts` generates it; resolves 0069 Future Work + F-Z-2 hand-typed 27-entry FE enum.

**Findings closed (sub-checklist):**

- [ ] C-17 — No `CorsLayer` in `crates/api/src/`
- [ ] F-AQ-7 — `unknown` + runtime probes for heavy XDR shapes
- [ ] F-AQ-8 — Triple cast `results_meta_xdr` codegen drift
- [ ] F-Z-2 — Operation type enum hand-typed in FE
- [ ] Z-1 Spot 5 — Same as F-Z-2
- [ ] 0075 #6 Emerged — `interface_metadata` hand-typed from indexer source
- [ ] 0073 #5 Emerged — Balances cannot distinguish SAC from API (backend gap)

**Notes:** Spawn backend tasks where needed; this card mostly coordinates.

---

### 6.4 ADR + doc-sync sweep (0254 pagination, cursor namespacing, evergreen sync)

- **Type:** DOCS
- **Effort:** ~2h
- **Severity / Class:** 🟡 D
- **Pre-launch:** NICE
- **STATUS:** TODO
- **design_parity note:** `06ab34cc` added a Mainnet/Testnet UI toggle (`NetworkToggle`) that is **NON-FUNCTIONAL** (see card 11.1 / F-DP-1 and §Regressions in impact doc). F-AN-6's "document single-environment config" line now has more to cover: there is a visible toggle implying multi-network, but config is still a single static `VITE_API_BASE_URL`. The doc must now also explain the **visual-only / decorative** toggle (or the toggle is wired / hidden first — see card 11.1 decision). **Added to scope** below. (`cursor` → `next_cursor` + `prev_cursor`) not propagated to `docs/architecture/backend/backend-overview.md`; 0238 multi-cursor namespacing (`cursor_p/_t/_e/_i`) has no ADR; per-feature wiki gaps (frontend conventions, data flow doc, error message standards as exemplar, useDetailMode vs useTableUrlState pattern, asymmetric folder split rule documented).

**Scope.** Update `docs/architecture/backend/backend-overview.md` pagination section with new shape. Write ADR `lore/2-adrs/XXXX_url-cursor-pagination-convention.md` for multi-cursor namespacing. Create `lore/3-wiki/frontend-conventions.md` + `lore/3-wiki/frontend-data-flow.md`.

**Findings closed (sub-checklist):**

- [ ] F-A-3 — Partial ADR 0032 gap on 0254 PR
- [ ] 0238 #5 Emerged — `cursorParam` multi-cursor namespacing ADR gap
- [ ] F-AB-2 — Interval labels (0065 #5) spec body not amended
- [ ] F-AB-1 — `useDetailMode` divergence not in originating task body
- [ ] Q-3 — 0246 missing `## Issues Encountered` heading
- [ ] F-X-4 — Hooks colocated in two places (document pattern)
- [ ] F-Z-4 — Discoverability `lore/3-wiki/frontend-data-flow.md` would help
- [ ] F-AD-2 — Onboarding doc completeness (add convention rules)
- [ ] F-AN-5 — Soroban-era ledger detection absent (document assumption)
- [ ] F-AN-6 — Mainnet/Testnet config single-environment (document) — **NOW ALSO: document the decorative non-functional NetworkToggle added in `06ab34cc`; cross-ref card 11.1 / F-DP-1 wire-or-hide decision**
- [ ] F-AH-6 — No tests doc note (cross-cite; testing-baseline task owns code)
- [ ] F-AA-4 — `useIntersectionObserver` single-consumer note in wiki
- [ ] Issues Encountered worth re-audit (worktree gotchas → `lore/3-wiki/`)

**Notes:** **\_**

---

## Category 7 — Wave 6 visual / UX

### 7.1 Wave 6 visual polish micro-batch (chips, badges, transitions)

- **Type:** REFACTOR
- **Effort:** ~2h
- **Severity / Class:** 🟡 C
- **Pre-launch:** NICE
- **STATUS:** PARTIAL
- **design_parity note:** `06ab34cc` (design_parity) is the Figma-compare pass and partially closed several sub-findings — asset metadata Domain row (F-W6-E8-1), AssetIcon color-coding (F-W6-E7-2), NFT media empty-state (F-W6-E11-2), contract tab-count pills, new Classic/SAC + protocol_version semantic chips (F-W6-CH-2 tangential). **NOT closed:** F-W6-E2-2 typo ("All operations type" still present), F-W6-CH-1 status-badge icon cue, F-W6-AG-3 non-GPU transitions (new components add more), op-type-on-transactions chip (F-W6-E13-3). **REGRESSED:** F-AK-1 / F-W6-AK-1 hardcoded hex 3→5 (AssetIcon `#724311`/`#fffcc2`); F-AK-2 / F-W6-AK-2 raw z-index (new `zIndex: 2` in shell). See cards 11.2 / 11.3. Do NOT mark 7.1 DONE.

**Rationale.** Wave 6 surfaced multiple small visual nits across routes — chip styling, status badge icon-cue for color-blindness, animation property choices, copy nits, transitions at edge of 100ms hover rule. Bundle into a single visual-polish PR.

**Scope.** Add checkmark / X icon to status badges for color-blind compliance. Add semantic color groups to operation type chips. Replace `background-color` / `width` / `border-radius` transitions with `transform` / `opacity` where possible (per F-W6-AG-3 list of 14 sites). Trim hover transitions to ~80-100ms. Wrap LP detail section operation type as entity-style chip. Fix small copy nits (typos, etc.).

**Findings closed (sub-checklist):**

- [ ] F-W6-CH-1 — Status badges rely on color but include text (mid-grade compliance) — **NOT closed by `06ab34cc` (no checkmark/X icon added)**
- [~] F-W6-CH-2 — Operation type chips rely on text only (informational) — **PARTIAL/tangential `06ab34cc`: NEW Classic/SAC chips (AssetsTable, AccountBalances) + protocol_version chip (LedgersTable) — semantic but not the op-type-on-transactions grouping asked**
- [ ] F-W6-AG-3 — Transitions favor non-GPU-accelerated properties
- [ ] F-W6-AG-4 — 150ms / 200ms transitions at edge of <100ms hover rule
- [ ] F-W6-E13-3 — Recent transactions operation type as plain text, no chip
- [ ] F-W6-E2-2 — "All operations type" typo (should be "All operation types") — **NOT closed by `06ab34cc` (TransactionFilters.tsx line unchanged; typo still present)**
- [ ] F-W6-E12-1 — Pool ID truncation shown twice per row
- [ ] F-W6-E12-2 — "Any TVL" filter looks like loading state
- [ ] F-W6-E10-2 — NFT row token IDs as inline text only
- [ ] F-W6-E3-1 — Memo "—" could be more semantic
- [ ] F-W6-E3-2 — "Normal / Advanced" tab pair no description
- [ ] F-W6-E5-1 — Prev/Next ledger button no disabled state for boundary
- [~] F-W6-E7-2 — Asset icon "?" fallback could be better — **PARTIAL `06ab34cc`: AssetIcon now color-coded by kind + 2-line header; "?" letter fallback unchanged (cosmetically richer)**
- [ ] F-W6-E7-3 — Asset detail link uses composite ID for Soroban contracts
- [~] F-W6-E8-1 — Asset metadata sparse — **PARTIAL `06ab34cc`: AssetMetadata adds Domain row (home_page hostname) paired with Homepage; still no full SEP-1 TOML (conditions/contact/org/validators)**
- [ ] F-W6-E8-2 — Holder count not linkable to per-asset holders list
- [ ] F-W6-E9-2 — Invocations + Events sections no obvious empty-state messaging
- [~] F-W6-E11-2 — NFT Traits "Metadata unavailable" no actionable guidance — **PARTIAL `06ab34cc`: NFT *media* empty-state improved (icon chip + "No media available" + subtext); Traits empty-state guidance NOT improved**
- [ ] F-W6-E2-1 — Heading "Transactions list" inconsistent with side-nav "Transactions"
- [ ] F-W6-E1-2 — Hero search box + header search box visually identical but separate state
- [ ] F-W6-E14-2 — Search input has TWO clear-button affordances
- [ ] F-W6-E14-1 — Empty-state hint at `?q=` does not enumerate prefix examples
- [ ] F-W6-E0-4 — Header search placeholder enumerates 4 entity types, page-search hint enumerates 5
- [ ] F-D-3 — Detail page H1 heading inconsistency (partial — non-NotFound version)
- [ ] F-L-2 — Search no-results hint enumerates 4 of 6 entity types
- [ ] F-AK-1 — 3 hardcoded hex constants — **REGRESSED by `06ab34cc`: now 5 (AssetIcon `sac` adds `#724311` + `#fffcc2`; `TYPE_REF_COLOR='#155dfc'` retained). See card 11.2 / F-DP-2**
- [ ] F-AK-2 — Z-index uses raw 0/1 ad-hoc; no defined scale — **REGRESSED by `06ab34cc`: shell now sprinkles raw `zIndex: 2` (AppShell/TopNav/SecondaryNav/Footer). See card 11.3 / F-DP-3**
- [ ] F-W6-AK-1 — Same as F-AK-1 — **REGRESSED (see above)**
- [ ] F-W6-AK-2 — Same as F-AK-2 — **REGRESSED (see above)**
- [ ] F-W6-AK-6 — Theme tokens used pervasively; combine with W6-AK-1
- [ ] F-AD-3 — 3 inline magic numbers worth naming

**Notes:** Subdivide if PR grows too big; intent is one coordinated visual-polish landing.

---

### 7.2 Live indicator freshness logic + footer health probe

- **Type:** FEATURE
- **Effort:** ~5h FE + ~2h backend
- **Severity / Class:** 🟠 A
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** All 5 "live" indicator sites + footer "All systems operational" are hardcoded — no logic compares latest ledger close time to now or probes API health. Data on display can be hours stale while badge shows green. Backfill activity is not surfaced to FE. Universal pre-launch credibility hit.

**Scope.** Add `useLiveStatus()` hook in `libs/ui/src/timestamps/`: compares `latest_close_at` with `now()`; threshold <30s = LIVE, >30s = STALE, >5min = OFFLINE. Single source of truth for footer + all 5 LIVE pill sites. Add `/v1/health` backend endpoint check for footer status indicator. Wire `is_live` / `latest_close_at` from `/v1/network/stats` into the hook. Add subtle pulse / row-flash on poll refresh (paired with card 2.3).

**Findings closed (sub-checklist):**

- [ ] DM-1 — Footer "All systems operational" hardcoded
- [ ] DM-2 — No `/health` or `/status` endpoint hit anywhere
- [ ] F-D-4 — Polling indicator absent on detail pages (PollingIndicator has 0 consumers)
- [ ] F-W6-V-1 — DM-1 re-confirmed + ALL live pills lack freshness logic
- [ ] F-W6-V-2 — Backfill-on-historical doesn't disable LIVE
- [ ] F-W6-V-3 — Latest-ledger polling works (informational)
- [ ] F-W6-AP-2 — Polling refresh silent (no visual indicator)
- [ ] F-W6-E1-1 — LIVE badge on Latest tx/Ledgers shown always (cross-cite DM-1)

**Notes:** Requires backend `/v1/health` endpoint and `is_live` / `latest_close_at` field on `/v1/network/stats`.

---

### 7.3 Pool participants share % precision fix

- **Type:** BUG
- **Effort:** ~15min
- **Severity / Class:** 🟠 A
- **Pre-launch:** SHOULD
- **STATUS:** TODO
- **design_parity R2 note (2026-05-29, PR #224, `fce0d666` / merge `35ac27c0`) — ILLUSORY FIX, STILL TODO:** R2 changed `PoolParticipants.tsx:58` to `formatAmount(row.share_percentage, 2)`, BUT `formatAmount(value, minDecimals)` (`web/src/pages/format.ts:12`) treats the 2nd arg as **minimum-decimal PADDING, NOT rounding** — it trims trailing zeros and pads UP to `minDecimals` but never caps precision. A raw `33.3333…` still renders full precision. The bug is **NOT actually fixed** unless the API pre-rounds `share_percentage` to 2dp. Card stays **TODO** (NOT done). **Needs live confirm** at `/liquidity-pools/:id` participants table before any DONE; if precision >2dp persists, switch to `.toFixed(2)` / a true max-decimals formatter. Source: `design-parity-impact-2026-05-29.md` §4 (F-W6-E13-1), §6.

**Rationale.** Pool participants "Share %" column renders at full precision (`33.3333333333333333%`). Two decimals (`33.33%`) is the universal convention. UX-degrading on every fractional share.

**Scope.** Find the share % render in `web/src/pages/pool-detail/PoolParticipants.tsx`. Apply `formatPercent(value, 2)` from card 2.1 batch (or inline `.toFixed(2)` until 2.1 lands).

**Findings closed (sub-checklist):**

- [ ] F-W6-E13-1 — Pool participants Share % rendered at full precision — **ILLUSORY CONFIRMED LIVE 2026-05-29 — pool `LD5MMO2Q…` participant renders `33.3333333333333333%` raw. `formatAmount(_, 2)` minDecimals ≠ rounding. Needs API pre-round OR FE `Number(x).toFixed(2)`.**

**Notes:** Fast standalone fix; do not wait for card 2.1. **R2 ILLUSORY-FIX WARNING (live re-verify 2026-05-29):** ILLUSORY CONFIRMED LIVE — pool `LD5MMO2Q…` participant renders `33.3333333333333333%` raw. `formatAmount(share_percentage, 2)` minDecimals ≠ rounding (minDecimals is PADDING, not capping). Needs API pre-round OR FE `Number(x).toFixed(2)`. Card NOT done — STAYS TODO.

---

### 7.4 Filter slot a11y (labels / placeholders / aria)

- **Type:** BUG
- **Effort:** ~1h
- **Severity / Class:** 🟡 C
- **Pre-launch:** SHOULD
- **STATUS:** DONE (already-fixed — STALE finding)
- **design_parity note:** STALE finding — NOT a design_parity closure. The filter inputs already carried `aria-label` + `placeholder` at `06ab34cc^` (the commit's own parent), verified by reading the pre-merge AssetFilters/NftFilters and the untouched PoolsFilterBar. The a11y half (F-W6-F-2 / F-W6-E7-1 / F-W6-E10-1) was already resolved by an earlier batch (likely Gate B); design_parity only added responsive widths. **Recommend: re-verify the live names against current develop, then archive this card.** Header-search aria-label (F-W6-F-4) is the only possibly-open residual — confirm in re-verify; if also present, full DONE.

**Rationale.** Filter input slots on `/assets` (2), `/nfts` (4), `/liquidity-pools` (3), plus header search are rendered without accessible names / placeholders / labels visible to screen readers. SR users hear "edit text" with no context. Header search has placeholder but no aria-label; hero search has aria-label. Inconsistent.

**Scope.** Add `aria-label` + `placeholder` to each filter input. Verify with `document.querySelectorAll('input').forEach(el => console.log(el.ariaLabel, el.placeholder))`. Update `libs/ui/src/layout/SearchInput.tsx` and per-page `AssetFilters.tsx`, `NftFilters.tsx`, `PoolsFilterBar.tsx`.

**Findings closed (sub-checklist):**

- [ ] F-W6-F-2 — Filter slots on /assets, /nfts, /liquidity-pools lack accessible names
- [ ] F-W6-F-4 — Header search lacks aria-label and id
- [ ] F-W6-E7-1 — Two filter slots above /assets with no label visible
- [ ] F-W6-E10-1 — Four filter slots above /nfts all unlabeled

**Notes:** **\_**

---

### 7.5 NFT detail heading hierarchy

- **Type:** BUG
- **Effort:** ~15min
- **Severity / Class:** 🟡 C
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** NFT detail page has h1 but NO h2/h3 elements. Section labels (Details / Traits / Transfer history) rendered as styled `<div>` or `<Typography>` without `component=` prop. SR users miss section anchors.

**Scope.** In `web/src/pages/nft-detail/`, add `component="h2"` to section heading Typography elements.

**Findings closed (sub-checklist):**

- [ ] F-W6-E11-1 — NFT detail has h1 but no h2/h3
- [ ] F-W6-F-1 — Same finding (recap)

**Notes:** **\_**

---

### 7.6 Header polling de-duplication

- **Type:** REFACTOR
- **Effort:** ~30min
- **Severity / Class:** 🟢 B
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** Header `HeaderStatsStrip` polls `/network/stats` AND home page also polls `/network/stats` via separate query key — two requests every 12s for the same data. TanStack would dedupe automatically if query keys matched.

**Scope.** Normalize both consumers to use the same `useNetworkStats()` hook + same query key, so TanStack dedupes. Verify in DevTools network panel.

**Findings closed (sub-checklist):**

- [ ] F-W6-E0-5 — Header polling duplicates home polling
- [ ] F-W6-AG-9 — Polling on home + header overlap (cross-cite)
- [ ] F-W6-E1-3 — Home stats strip duplicated in header (informational)
- [ ] F-I-5 — TanStack default dedup (informational; confirmed working — this card validates same-key usage)

**Notes:** **\_**

---

### 7.7 Route transition loading indicator

- **Type:** FEATURE
- **Effort:** ~1h
- **Severity / Class:** 🟡 C
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** Navigation between routes shows blank-state momentarily before route chunk + data load. With LP detail at 300KB this is noticeable on cold cache. React Router 7 + Suspense would let a single `<Suspense fallback>` cover this.

**Scope.** Add global top-bar progress indicator (e.g. `nprogress`-style or React Router's `useNavigation()` state). Wire to a small `<RouteTransitionIndicator>` in `AppShell`.

**Findings closed (sub-checklist):**

- [ ] F-W6-AG-5 — No visible route-transition loading indicator

**Notes:** **\_**

---

### 7.8 Reduced-motion + keyboard trap audit

- **Type:** RESEARCH
- **Effort:** ~1h
- **Severity / Class:** 🟢 C
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** Wave 6 a11y pass deferred two checks: `@media (prefers-reduced-motion: reduce)` not verified on the 14 CSS transitions; keyboard trap on modals/dialogs not exercised (no modal-based UX heavily used). Small audit + fix-up pass.

**Scope.** Add `@media (prefers-reduced-motion: reduce)` rule to global CSS that shortens all transitions to ~0ms. Run keyboard trap test on any popovers (TanStack devtools, Autocomplete, etc.).

**Findings closed (sub-checklist):**

- [ ] F-W6-F-7 — Reduced-motion not verified
- [ ] F-W6-F-8 — No keyboard trap test on dialogs/modals

**Notes:** **\_**

---

## Category 8 — Catalog / lore / docs

### 8.1 Test coverage baseline (libs/ui vitest + critical components)

- **Type:** FEATURE
- **Effort:** ~1w
- **Severity / Class:** 🟠 D (pre-launch maintenance risk)
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** Zero `*.test.*` / `*.spec.*` files across `web/src/` + `libs/ui/src/`. Single biggest pre-launch maintenance risk per F-AD-5. Documented as 0257 dropped scope `O`. Spawn the testing-baseline task with the inheritance chain `related_tasks: ['0238', '0254', '0257']` (per Q-7 forward-link note).

**Scope.** Spawn / promote task 0226 (libs/ui vitest infra). Promote / activate. Add unit tests for: `truncateMiddle`, `useCursorPagination`, `formatAmount`, `useDebouncedDraft`. Add Playwright CLI smoke for 11 paginated pages (blocks 0077, 0238). Wire CI gate.

**Findings closed (sub-checklist):**

- [ ] F-AD-5 — Zero test coverage (cross-cite)
- [ ] F-AH-6 — No tests collocated or in `__tests__/`
- [ ] A4 — Task 0226 backlog since 2026-05-15 unblocks 4 deferred items
- [ ] 0226 promote — blocks 0073/0074/0077/0238 Playwright CLI runs + unit tests
- [ ] 0077 Future Work — Playwright CLI regression for both LP pages
- [ ] 0238 Future Work — Unit tests for `useCursorPagination`, Playwright CLI smoke
- [ ] 0257 dropped scope O — testing coverage

**Notes:** Likely splits into 2-3 sub-tasks. Effort estimate covers full baseline.

---

### 8.2 Dependency hygiene cluster (knip + Renovate + lodash allowlist + prettier 3 + eslint 9)

- **Type:** CHORE
- **Effort:** ~1d
- **Severity / Class:** 🟡 D
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** 33 npm audit vulns (most via nx/module-federation/lodash-es transitive infra), no automated bump mechanism (no Renovate / Dependabot), no dead-export detection in CI (no knip/ts-prune), eslint v8 EoL since 2024-10, prettier 2→3 deferred, MUI 7→9 deferred (causes `@mui/utils` triple-version bloat).

**Scope.** Bundle into single dep-hygiene task: add `knip` to CI with baseline; add Renovate config + grouped MUI/Nx batches; allowlist `lodash-es` via cargo-lambda-cdk in `npm audit`; bump prettier 2→3 + format:write follow-up; bump eslint 8→9 (flat config) + typescript-eslint 8→9.

**Findings closed (sub-checklist):**

- [ ] F-P-2 — No dead-export detection in CI (knip / ts-prune)
- [ ] F-P-6 — Cyclical imports not checked (madge / dependency-cruiser)
- [ ] F-P-8 — No production-bundle console-leak check in CI
- [ ] F-CO-2 — `lodash-es` audit false positive (allowlist)
- [ ] F-CO-5 — eslint v8 EoL
- [ ] F-CO-6 — `@mui/utils` triple-versioned (also touched by 10.2)
- [ ] F-CO-7 — No Snyk / Dependabot / Renovate automation
- [ ] F-CO-8 — prettier 2→3 deferred
- [ ] F-CO-3 — `@mui/material` 2 majors behind (folds into 10.2)
- [ ] F-CO-4 — `react-router-dom` 2 minor behind

**Notes:** MUI 7→9 is the biggest lift — see card 10.2.

---

### 8.3 Responsive redesign (mobile + tablet + hamburger nav)

- **Type:** FEATURE
- **Effort:** ~3-5d
- **Severity / Class:** 🟠 C (pre-launch must-fix if mobile is a goal)
- **Pre-launch:** MUST (if mobile launch in scope) / DEFER-M2 otherwise
- **STATUS:** PARTIAL — root-cause + table overflow RESOLVED via design_parity `06ab34cc` + live re-verify 2026-05-28; residual responsive items split to **C11.5 (hamburger)**, **C11.6 (touch targets)**, **C11.7 (search overflow)**. This card's original scope explicitly bundled hamburger + touch targets, so it stays PARTIAL (not DONE): the scrollWidth/table-overflow scope it covered is DONE, the remaining sub-findings moved to new cards.
- **design_parity note:** Biggest impact from `06ab34cc`. The **802px fixed-page-width root cause is removed** (AppShell `<main>` + TopNav/SecondaryNav/Footer switched to responsive `px`; Home full-bleed sections dropped `px: 10`; HomeHero subtitle no longer nowrap at xs). Tables now wrap in `overflowX: 'auto'` (ExplorerTable + standalone tx-detail tables). Nav scrolls horizontally; heroes stack; KPI strip 2×2. **Live re-verify 2026-05-28 (Playwright):** 41/42 cells show no doc-level horizontal scroll; 768 now docW=757 (was 802 everywhere); 1280 pristine (no regression). → **F-W6-RESPONSIVE-1 RESOLVED**, **F-W6-RESPONSIVE-2 RESOLVED as bug** (tables contained; table→card transform = separate optional enhancement). **Residual split out:** hamburger → C11.5 (user decision 2026-05-28 requires it; scroll-nav alt rejected); touch targets ≥44px → C11.6 (still failing live, 105/106 <44px @375); newly-surfaced /search overflow <660px → C11.7.

**Rationale.** Responsive matrix exposed page-level horizontal scrollbar at <800px (mobile severe, tablet noticeable). Root cause: layout shell has hardcoded ~802px min-width. Tables don't transform to card layout at narrow viewports. No hamburger menu at <768px. Touch targets <44px. WCAG 2.5.5 fail. Per F-W6-RESPONSIVE-1.

**Scope.** Audit + fix 800px min-width root cause (likely `web/src/router/AppShell.tsx` or `libs/ui/src/layout/HeaderStatsStrip.tsx`). Add hamburger menu at <768px (resolves 0059 Future Work). Add table → card transformation OR horizontal-scroll-with-shadow for embedded tables. Audit touch targets to 44px minimum.

**Findings closed (sub-checklist):**

- [x] F-W6-RESPONSIVE-1 — All routes break at viewport <800px due to fixed minimum — **RESOLVED `06ab34cc` + live re-verify 2026-05-28: 41/42 cells no doc-scroll, 768 docW=757 (was 802), 1280 pristine; 802px root cause gone**
- [x] F-W6-RESPONSIVE-2 — No table → card layout responsive transformation — **RESOLVED as bug `06ab34cc` + live re-verify 2026-05-28: tables contained in `overflowX:auto`, doc never overflows; table→card transform = separate optional enhancement (not a failure)**
- [→] F-W6-RESPONSIVE-3 — No hamburger / mobile nav — **SPLIT → C11.5. User decision 2026-05-28: REQUIRE hamburger <768px; scroll-nav alternative rejected**
- [→] F-W6-RESPONSIVE-4 — Touch targets <44px on mobile — **SPLIT → C11.6. Still failing live 2026-05-28: 105/106 interactive elements <44px @375 (pagination 36px, nav 24–32px)**
- [→] F-W6-E0-3 — No hamburger menu at mobile (recap) — **SPLIT → C11.5 (user requires hamburger; see RESPONSIVE-3)**
- [→] 0059 Future Work — Responsive nav (collapsible / hamburger on mobile) — **SPLIT → C11.5 (user requires hamburger)**

**Notes:** Live re-verify 2026-05-28 (Playwright) + design_parity `06ab34cc` resolved the scrollWidth/802px root cause and table page-overflow (RESPONSIVE-1/2 RESOLVED). User decision 2026-05-28 requires a hamburger menu (scroll-nav alt rejected). Residual responsive work split to new cards: **C11.5** hamburger, **C11.6** touch targets, **C11.7** search-page overflow. Card stays PARTIAL because its original scope bundled hamburger + touch targets; only the scrollWidth + table-overflow portion is DONE. User decision still needed: pre-launch (MUST) vs post-launch (DEFER-M2) for the residual cards.

---

### 8.4 Error envelope + reporter + SectionErrorBoundary coverage

- **Type:** REFACTOR
- **Effort:** ~3h
- **Severity / Class:** 🟡 A
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** `client.ts` error interceptor flattens typed `ErrorEnvelope` (ADR 0008 `{code, message}`) into a vanilla `Error` — downstream consumers cannot discriminate on `error.code`. `SectionErrorBoundary` only wraps 2/7 detail pages. No global error reporter (Sentry / DataDog). Silent shape mismatches have no console signal.

**Scope.** Add typed `extractErrorCode(error: unknown): string | null` helper next to `client.ts`. Wrap all 7 detail pages in `SectionErrorBoundary`. Spawn `XXXX_FEATURE_frontend-error-reporting` task or wire a minimal Sentry/console reporter behind env var. Add runtime shape probe (per F-AE-6 / F-D-1 root-cause prevention).

**Findings closed (sub-checklist):**

- [ ] F-AF-1 — Error interceptor swallows raw envelope shape (information loss)
- [ ] F-AE-3 — SectionErrorBoundary inconsistent coverage
- [ ] F-AE-4 — Error interceptor flattens typed envelope (cross-cite F-AF-1)
- [ ] F-AE-6 — Silent shape-mismatch has no console signal (root-cause preventive)
- [ ] F-AE-7 — No global error reporter
- [ ] F-AF-2 — `Object.assign(error)` mutates caught Error (code-review note)
- [ ] F-AF-4 — `envelopeMessage ?? ...` may include `[object Object]` (defensive guard)

**Notes:** Cross-cite Z-1 Spot 1.

---

### 8.5 Polling cache hygiene (gcTime tuning + visibility doc + invalidateResource decision)

- **Type:** REFACTOR
- **Effort:** ~1h
- **Severity / Class:** 🟡 D
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** `detailPolicy.gcTime` not set → relies on global 5min, equal to `staleTime` → cached detail page is gc'd at the moment it would become stale (no overlap for back-button cache hit). `refetchIntervalInBackground` not explicitly pinned. No visibilitychange documentation. `invalidateResource` defined but never called — dead code or pre-mutation infra.

**Scope.** Bump `detailPolicy.gcTime` to ≥10min in `web/src/api/polling.ts`. Explicitly set `refetchIntervalInBackground: false` on `homePolicy`. Add JSDoc header comment documenting TanStack's visibility-API integration. Decide on `invalidateResource`: drop (dead code) or keep + mark "pre-mutation infra".

**Findings closed (sub-checklist):**

- [ ] F-I-3 — No visibilitychange / document.hidden pause
- [ ] F-I-4 — `invalidateResource` defined + exported but never called
- [ ] F-I-6 — No explicit `refetchIntervalInBackground` setting
- [ ] F-I-7 — `gcTime` not set on listPolicy / detailPolicy

**Notes:** **\_**

---

### 8.6 Misc small-batch polish (favicon, color-mode key, time element)

- **Type:** CHORE
- **Effort:** ~1h
- **Severity / Class:** 🟢 D
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** Several trivial nits not worth own cards but worth one polish PR. Favicon 404 on every cold load. localStorage color-mode key naming inconsistency. No `<time dateTime>` semantic element. Em-dash vs ellipsis convention undocumented.

**Scope.** Add `web/public/favicon.ico` or `<link rel="icon" href="data:,">` shim. Rename `soroban-explorer.color-mode` → `sbe:theme` (or document). Wrap timestamp renderings in `<time dateTime>`. Document em-dash convention in wiki.

**Findings closed (sub-checklist):**

- [ ] F-AE-1 — `/favicon.ico` 404 on every route
- [ ] H-12 — Color-mode storage key naming inconsistency
- [ ] C-8 — No `<time dateTime>` semantic element
- [ ] J-6 — Same as C-8 (cross-cite)
- [ ] C-11 — Em-dash vs ellipsis convention undocumented
- [ ] J-14 — Currency symbol "XLM" hardcoded in 2 sites (constant)
- [ ] J-11 — Percentages decimal places no shared constant
- [ ] C-3 — Non-operation enums no FE mirror (document convention)
- [ ] C-4 — Polymorphic IDs link builders inconsistent encoding
- [ ] AO-9 — No FE production deploy workflow visible
- [ ] AO-10 — No PR preview-deploy workflow

**Notes:** Subdivide if PR grows.

---

### 8.7 0061 #4 sort-caret middle-ground designer sign-off

- **Type:** RESEARCH
- **Effort:** ~30min (coordination)
- **Severity / Class:** 🟢 A
- **Pre-launch:** NICE
- **STATUS:** TODO
- **design_parity note:** ARTIFACT CHANGED. `06ab34cc` **rewrote the sort caret** — removed MUI `TableSortLabel` + `UnfoldMore`; new `SortableHeader` with a circular badge + rotating `KeyboardArrowDownIcon`. The "middle-ground" caret the audit flagged for sign-off is now a *different* implementation. Designer sign-off (F-AB-4 / 0061 #4) now applies to the **new circular-badge caret**, not the old one. Status stays TODO; only the artifact under review changed.

**Rationale.** Sort caret design was "deliberate middle ground" between two Figma variants — never confirmed with designer. Audit flagged as partial hallucination risk.

**Scope.** Show designer the current implementation alongside both Figma variants. Pick canonical pattern.

**Findings closed (sub-checklist):**

- [ ] 0061 #4 Emerged — Sort caret middle-ground designer sign-off — **artifact rewritten by `06ab34cc` (now circular-badge `SortableHeader`); sign-off applies to new caret**
- [ ] F-AB-4 — Sort-caret middle ground needs designer sign-off (recap) — **see above; new artifact**

**Notes:** **\_**

---

### 8.8 Lore process hardening (commitlint + PR template + branch protection check)

- **Type:** CHORE
- **Effort:** ~1h
- **Severity / Class:** 🟡 D
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** Conventional Commits compliance + lore-NNNN scope quality is currently 100% team-discipline (81% measured). No commitlint config → no error feedback for new contributors. No `.github/PULL_REQUEST_TEMPLATE.md`. Branch protection on develop needs human verification.

**Scope.** Add `commitlint.config.js` + husky `commit-msg` hook. Create `.github/PULL_REQUEST_TEMPLATE.md` with lore task reference field. Human-verify GitHub branch protection rules on develop.

**Findings closed (sub-checklist):**

- [ ] AR-2 — Mixed `feat(lore-NNNN)` vs `feat(NNNN)` scope styles
- [ ] AR-3 — Commitlint config missing
- [ ] AR-4 — PR template missing
- [ ] AR-7 — Branch protection on develop not verifiable from repo
- [ ] AR-8 — No CHANGELOG.md (pre-launch defer per default; document decision)

**Notes:** **\_**

---

## Category 9 — Out-of-scope follow-ups (per audit README Out of scope table)

### 9.1 Spawn 13 out-of-scope follow-up tasks (bulk)

- **Type:** DOCS
- **Effort:** ~1h
- **Severity / Class:** 🟢 D
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** Audit README "Out of scope" table lists 13 areas to spawn as separate backlog tasks. Per Phase 3 spec.

**Scope.** Walk the Out-of-scope table from `README.md`. Spawn each as `lore/1-tasks/backlog/XXXX_*.md` with `related_tasks: ['0257']` and clear scope.

**Findings closed (sub-checklist):**

- [ ] Out of scope O — testing coverage → `XXXX_FEATURE_frontend-testing-baseline` (covered by card 8.1; skip if already spawned)
- [ ] Out of scope N — i18n readiness → `XXXX_FEATURE_frontend-i18n` (conditional)
- [ ] Out of scope AJ — asset optimization (spawn if perf issues found; covered by card 4.1)
- [ ] Out of scope AT — animation polish (spawn if specific complaint)
- [ ] Out of scope S — browser compat matrix → `XXXX_FEATURE_browser-compat-ci`
- [ ] Out of scope T — production parity (post-prod-up audit)
- [ ] Out of scope BR — Open Graph / Twitter cards → `XXXX_FEATURE_frontend-og-meta`
- [ ] Out of scope BM — long-running tab leaks → `XXXX_RESEARCH_frontend-memory-leaks`
- [ ] Out of scope BJ — WebSocket / SSE → `XXXX_RESEARCH_frontend-realtime`
- [ ] Out of scope BV — offline / service worker → `XXXX_FEATURE_frontend-pwa`
- [ ] Out of scope BZ — GDPR / cookie banner → `XXXX_COMPLIANCE_frontend-gdpr`
- [ ] Out of scope CE — command palette → `XXXX_FEATURE_frontend-command-palette`
- [ ] Out of scope CF — export CSV/JSON → `XXXX_FEATURE_frontend-data-export`

**Notes:** BO session replay skipped per user decision.

---

## Category 10 — Gated external (dependent on other work / decisions)

### 10.1 LP oracle ADR (unblocks 0199 / 0215)

- **Type:** RESEARCH
- **Effort:** ~3h
- **Severity / Class:** 🟠 A
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** Tasks 0199 (LP analytics — TVL/volume/fee_revenue) and 0215 (LP-blocked endpoint FE impact catalog) are blocked-on-oracle with no ADR. Every LP detail chart renders "Chart data not yet available — pending oracle (task 0199)" placeholder. Circular: 0215 is the doc that catalogs what FE shows; itself blocked on 0199. Write the ADR or kill-decision so the team knows whether LP analytics ships pre- or post-launch.

**Scope.** Spawn `XXXX_RESEARCH_lp-oracle-decision-adr` in `lore/2-adrs/`. Decide: oracle source (Stellar Reflector, Chainlink, custom indexer-side computation, etc.). Once ADR lands, 0199 + 0215 unblock.

**Findings closed (sub-checklist):**

- [ ] A5 — 0199 / 0215 LP-blocked tasks never unblocked
- [ ] F-A-4 (Gap) — 0199 blocked-on-oracle (recap)
- [ ] 0077 Future Work — Chart series wiring (gated on 0199)
- [ ] 0077 Future Work — Per-leg `icon_url` in `PoolAssetLeg` (backend extension)

**Notes:** Required input from team for oracle decision.

---

### 10.2 MUI 7 → 9 coordinated bump

- **Type:** REFACTOR
- **Effort:** ~2d (real upgrade lift: sx changes, Grid v2, etc.)
- **Severity / Class:** 🟡 D
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** `@mui/material` is 2 major versions behind (7.3.9 vs 9.0.1). Staying on 7 while sibling `@mui/x-charts` / `@mui/icons-material` are on 9 creates `@mui/utils` triple-versioning in the bundle (real bloat). Coordinated bump eliminates the triplication AND pulls latest MUI security patches.

**Scope.** Spawn `XXXX_REFACTOR_frontend-mui-7-to-9-bump`. Bump `@mui/material` 7→9. Migrate sx changes per upstream guide. Migrate Grid usage to Grid v2. Run visual regression (Wave 6 + Playwright CLI smoke once card 8.1 lands). Verify `@mui/utils` single version in lock.

**Findings closed (sub-checklist):**

- [ ] F-CO-3 — `@mui/material` 2 major versions behind
- [ ] F-CO-6 — `@mui/utils` triple-versioned (RESOLVED by this bump)

**Notes:** Big lift; coordinate with card 4.1 bundle work.

---

### 10.3 0251 B1 root-cause fix (pool-id href + re-enable link)

- **Type:** BUG
- **Effort:** ~30min
- **Severity / Class:** 🟢 D
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** 0251 B1 set `linked={false}` on pool-id header to hide the bug rather than fix the href routing. Future junior may reintroduce the broken link. Now that 0264 has shipped strkey canonical, root-cause fix should be trivial — re-enable the link with correct strkey routing.

**Scope.** Find `linked={false}` site (likely `web/src/pages/pool-detail/PoolDetailHeader.tsx`). Verify `routes.pool(strkey)` produces correct URL. Flip back to `linked={true}` (or remove the prop).

**Findings closed (sub-checklist):**

- [ ] F-AB-3 — Mild fix-by-hide in 0251 B1
- [ ] 0251 B1 Emerged — fix-by-hide on pool-id header

**Notes:** **\_**

---

## Category 11 — design_parity regressions (introduced by `06ab34cc`)

> New cluster. Source: `design-parity-impact-2026-05-27.md` §Regressions. The `feat/design_parity` merge (`06ab34cc` / merge `62c988d4`) introduced 4 net-new debt items while doing its Figma-parity + responsive pass. Each is tracked as a new `F-DP-*` finding (see appendix) and clustered here. Two of these directly **regress** existing audit findings the visual-polish card 7.1 was meant to *close* (F-AK-1 hex, F-AK-2 z-index).

### 11.1 NetworkToggle non-functional affordance

- **Type:** BUG
- **Effort:** ~4h (wire) / ~30min (hide)
- **Severity / Class:** 🟠 C
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** `06ab34cc` added `libs/ui/src/layout/NetworkToggle.tsx` (124 lines): a Mainnet/Testnet segmented control with `role="group"`, `aria-pressed`, hover, per-network palette — wired AppShell → TopNav → NetworkToggle via a local `useState<Network>`. **It is purely visual.** `web/src/api/config.ts` `apiBaseUrl` is a static module constant from `VITE_API_BASE_URL` and does NOT read `network`; query keys (`queryKeys.ts`) do not include network; there is no network context/provider. Switching the toggle changes only the toggle's own rendering — no API base URL change, no refetch, no data difference. It is also **invisible on `/`** (TopNav is now hidden on the home route: `{!isHome && <TopNav .../>}`). This is a misleading affordance — worse for users than F-AN-6's prior no-toggle baseline.

**Scope.** DECISION NEEDED — present both options:
- **Option A (wire it).** Thread `network` into `apiBaseUrl` resolution + namespace query keys by network + add a network context/provider so switching actually changes data. Larger lift; only valid if backend serves both networks. Also surface on `/` (or accept home-route absence by design).
- **Option B (hide it).** Remove / feature-flag the toggle until multi-network is real. Restores the honest single-environment baseline; pairs with card 6.4 documenting single-env config.

**Findings closed (sub-checklist):**

- [ ] F-DP-1 — NetworkToggle non-functional (wire OR hide)
- [ ] F-AN-6 (cross-cite) — single-environment config doc must reflect the chosen outcome

**Notes:** Introduced by design_parity `06ab34cc`. Cross-ref card 6.4 (documentation) + "Pending live verification" (no-op confirm). Decision owner: user/designer + backend (is multi-network served?). **design_parity R2 (2026-05-29, PR #224, merge `35ac27c0`) did NOT address this** — `web/src/api/config.ts` unchanged (still static `apiBaseUrl` from `VITE_API_BASE_URL`, no `network` read); `queryKeys.ts` network set is endpoint-grouping not per-network namespacing; AppShell still local `useState<Network>` flowing only into TopNav; TopNav still hidden on `/`. Toggle remains neither wired nor removed. STILL FAKE. **VERIFIED-FAKE live 2026-05-29** — on `/transactions`, clicking Testnet flips `aria-pressed` only; no URL change, no banner, no list refetch; the only request fired is the periodic LiveIndicator poll (`GET localhost:9000/v1/network/stats`) hitting the SAME Mainnet host (no testnet base URL, no network query param). Pure decorative. Card STAYS TODO. Source: `design-parity-impact-2026-05-29.md` §1 (11.1), §2 (F-DP-1), §Live re-verify 2026-05-29 (item 3).

---

### 11.2 AssetIcon hardcoded hex → theme tokens (regresses F-AK-1)

- **Type:** REFACTOR
- **Effort:** ~30min
- **Severity / Class:** 🟠 C
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** `06ab34cc` added inline hardcoded hex `'#724311'` + `'#fffcc2'` in AssetIcon (`sac` kind), bringing the hardcoded-hex count from 3 → 5 (`ContractInterface` `TYPE_REF_COLOR='#155dfc'` retained). Directly **regresses F-AK-1 / F-W6-AK-1**, which card 7.1 was meant to close.

**Scope.** Move the 2 new AssetIcon hex values to theme tokens (e.g. a `palette.assetKind.sac` token pair) alongside the card 7.1 hex consolidation. Fold into card 7.1's hex sweep OR land standalone here.

**Findings closed (sub-checklist):**

- [ ] F-DP-2 — AssetIcon `#724311` / `#fffcc2` hardcoded (move to theme tokens)
- [ ] F-AK-1 / F-W6-AK-1 (cross-cite) — regression must be undone as part of the hex consolidation

**Notes:** Introduced by design_parity `06ab34cc`. Coordinate with card 7.1. **design_parity R2 (2026-05-29, PR #224, merge `35ac27c0`) did NOT fix this** — `AssetIcon.tsx:28` still inlines `#724311`/`#fffcc2`. R2 confirmed `#724311`/`#fffcc2` ARE legit token VALUES (`colors.ts:91`) but they are bound **raw, not via `theme.palette`** — still the regression. The R2 `assetColor.ts` touch was a **red herring** (that file already uses `colorsLight.*` tokens; it is NOT the hardcoded-hex regression site, which is AssetIcon). Regression persists at AssetIcon. Source: `design-parity-impact-2026-05-29.md` §1 (11.2), §2 (F-DP-2).

---

### 11.3 Raw z-index additions → z-index scale (regresses F-AK-2)

- **Type:** REFACTOR
- **Effort:** ~30min
- **Severity / Class:** 🟠 C
- **Pre-launch:** NICE
- **STATUS:** TODO

**Rationale.** `06ab34cc` added raw `zIndex: 2` in several shell spots (AppShell / TopNav / SecondaryNav / Footer, layering the shell above `PageGridBackdrop`). Adds to the ad-hoc-z-index debt with no defined scale. **Regresses F-AK-2 / F-W6-AK-2.**

**Scope.** Define a z-index scale (e.g. `theme.zIndex.appBackdrop` / `appContent` / `appShell`) and migrate the raw `0`/`1`/`2` values across HomePage, AppShell, PageGridBackdrop, TopNav, SecondaryNav, Footer. Fold into card 7.1's z-index item OR land standalone here.

**Findings closed (sub-checklist):**

- [ ] F-DP-3 — Raw `zIndex: 2` additions in shell (move to z-index scale constants)
- [ ] F-AK-2 / F-W6-AK-2 (cross-cite) — regression folded into the z-index scale work

**Notes:** Introduced by design_parity `06ab34cc`. Coordinate with card 7.1.

---

### 11.4 OperationFlowTree collapse/expand lost — verify vs Figma

- **Type:** BUG
- **Effort:** ~2h (verify + restore if regression)
- **Severity / Class:** 🟠 C
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** `06ab34cc` rewrote `OperationFlowTree` — removed the `useState` + `Collapse` + expand chevron; operation trees now render **flat** with dashed sibling connectors. If collapse/expand was intended UX (deep Soroban call trees), this is a **functional regression**; if Figma specifies a flat tree, it is intended and should be documented.

**Scope.** Verify the current `OperationFlowTree` against Figma + with the designer. If collapse was intended UX → restore `useState`/`Collapse`/chevron (or a better affordance for deep trees). If flat is intended → document the decision and close. Also: cross-check the contract `events` tab count wired to `recent_unique_callers` (callers ≠ events — possible mislabel; verify intended stat, see impact doc lower-severity notes).

**Findings closed (sub-checklist):**

- [ ] F-DP-4 — OperationFlowTree collapse/expand removed (verify vs Figma; restore if regression)

**Notes:** Introduced by design_parity `06ab34cc`. Designer / Figma confirmation required to classify as regression vs intended. **design_parity R2 (2026-05-29, PR #224, merge `35ac27c0`):** R2 **rewrote `OperationFlowTree` again** (202 lines in merge) but collapse/expand is **still absent** — renders flat with static indented `borderLeft` dashed connectors; no `useState` / `Collapse` / chevron; `defaultExpanded?` on the `FlowNode` interface (line 44) is now a dead/unused prop. Still needs verify-vs-Figma (intended flat vs regression). **live re-verify 2026-05-29:** flat render confirmed live (tx `7b9bacc8…` Advanced mode `?mode=advanced`: 0 expand/collapse buttons, no chevron affordance) BUT nested-tree verify is BLOCKED — local dev dataset has 0 soroban / 0 multi-op txs (all 38 single-op, `has_soroban:false`, `operation_count:1`). Full verify needs `invoke_host_function` / multi-op data. Figma sign-off still pending. Card STAYS TODO. Source: `design-parity-impact-2026-05-29.md` §1 (11.4), §2 (F-DP-4), §Live re-verify 2026-05-29 (item 6).

---

### 11.5 Hamburger nav menu for <768px

- **Type:** FEATURE
- **Effort:** ~3-4h
- **Severity / Class:** 🟠 C
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** design_parity removed the 802px doc-scroll root cause but left no hamburger menu at narrow viewports. At 375px the 8 nav links happen to fit in 364px without scrolling, but that's fragile — any nav label change or i18n overflows. User decision 2026-05-28: require a proper hamburger menu, not the scroll-nav fallback.

**Scope.** Add hamburger menu component to TopNav (libs/ui/src/layout/TopNav.tsx) that collapses nav links into a drawer/menu below ~768px breakpoint. Desktop unchanged.

**Findings closed (sub-checklist):**

- [ ] F-W6-RESPONSIVE-3 — no hamburger nav at <768px (user requires hamburger)

**Notes:** Live re-verify 2026-05-28 confirmed scroll-nav alt present but user rejected it. Effort ~3-4h (drawer + breakpoint + a11y focus trap). **design_parity R2 (2026-05-29, PR #224, merge `35ac27c0`) did NOT add a hamburger** — R2 "responsive nav tweaks" (`fce0d666`) = TopNav stats `overflowX:auto` + SecondaryNav scroll-nav (already from round 1); grep for hamburger/MenuIcon/Drawer/`aria-label="Open menu"` = zero hits. Card remains TODO. Source: `design-parity-impact-2026-05-29.md` §1 (11.5), §2 (RESPONSIVE-3).

---

### 11.6 Touch targets ≥44px (mobile a11y)

- **Type:** BUG
- **Effort:** ~2-3h
- **Severity / Class:** 🟠 C
- **Pre-launch:** SHOULD
- **STATUS:** TODO

**Rationale.** 105 of 106 interactive elements are <44px in at least one dimension at 375px viewport (pagination Prev/Next = 36px tall, nav links 24-32px). WCAG 2.1 AA target-size minimum is 44×44px. Mobile users mis-tap. Untouched by design_parity.

**Scope.** Audit + enlarge interactive element hit areas to ≥44×44px at mobile breakpoint: pagination controls (libs/ui PaginationControls), nav links (TopNav/SecondaryNav), table row actions, filter chips. Use min-height/min-width or padding; visual size can stay smaller if hit-area padded.

**Findings closed (sub-checklist):**

- [ ] F-W6-RESPONSIVE-4 — 105/106 interactive elements <44px at 375px

**Notes:** Live-confirmed 2026-05-28. WCAG 2.1 AA 2.5.5 target size. **design_parity R2 (2026-05-29, PR #224, merge `35ac27c0`) did NOT enlarge touch targets** — no min-height/min-width sizing pass; 105/106 <44px @375 from 2026-05-28 live still stands. Card remains TODO. Source: `design-parity-impact-2026-05-29.md` §1 (11.6), §2 (RESPONSIVE-4).

---

### 11.7 Search page overflow <660px — RE-CLASSIFIED: page overflow RESOLVED (live 2026-05-29)

- **Type:** BUG
- **Effort:** ~1h
- **Severity / Class:** 🟡 C
- **Pre-launch:** NICE
- **STATUS:** DONE-mitigated (page overflow RESOLVED live 2026-05-29; residual per-card reflow = optional NICE enhancement, same treatment as RESPONSIVE-2 table→card)

**Rationale.** ~~The /search page causes document horizontal scroll below ~660px viewport.~~ **RE-CLASSIFIED (live re-verify 2026-05-29):** the page-level overflow is GONE — `/search?q=test` @375 reports `documentElement.scrollWidth = 364 ≤ innerWidth 375` (NO page-level horizontal scroll; R1/R2 "~644px still overflowing" prediction REFUTED live). The 651px category-card row (Transactions/Accounts/Contract/Token/NFT/Liquidity Pool) now sits in an `overflow-x:auto` container (clientWidth 332) and scrolls WITHIN the container — same scroll-within mitigation as tables (RESPONSIVE-2), does not push the page. The gating page-overflow check PASSES. Residual: the category-card row lacks per-card reflow (scrolls within container, NOT page overflow) — a softer enhancement, same class as the RESPONSIVE-2 table→card transform (separate optional NICE, not a bug).

**Scope.** Page-overflow bug is RESOLVED (no action needed). OPTIONAL enhancement (NICE, not pre-launch): make the search result-category row reflow/wrap per-card at narrow widths instead of scrolling within its container (`flex-wrap` / `min-width:0` / stack on narrow). Track as a NICE enhancement only if worth following up — otherwise this card is effectively closed-as-mitigated.

**Findings closed (sub-checklist):**

- [x] F-W6-RESPONSIVE-5 — search category card overflows <660px — **RESOLVED (page overflow gone, live-confirmed 2026-05-29: scrollWidth 364 ≤ 375); 651px category-card row scrolls within `overflow-x:auto` container (same mitigation as RESPONSIVE-2). Residual per-card reflow = optional NICE enhancement.**

**Notes:** Newly-surfaced live 2026-05-28; original screenshot at .playwright-mcp/e14-search-375-overflow.png (pre-swap Mona Sans state, retained for before/after). **RE-CLASSIFIED live re-verify 2026-05-29:** page-level overflow is GONE — `/search?q=test` @375 `documentElement.scrollWidth = 364 ≤ 375`. The R1/R2 code-only prediction (~644px page overflow, search untouched by R2 theme-token refactor) is REFUTED by the live run: the 651px category-card row scrolls inside an `overflow-x:auto` container rather than pushing page width (same scroll-within mitigation as tables, RESPONSIVE-2). F-W6-RESPONSIVE-5 → RESOLVED (page overflow mitigated). True per-card reflow/wrap still absent → optional NICE enhancement, mirroring how RESPONSIVE-2 table→card transform was treated (RESOLVED-as-bug; transform = separate optional). Screenshot: `screenshots/search-375-no-page-overflow.png`. Source: `design-parity-impact-2026-05-29.md` §Live re-verify 2026-05-29 (item 7).

---

## Appendix — 281-finding STATUS table

Compact per-finding cross-reference. One line per finding ID surfaced by audit Waves 1-6. STATUS:

- `RESOLVED` — already shipped pre-queue; SHA in Notes
- `SKIP` — user-dropped, rationale in Notes
- `PARTIAL` — partially addressed (e.g. design_parity `06ab34cc` code-verified one half); remaining scope open + live re-verify pending
- `STALE` — finding's premise no longer true (e.g. F-AH-1 PageStub revived); see Notes
- `DONE` (appendix) — closed (incl. already-fixed-pre-merge stale-a11y rows); confirm on develop before archive
- `→ C N.M` — clustered into card N.M of this queue; STATUS tracks per card
- `TODO` (orphan) — surfaced by audit but not assigned to any card or skip; review during impl

| Finding                                        | Wave        | Sev     | Cluster                 | STATUS      | Notes                                                                       |
| ---------------------------------------------- | ----------- | ------- | ----------------------- | ----------- | --------------------------------------------------------------------------- |
| A1                                             | 1           | 🔴      | —                       | RESOLVED    | TxDetail stub — a2c1b205 (FilipDz PR #215)                                  |
| A2                                             | 1           | 🟠      | C 6.1                   | TODO        | 0066 task body drift                                                        |
| A3                                             | 1           | 🟠      | C 6.2                   | TODO        | 25/28 Future Work un-spawned                                                |
| A4                                             | 1           | 🟡      | C 8.1                   | TODO        | 0226 test infra blocked                                                     |
| A5                                             | 1           | 🟡      | C 10.1                  | TODO        | 0199/0215 LP blocked                                                        |
| F-AF-1                                         | 1           | 🟡      | C 8.4                   | TODO        | Error interceptor flattens envelope                                         |
| F-AF-2                                         | 1           | 🟢      | C 8.4                   | TODO        | Object.assign(error) smell                                                  |
| F-AF-3                                         | 1           | 🟢      | —                       | SKIP        | as unknown as `useNow.ts` justified                                         |
| F-AF-4                                         | 1           | 🟢      | C 8.4                   | TODO        | envelopeMessage object-string guard                                         |
| F-AQ-1                                         | 1           | 🟠      | C 3.1                   | TODO        | noUncheckedIndexedAccess flag                                               |
| F-AQ-2                                         | 1           | 🟡      | C 3.1                   | TODO        | exactOptionalPropertyTypes flag                                             |
| F-AQ-3                                         | 1           | 🟡      | C 3.3                   | TODO        | Switch exhaustiveness + assertNever                                         |
| F-AQ-4                                         | 1           | 🟠      | C 3.2                   | TODO        | Branded ID types                                                            |
| F-AQ-5                                         | 1           | 🟢      | —                       | SKIP        | Discriminated unions zero — no issue                                        |
| F-AQ-6                                         | 1           | 🟢      | —                       | SKIP        | Generic constraints sensible — no issue                                     |
| F-AQ-7                                         | 1           | 🟡      | C 6.3                   | TODO        | XDR unknown casts (backend coordination)                                    |
| F-AQ-8                                         | 1           | 🟡      | C 6.3                   | TODO        | results_meta_xdr codegen drift                                              |
| F-P-1                                          | 1           | 🟡      | C 3.1                   | TODO        | Lint warning assetColor.ts:131 — R2 (PR #224): still present, live `nx lint` 2026-05-29 confirms 1 warning unchanged (`assetColor.ts:131:10 Forbidden non-null assertion`) |
| F-P-2                                          | 1           | 🟡      | C 8.2                   | TODO        | No knip/ts-prune in CI                                                      |
| F-P-3                                          | 1           | ✓       | —                       | RESOLVED    | Zero console.\* in source (baseline)                                        |
| F-P-4                                          | 1           | ✓       | —                       | RESOLVED    | Zero TODO/FIXME markers (baseline)                                          |
| F-P-5                                          | 1           | ✓       | —                       | RESOLVED    | Zero commented-out blocks (baseline)                                        |
| F-P-6                                          | 1           | 🟢      | C 8.2                   | TODO        | Cyclical imports not checked                                                |
| F-P-7                                          | 1           | 🟢      | C 7.1 (overrides split) | DEFER-M2    | overrides.ts 867 LOC — splittable; F-Y-1                                    |
| F-P-8                                          | 1           | 🟢      | C 8.2                   | TODO        | No bundle console-leak grep in CI                                           |
| F-AI-1                                         | 1           | 🟠      | C 4.1                   | TODO        | Main bundle > 500KB                                                         |
| F-AI-2                                         | 1           | 🟠      | C 4.1                   | TODO        | LP detail chunk 313KB                                                       |
| F-AI-3                                         | 1           | 🟡      | C 4.1                   | TODO        | SearchOutlined 67KB chunk anomaly                                           |
| F-AI-4                                         | 1           | 🟢      | —                       | SKIP        | ExplorerTable chunk informational                                           |
| F-AI-5                                         | 1           | ✓       | —                       | RESOLVED    | Devtools tree-shake confirmed                                               |
| F-AI-6                                         | 1           | ✓       | —                       | RESOLVED    | Tree-shake validated                                                        |
| F-AI-7                                         | 1           | 🟡      | C 4.1                   | TODO        | No bundle visualizer                                                        |
| F-AI-8                                         | 1           | 🟡      | C 4.1                   | TODO        | No vendor chunk split                                                       |
| F-AI-9                                         | 1           | ✓       | —                       | RESOLVED    | CSS total tiny (informational)                                              |
| F-AI-10                                        | 1           | 🟡      | C 4.1                   | TODO        | TxDetail chunk 30KB (Filip baseline)                                        |
| F-AI-11                                        | 1           | 🟢      | —                       | SKIP        | TransactionsListPage +0.15KB (informational)                                |
| F-CO-1                                         | 1           | 🟠      | —                       | RESOLVED    | Vite 7.3.3 CVE bump — 473de2a2                                              |
| F-CO-2                                         | 1           | 🟢      | C 8.2                   | TODO        | lodash-es allowlist                                                         |
| F-CO-3                                         | 1           | 🟡      | C 10.2                  | TODO        | MUI 7→9 bump                                                                |
| F-CO-4                                         | 1           | 🟢      | C 8.2                   | TODO        | react-router-dom 2 minor                                                    |
| F-CO-5                                         | 1           | 🟡      | C 8.2                   | TODO        | eslint v8 EoL                                                               |
| F-CO-6                                         | 1           | 🟠      | C 10.2 / C 8.2          | TODO        | mui/utils triple-version                                                    |
| F-CO-7                                         | 1           | 🟢      | C 8.2                   | TODO        | No Renovate/Dependabot                                                      |
| F-CO-8                                         | 1           | 🟢      | C 8.2                   | TODO        | prettier 2→3                                                                |
| C-1                                            | 2           | ✓       | —                       | RESOLVED    | normalizeOperationType H2 root cause baseline                               |
| C-2                                            | 2           | ✓       | —                       | RESOLVED    | 27 ops parity holds                                                         |
| C-3                                            | 2           | 🟢      | C 8.6                   | TODO        | Non-op enums no FE mirror (document)                                        |
| C-4                                            | 2           | 🟢      | C 8.6                   | TODO        | Polymorphic ID link builders inconsistent encoding                          |
| C-5                                            | 2           | 🟡      | C 3.2                   | TODO        | Missing isAssetId / isNftId validator                                       |
| C-6                                            | 2           | ✓       | —                       | RESOLVED    | Pool id strkey/hex round-trip OK                                            |
| C-7                                            | 2           | partial | —                       | RESOLVED    | UTC timestamps consistent (baseline)                                        |
| C-8                                            | 2           | 🟢      | C 8.6                   | TODO        | No `<time dateTime>` element                                                |
| C-9                                            | 2           | ✓       | —                       | RESOLVED    | Trailing-zero trim works                                                    |
| C-10                                           | 2           | ✓       | —                       | RESOLVED    | minDecimals floor works                                                     |
| C-11                                           | 2           | 🟢      | C 8.6                   | TODO        | Em-dash vs ellipsis convention undocumented                                 |
| C-12                                           | 2           | ✓       | —                       | RESOLVED    | Em-dash exclusive (no hyphen)                                               |
| C-13                                           | 2           | ✓       | —                       | RESOLVED    | Cursor pagination semantic uniform                                          |
| C-14                                           | 2           | ✓       | —                       | RESOLVED    | useCursorPagination single hook                                             |
| C-15                                           | 2           | 🟢      | C 8.5                   | TODO        | Polling cache headers per-endpoint (minor smell)                            |
| C-16                                           | 2           | —       | —                       | SKIP        | Polling pause check deferred to 1.22 (covered by F-I-3)                     |
| C-17                                           | 2           | 🟠      | C 6.3                   | TODO        | No CorsLayer (infra coordination)                                           |
| C-18                                           | 2           | ✓       | —                       | RESOLVED    | FE client credentials OK                                                    |
| C-19                                           | 2           | ✓       | —                       | RESOLVED    | Error envelope shape OK                                                     |
| C-20                                           | 2           | ✓       | —                       | RESOLVED    | API base URL config OK                                                      |
| J-1                                            | 2           | ✓       | —                       | RESOLVED    | formatAmount usage healthy                                                  |
| J-2                                            | 2           | 🟡      | C 2.1                   | TODO        | 10 toLocaleString sites bypass formatter                                    |
| J-3                                            | 2           | 🟡      | C 2.1                   | TODO        | TopNav.formatNumber duplicate                                               |
| J-4                                            | 2           | 🟠      | C 2.1                   | TODO        | STROOPS_PER_XLM single site (drift realized)                                |
| J-5                                            | 2           | 🟡      | C 2.1                   | TODO        | Timestamp depth inconsistency                                               |
| J-6                                            | 2           | 🟢      | C 8.6                   | TODO        | No `<time>` element (cross-cite C-8)                                        |
| J-7                                            | 2           | 🟠      | C 2.1                   | TODO        | Truncation re-impls (6 sites)                                               |
| J-8                                            | 2           | ✓       | —                       | RESOLVED    | Hash truncation per-type via IdentifierDisplay                              |
| J-9                                            | 2           | ✓       | —                       | RESOLVED    | Strkey vs hex pool strategy documented                                      |
| J-10                                           | 2           | ✓       | —                       | RESOLVED    | Asset labels with issuer disambig OK                                        |
| J-11                                           | 2           | 🟢      | C 8.6                   | TODO        | Percentages decimals no shared constant                                     |
| J-12                                           | 2           | ✓       | —                       | RESOLVED    | Status badge colors consistent                                              |
| J-13                                           | 2           | ✓       | —                       | RESOLVED    | Event-type chip colors single map                                           |
| J-14                                           | 2           | 🟢      | C 8.6                   | TODO        | Currency symbol XLM hardcoded                                               |
| J-15                                           | 2           | ✓       | —                       | RESOLVED    | Em-dash convention OK                                                       |
| F-J-16                                         | 2           | 🟠      | C 2.1                   | TODO        | Duplicate formatFee BigInt vs Number                                        |
| F-J-17                                         | 2           | 🟡      | C 2.1                   | TODO        | formatStroops 3rd entry point                                               |
| Q-1                                            | 2           | ✓       | —                       | RESOLVED    | Acceptance Criteria present in archived tasks                               |
| Q-2                                            | 2           | ✓       | —                       | RESOLVED    | Design Decisions section present                                            |
| Q-3                                            | 2           | 🟢      | C 6.4                   | TODO        | 0246 missing Issues Encountered heading                                     |
| Q-4                                            | 2           | 🟠      | C 6.1                   | TODO        | 0066 triple-drift                                                           |
| Q-5                                            | 2           | ✓       | —                       | RESOLVED    | API commits include openapi regen                                           |
| Q-6                                            | 2           | 🟢      | —                       | RESOLVED    | ADR 0032 evergreen-docs gate honored (baseline)                             |
| Q-7                                            | 2           | ✓       | —                       | RESOLVED    | ADR cross-ref density healthy                                               |
| Q-7 (post-merge)                               | 2           | 🟡      | C 6.2                   | TODO        | Forward-link expectation mismatch 0254↔0257                                 |
| AR-1                                           | 2           | ✓       | —                       | RESOLVED    | Conventional Commits 81% compliance                                         |
| AR-2                                           | 2           | 🟢      | C 8.8                   | TODO        | Mixed lore-scope styles                                                     |
| AR-3                                           | 2           | 🟡      | C 8.8                   | TODO        | Commitlint missing                                                          |
| AR-4                                           | 2           | 🟡      | C 8.8                   | TODO        | PR template missing                                                         |
| AR-5                                           | 2           | ✓       | —                       | RESOLVED    | Branch naming OK                                                            |
| AR-6                                           | 2           | ✓       | —                       | RESOLVED    | Husky pre-commit OK                                                         |
| AR-7                                           | 2           | 🟡      | C 8.8                   | TODO        | Branch protection check (human)                                             |
| AR-8                                           | 2           | 🟢      | C 8.8                   | TODO        | No CHANGELOG.md                                                             |
| DM-1                                           | 2           | 🟠      | C 7.2                   | TODO        | Footer "All systems operational" hardcoded                                  |
| DM-2                                           | 2           | 🟢      | C 7.2                   | TODO        | No /health probe                                                            |
| DN-1                                           | 2           | 🟠      | C 1.2                   | TODO        | No build SHA in UI                                                          |
| DN-2                                           | 2           | 🟡      | C 1.2                   | TODO        | No vite define block                                                        |
| CA-1                                           | 2           | 🟠      | C 1.1                   | TODO        | Footer Terms/Privacy/Cookies dead spans                                     |
| CA-2                                           | 2           | 🟠      | C 1.1                   | TODO        | Footer Resources dead spans                                                 |
| CA-3                                           | 2           | 🟢      | C 1.1                   | TODO        | target=\_blank+rel preventive                                               |
| CA-4                                           | 2           | ✓       | —                       | RESOLVED    | Copyright line OK                                                           |
| AO-1                                           | 2           | ✓       | —                       | RESOLVED    | .env.example exists                                                         |
| AO-2                                           | 2           | ✓       | —                       | RESOLVED    | web/.env.example covers VITE\_\*                                            |
| AO-3                                           | 2           | ✓       | —                       | RESOLVED    | No hardcoded localhost in src                                               |
| AO-4                                           | 2           | ✓       | —                       | RESOLVED    | No console.\* leftover                                                      |
| AO-5                                           | 2           | ✓       | —                       | RESOLVED    | .gitignore coverage OK                                                      |
| AO-6                                           | 2           | ✓       | —                       | RESOLVED    | No secrets in history                                                       |
| AO-7                                           | 2           | ✓       | —                       | RESOLVED    | CI typescript gate OK                                                       |
| AO-8                                           | 2           | ✓       | —                       | RESOLVED    | CI api-types-codegen gate OK                                                |
| AO-9                                           | 2           | 🟢      | C 8.6                   | TODO        | No FE prod deploy workflow                                                  |
| AO-10                                          | 2           | 🟢      | C 8.6                   | TODO        | No PR preview-deploy workflow                                               |
| AO-11                                          | 2           | —       | C 1.2                   | TODO        | Prod build version stamp (covered by DN-1)                                  |
| K-1 (=F-K-1)                                   | 3           | 🟠      | —                       | RESOLVED    | TxDetail stub — a2c1b205 (Filip)                                            |
| F-K-2                                          | 3           | 🟠      | —                       | RESOLVED    | Pool reserve links — 473de2a2 + a5f15166                                    |
| F-K-3                                          | 3           | 🟠      | —                       | RESOLVED    | Pool participants "Since ledger" link — 473de2a2                            |
| F-K-4                                          | 3           | 🟡      | —                       | RESOLVED    | Pool URL strkey hint — 6421d3d7 (0270)                                      |
| F-K-5                                          | 3           | 🟢      | —                       | SKIP        | Account self-link cosmetic — no fix                                         |
| F-K-6                                          | 3           | 🟢      | —                       | SKIP        | Account TX no source-account column (intentional)                           |
| F-K-7                                          | 3           | 🟡      | C 5.4                   | TODO        | E3 tx-detail ledger link verification                                       |
| F-K-8                                          | 3           | 🟡      | C 5.4                   | TODO        | Soroban call tree destination account routing                               |
| F-K-9                                          | 3           | 🟠      | —                       | RESOLVED    | PoolAssetLeg schema gap — 473de2a2                                          |
| F-E-1                                          | 3           | 🔴      | —                       | RESOLVED    | URL cursor write — f646047d (0254 merge)                                    |
| F-E-2                                          | 3           | 🟠      | —                       | SKIP        | URL wire contract — user-dropped 2026-05-25                                 |
| F-E-3                                          | 3           | 🟡      | C 5.1                   | TODO        | Catch-all 404 `<main>` landmark — **VERIFIED BROKEN live 2026-05-29: `/foobar` catch-all has NO `<main>` (`hasMain: false`) post-AppShell restructure. STAYS TODO** |
| F-E-4                                          | 3           | ✓       | —                       | RESOLVED    | Filter URL preserves refresh OK                                             |
| F-E-5                                          | 3           | ✓       | —                       | RESOLVED    | Trailing slash tolerated                                                    |
| F-E-6                                          | 3           | ✓       | —                       | RESOLVED    | Deep link from raw URL OK                                                   |
| F-E-7                                          | 3           | 🟡      | C 5.2                   | TODO        | No URL state for tabs                                                       |
| F-E-8                                          | 3           | 🟢      | —                       | RESOLVED    | cursor_p/\_e/\_i — same fix as F-E-1                                        |
| F-L-1                                          | 3           | 🟠      | —                       | RESOLVED    | Pool strkey search — 047ce51e + 6421d3d7 (0270)                             |
| F-L-2                                          | 3           | 🟡      | C 7.1                   | TODO        | Hint enumerates 4 of 6 entity types                                         |
| F-L-3                                          | 3           | ✓       | —                       | RESOLVED    | XSS escaped (baseline)                                                      |
| F-L-4                                          | 3           | ✓       | —                       | RESOLVED    | Debounce confirmed                                                          |
| F-L-5                                          | 3           | ✓       | —                       | RESOLVED    | Long query handled gracefully                                               |
| F-L-6                                          | 3           | 🟡      | —                       | SKIP        | treatRedirectAsResult flag (catalog-only; no bug)                           |
| F-H-1                                          | 3           | ✓       | —                       | RESOLVED    | Zero console.\* (baseline)                                                  |
| F-H-2                                          | 3           | ✓       | —                       | RESOLVED    | Zero dangerouslySetInnerHTML / eval                                         |
| F-H-3                                          | 3           | ✓       | —                       | RESOLVED    | XSS probe escaped                                                           |
| F-H-4                                          | 3           | ✓       | —                       | RESOLVED    | safeHttpUrl link injection guard                                            |
| F-H-5                                          | 3           | ✓       | —                       | RESOLVED    | target=\_blank with rel=noopener                                            |
| F-H-6                                          | 3           | ✓       | —                       | RESOLVED    | Zero iframe                                                                 |
| F-H-7                                          | 3           | ✓       | —                       | RESOLVED    | localStorage minimal + non-sensitive                                        |
| F-H-8                                          | 3           | ✓       | —                       | RESOLVED    | Zero sessionStorage                                                         |
| F-H-9                                          | 3           | ✓       | —                       | RESOLVED    | Zero document.cookie                                                        |
| F-H-10                                         | 3           | ✓       | —                       | RESOLVED    | Auth headers only in generated SDK                                          |
| F-H-11                                         | 3           | ✓       | —                       | RESOLVED    | Env vars constrained                                                        |
| H-12                                           | 3           | 🟢      | C 8.6                   | TODO        | Color-mode storage key naming                                               |
| F-I-1                                          | 3           | ✓       | —                       | RESOLVED    | Polling policies segmented                                                  |
| F-I-2                                          | 3           | ✓       | —                       | RESOLVED    | Live verification matches intervals                                         |
| F-I-3                                          | 3           | 🟡      | C 8.5                   | TODO        | No visibilitychange pause doc                                               |
| F-I-4                                          | 3           | 🟠      | C 8.5                   | TODO        | invalidateResource dead/abandoned                                           |
| F-I-5                                          | 3           | ✓       | C 7.6                   | TODO        | TanStack dedup confirmed (validate same-key)                                |
| F-I-6                                          | 3           | 🟢      | C 8.5                   | TODO        | refetchIntervalInBackground not explicit                                    |
| F-I-7                                          | 3           | 🟡      | C 8.5                   | TODO        | gcTime not set on listPolicy/detailPolicy                                   |
| F-I-8                                          | 3           | ✓       | —                       | RESOLVED    | Retry policy excludes 4xx                                                   |
| F-D-1                                          | 4           | 🔴      | —                       | RESOLVED    | API stale binary — restart 2026-05-25                                       |
| F-D-2                                          | 4           | 🟠      | —                       | RESOLVED    | Composite NotFound — 473de2a2 + 9e88114b                                    |
| F-D-3                                          | 4           | 🟡      | C 5.1 / C 7.1           | TODO        | Detail page H1 heading inconsistency                                        |
| F-D-4                                          | 4           | 🟡      | C 7.2                   | TODO        | Polling indicator absent on detail pages                                    |
| F-D-5                                          | 4           | 🟡      | —                       | SKIP        | E5 empty-state spot-check unverified (low-pri)                              |
| F-AE-1                                         | 4           | 🟢      | C 8.6                   | TODO        | favicon.ico 404                                                             |
| F-AE-2                                         | 4           | 🟢      | —                       | RESOLVED    | try/catch inventory baseline                                                |
| F-AE-3                                         | 4           | 🟡      | C 8.4                   | TODO        | SectionErrorBoundary inconsistent coverage                                  |
| F-AE-4                                         | 4           | 🟡      | C 8.4                   | TODO        | Error interceptor flattens envelope (recap)                                 |
| F-AE-5                                         | 4           | 🟠      | —                       | RESOLVED    | Composite NotFound err — 473de2a2 + 9e88114b                                |
| F-AE-6                                         | 4           | 🟠      | C 8.4                   | TODO        | Silent shape-mismatch no console signal                                     |
| F-AE-7                                         | 4           | 🟢      | C 8.4                   | TODO        | No global error reporter                                                    |
| F-U-1                                          | 4           | 🟡      | C 2.2                   | TODO        | SectionCard wrong home                                                      |
| F-U-2                                          | 4           | 🟡      | C 2.1                   | TODO        | Inline toFixed/toLocaleString 10 sites                                      |
| F-U-3                                          | 4           | 🟠      | C 2.1                   | TODO        | Truncation re-impls 6 sites                                                 |
| F-U-4                                          | 4           | 🟠      | C 2.1                   | TODO        | STROOPS_PER_XLM 2 constants                                                 |
| F-U-5                                          | 4           | 🟡      | C 2.3                   | TODO        | EmptyState minor reuse violation                                            |
| F-X-1                                          | 4           | 🟡      | C 2.2                   | TODO        | assetLegLabel cross-folder reach                                            |
| F-X-2                                          | 4           | 🟢      | C 2.2                   | TODO        | web/src/pages/detail/ single-file                                           |
| F-X-3                                          | 4           | 🟡      | —                       | RESOLVED    | usePageHandlers shared chunk (positive baseline)                            |
| F-X-4                                          | 4           | 🟡      | C 6.4                   | TODO        | Hooks colocated in two places (document)                                    |
| F-X-5                                          | 4           | 🟢      | C 2.2                   | TODO        | web/src/utils/ single-file                                                  |
| F-AL-1                                         | 4           | 🟡      | C 5.2                   | DEFER-M2    | tx-detail selectedIndex useState (borderline)                               |
| F-AL-2                                         | 4           | 🟢      | C 6.4                   | TODO        | useDetailMode parallel pattern doc                                          |
| F-AN-1                                         | 4           | 🟡      | —                       | DEFER-M2    | Strkey vs hex strategy (partly resolved 0264; remainder bidirectional util) |
| F-AN-2                                         | 4           | 🟢      | —                       | RESOLVED    | XDR rendering inventory clean baseline                                      |
| F-AN-3                                         | 4           | 🟡      | C 7.1                   | TODO        | Op-type label single source; icon mapping absent (Figma check)              |
| F-AN-4                                         | 4           | 🟢      | —                       | RESOLVED    | SEP-1 TOML enrichment OK                                                    |
| F-AN-5                                         | 4           | 🟡      | C 6.4                   | TODO        | Soroban-era ledger detection absent (document)                              |
| F-AN-6                                         | 4           | 🟢      | C 6.4 / C 11.1          | TODO        | Mainnet/Testnet config single-env — REGRESSED by `06ab34cc`: NetworkToggle added but NON-FUNCTIONAL (fake toggle, no apiBaseUrl/query-key change, invisible on `/`); worse than no-toggle baseline. See F-DP-1 / card 11.1 (wire-or-hide) + design-parity-impact |
| F-AN-7                                         | 4           | 🟠      | C 2.1                   | TODO        | Stroop/XLM 2-place (recap F-U-4)                                            |
| F-AN-8                                         | 4           | 🟠      | —                       | RESOLVED    | Strkey canonical convention — 473de2a2 (0264)                               |
| F-AE-1..F-AE-7                                 | 4           | various | (above)                 | (above)     | (see individual rows)                                                       |
| F-A-1                                          | 5           | 🟡      | —                       | RESOLVED    | Spec drift 0246 Phase 3 dropped (positive baseline)                         |
| F-A-2                                          | 5           | 🟡      | —                       | RESOLVED    | 0254 BREAKING wire rename clean (positive baseline)                         |
| F-A-3                                          | 5           | 🟡      | C 6.4                   | TODO        | ADR 0032 partial gap on 0254 (doc sync)                                     |
| F-A-4                                          | 5           | 🟡      | —                       | RESOLVED    | LP feature gold-standard exemplar (positive note)                           |
| F-A-5                                          | 5           | 🟡      | C 1.3                   | PARTIAL     | Contract list page gap (launch blocker) — nav added, page stubbed via PageStub (design_parity `06ab34cc`); list page still TODO. **R2 (PR #224 `fce0d666`): `/accounts` half RESOLVED — real `AccountsListPage` + `useAccountsList` shipped. `/contracts` half STILL TODO — `router/index.tsx:66` still `<PageStub>`.** Card stays PARTIAL until `/contracts` real list ships |
| F-A-6                                          | 5           | 🟢      | —                       | RESOLVED    | Tx-detail spec/ship chain clean                                             |
| F-A-7                                          | 5           | 🟢      | —                       | RESOLVED    | Deviation notes discipline excellent                                        |
| F-AH-1                                         | 5           | 🟡      | C 2.2                   | STALE       | PageStub.tsx dead orphan — FALSE post-`06ab34cc`: PageStub revived as `/accounts`+`/contracts` stub (2 live consumers); deletion gated behind card 1.3 |
| F-AH-2                                         | 5           | 🟡      | C 2.2                   | TODO        | Folder asymmetry                                                            |
| F-AH-3                                         | 5           | 🟡      | C 2.2                   | TODO        | SectionCard wrong home (recap)                                              |
| F-AH-4                                         | 5           | 🟢      | C 2.2                   | TODO        | web/src/utils/ single-file (recap)                                          |
| F-AH-5                                         | 5           | 🟢      | C 2.2                   | TODO        | web/src/pages/detail/ misnamed (recap)                                      |
| F-AH-6                                         | 5           | 🟢      | C 8.1                   | TODO        | No tests (cross-cite testing baseline)                                      |
| F-AH-7                                         | 5           | 🟢      | C 2.2                   | TODO        | web/src/search/ parallel folder                                             |
| F-AH-8                                         | 5           | 🟢      | C 2.2                   | TODO        | Page-root helpers mixed with \*Page.tsx                                     |
| F-Y-1                                          | 5           | 🟡      | —                       | DEFER-M2    | overrides.ts 890 LOC split (low stakes)                                     |
| F-Y-2                                          | 5           | 🟠      | C 2.1                   | TODO        | Debounce pattern duplicated 4×                                              |
| F-Y-3                                          | 5           | 🟢      | —                       | RESOLVED    | useEffect discipline good (baseline)                                        |
| F-Y-4                                          | 5           | 🟢      | —                       | DEFER-M2    | PoolCharts 268 LOC borderline                                               |
| F-Y-5                                          | 5           | 🟢      | —                       | RESOLVED    | Long files domain-justified (baseline)                                      |
| F-Y-6                                          | 5           | 🟡      | C 2.1                   | TODO        | Cross-cites formatter/truncation (recap)                                    |
| F-Z-1                                          | 5           | 🟡      | C 2.1                   | TODO        | Multiple formatter homes (recap)                                            |
| F-Z-2                                          | 5           | 🟢      | C 6.3                   | TODO        | Op-type enum hand-typed (backend coordination)                              |
| F-Z-3                                          | 5           | 🟢      | —                       | DEFER-M2    | Chip JSDoc @param polish                                                    |
| F-Z-4                                          | 5           | 🟢      | C 6.4                   | TODO        | frontend-data-flow wiki                                                     |
| F-AA-1                                         | 5           | 🟢      | —                       | SKIP        | Single-consumer abstractions (keep-or-trim toss-up)                         |
| F-AA-2                                         | 5           | 🟢      | —                       | RESOLVED    | Zero Redux/Zustand (positive baseline)                                      |
| F-AA-3                                         | 5           | 🟢      | —                       | RESOLVED    | useDebounced will broaden in C 2.1                                          |
| F-AA-4                                         | 5           | 🟢      | C 6.4                   | TODO        | useIntersectionObserver single-consumer wiki note                           |
| F-AA-5                                         | 5           | 🟢      | —                       | RESOLVED    | Provider count minimal (positive baseline)                                  |
| F-AA-6                                         | 5           | 🟢      | —                       | RESOLVED    | Hook proliferation bounded (positive baseline)                              |
| F-AB-1                                         | 5           | 🟡      | C 6.4                   | TODO        | useDetailMode divergence not in task body                                   |
| F-AB-2                                         | 5           | 🟡      | C 6.4                   | TODO        | Interval labels 0065 #5 spec not amended                                    |
| F-AB-3                                         | 5           | 🟢      | C 10.3                  | TODO        | 0251 B1 fix-by-hide root-cause fix                                          |
| F-AB-4                                         | 5           | 🟢      | C 8.7                   | TODO        | Sort-caret middle-ground designer sign-off                                  |
| F-AB-5                                         | 5           | 🟠      | C 2.1                   | TODO        | Cross-task formatter dups (recap symptom)                                   |
| F-AD-1                                         | 5           | 🟠      | C 2.1                   | TODO        | Leaked-concern 5+ file bug fixes                                            |
| F-AD-2                                         | 5           | 🟢      | C 6.4                   | TODO        | Onboarding doc polish                                                       |
| F-AD-3                                         | 5           | 🟢      | C 7.1                   | TODO        | 3 inline magic numbers (1500ms, 1062, 1064)                                 |
| F-AD-4                                         | 5           | 🟢      | —                       | RESOLVED    | Zero implicit-context surprises (baseline)                                  |
| F-AD-5                                         | 5           | 🟠      | C 8.1                   | TODO        | Zero test coverage (cross-cite)                                             |
| F-AC checks (AC-1..AC-14)                      | 5           | —       | (rolled up)             | (rolled up) | See F-A-1..F-A-7 above                                                      |
| F-EX-1                                         | 5 sweep     | 🟡      | C 5.4                   | TODO        | NFT minted_at_ledger plain text (Figma check)                               |
| F-EX-2                                         | 5 sweep     | 🟢      | C 5.2                   | TODO        | Pool chart metric/period useState                                           |
| F-W6-AG-1                                      | 6           | 🟠      | C 4.1                   | TODO        | Main bundle >500KB (recap)                                                  |
| F-W6-AG-2                                      | 6           | 🟠      | C 4.1                   | TODO        | LP detail chunk 300KB (recap)                                               |
| F-W6-AG-3                                      | 6           | 🟡      | C 7.1                   | TODO        | Transitions non-GPU — slight NEG from `06ab34cc` (NetworkToggle/sort-caret/Tabs add more `background-color`/`color`/`border-color` transitions; no move to transform/opacity) |
| F-W6-AG-4                                      | 6           | 🟢      | C 7.1                   | TODO        | 150ms transitions edge of hover rule                                        |
| F-W6-AG-5                                      | 6           | 🟡      | C 7.7                   | TODO        | No route-transition loading indicator                                       |
| F-W6-AG-6                                      | 6           | 🟢      | —                       | SKIP        | useMemo/useCallback spot-check informational                                |
| F-W6-AG-7                                      | 6           | 🟢      | —                       | RESOLVED    | TanStack staleTime/gcTime tuned (baseline)                                  |
| F-W6-AG-8                                      | 6           | 🟢      | —                       | RESOLVED    | Cache hit on navigate-back confirmed                                        |
| F-W6-AG-9                                      | 6           | 🟢      | C 7.6                   | TODO        | Polling home+header overlap                                                 |
| F-W6-AP-1                                      | 6           | 🟡      | C 2.3                   | TODO        | Loading pattern inconsistency                                               |
| F-W6-AP-2                                      | 6           | 🟢      | C 7.2                   | TODO        | Polling refresh silent                                                      |
| F-W6-AP-3                                      | 6           | 🟢      | C 2.3                   | TODO        | Error retry no distinct state                                               |
| F-W6-AP-4                                      | 6           | 🟢      | C 2.3                   | TODO        | Inline/overlay/full-page not standardised                                   |
| F-W6-V-1                                       | 6           | 🟠      | C 7.2                   | TODO        | DM-1 reconfirmed + all live pills lack freshness                            |
| F-W6-V-2                                       | 6           | 🟡      | C 7.2                   | TODO        | Backfill doesn't disable LIVE                                               |
| F-W6-V-3                                       | 6           | 🟢      | C 7.2                   | TODO        | Latest-ledger polling works (informational)                                 |
| F-W6-AK-1                                      | 6           | 🟡      | C 7.1 / C 11.2          | TODO        | 3 hardcoded hex constants — REGRESSED by `06ab34cc`: now 5 (AssetIcon adds `#724311`/`#fffcc2`). See F-DP-2 / card 11.2 |
| F-W6-AK-2                                      | 6           | 🟢      | C 7.1 / C 11.3          | TODO        | Z-index raw 0/1 no scale — REGRESSED by `06ab34cc`: shell adds raw `zIndex: 2` (AppShell/TopNav/SecondaryNav/Footer). See F-DP-3 / card 11.3 |
| F-W6-AK-3                                      | 6           | ✓       | —                       | RESOLVED    | Spacing scale consistent (baseline)                                         |
| F-W6-AK-4                                      | 6           | 🟢      | —                       | DEFER-M2    | Border-radius/shadow audit deferred                                         |
| F-W6-AK-5                                      | 6           | ✓       | —                       | RESOLVED    | CSS approach single (baseline)                                              |
| F-W6-AK-6                                      | 6           | 🟢      | C 7.1                   | TODO        | Theme tokens pervasive; tiny leakage (recap)                                |
| F-W6-F-1                                       | 6           | 🟡      | C 7.5                   | TODO        | NFT detail no h2/h3                                                         |
| F-W6-F-2                                       | 6           | 🟡      | C 7.4                   | DONE        | Filter slots lack accessible names — STALE: already had `aria-label`+`placeholder` at `06ab34cc^` (pre-merge); NOT a design_parity closure. Re-verify on develop then archive |
| F-W6-F-3                                       | 6           | 🟢      | —                       | RESOLVED    | First Tab focus visible (baseline)                                          |
| F-W6-F-4                                       | 6           | 🟢      | C 7.4                   | TODO        | Header search lacks aria-label/id — only possibly-open residual of card 7.4 (filter a11y stale-fixed); confirm in re-verify |
| F-W6-F-5                                       | 6           | 🟢      | —                       | RESOLVED    | Copy buttons aria-label correct (baseline)                                  |
| F-W6-F-6                                       | 6           | 🟢      | C 8.1                   | DEFER-M2    | Lighthouse a11y audit not run                                               |
| F-W6-F-7                                       | 6           | 🟢      | C 7.8                   | TODO        | Reduced-motion not verified                                                 |
| F-W6-F-8                                       | 6           | 🟢      | C 7.8                   | TODO        | No keyboard trap test on modals                                             |
| F-W6-CH-1                                      | 6           | 🟡      | C 7.1                   | TODO        | Status badges color+text, no shape icon — NOT closed by `06ab34cc` (no checkmark/X icon added) |
| F-W6-CH-2                                      | 6           | 🟢      | C 7.1                   | PARTIAL     | Operation type chips text-only (informational) — design_parity `06ab34cc` adds NEW Classic/SAC + protocol_version chips (tangential, not op-type-on-tx grouping) |
| F-W6-RESPONSIVE-1                              | 6           | 🟠      | C 8.3                   | RESOLVED    | design_parity 06ab34cc + live re-verify 2026-05-28: 41/42 no doc-scroll, 802px root cause gone |
| F-W6-RESPONSIVE-2                              | 6           | 🟡      | C 8.3                   | RESOLVED    | tables in overflowX:auto; table→card transform = separate optional enhancement |
| F-W6-RESPONSIVE-3                              | 6           | 🟠      | C 11.5                  | TODO        | user decision 2026-05-28: REQUIRE hamburger <768px; scroll-nav alt rejected → card 11.5. **R2 (PR #224 merge `35ac27c0`) responsive nav tweaks did NOT add hamburger — grep MenuIcon/Drawer/aria-label="Open menu" = 0. Remains TODO.** |
| F-W6-RESPONSIVE-4                              | 6           | 🟠      | C 11.6                  | TODO        | still failing live; 105/106 elements <44px @375 → card 11.6. **R2 (PR #224 merge `35ac27c0`) did NOT enlarge touch targets — no sizing pass. Remains TODO.** |
| F-W6-RESPONSIVE-5                              | 6           | 🟡      | C 11.7                  | RESOLVED    | search page overflow <660px — **RESOLVED (page overflow mitigated, live-confirmed 2026-05-29): `/search?q=test` @375 `documentElement.scrollWidth = 364 ≤ 375`, NO page-level scroll (R1/R2 ~644px prediction REFUTED). 651px category-card row scrolls within `overflow-x:auto` container (same mitigation as RESPONSIVE-2). Residual per-card reflow = optional NICE enhancement, not a bug.** |
| F-W6-NOTFOUND-1                                | 6           | 🟡      | C 5.1                   | TODO        | NotFound missing h1 — **VERIFIED BROKEN live 2026-05-29: catch-all 404 (`/foobar`) AND account-404 have NO h1 (`headings: []`) post-AppShell restructure; EmptyState/404 restyle is styled but heading-less. STAYS TODO** |
| F-W6-NOTFOUND-2                                | 6           | 🟡      | C 5.3                   | TODO        | Sub-section queries fire on parent 404                                      |
| F-W6-E0-1                                      | 6           | 🟠      | C 1.1                   | TODO        | Footer dead spans (recap)                                                   |
| F-W6-E0-2                                      | 6           | 🟠      | C 7.2                   | TODO        | Footer hardcoded operational (recap)                                        |
| F-W6-E0-3                                      | 6           | 🟡      | C 11.5                  | TODO        | No hamburger at mobile — user decision 2026-05-28: REQUIRE hamburger <768px (scroll-nav alt rejected); → card 11.5 (see F-W6-RESPONSIVE-3) |
| F-W6-E0-4                                      | 6           | 🟡      | C 7.1                   | TODO        | Header search placeholder 4 vs hint 5                                       |
| F-W6-E0-5                                      | 6           | 🟢      | C 7.6                   | TODO        | Header polling duplicates home                                              |
| F-W6-E1-1                                      | 6           | 🟡      | C 7.2                   | TODO        | LIVE badge always on (recap)                                                |
| F-W6-E1-2                                      | 6           | 🟢      | C 7.1                   | TODO        | Hero+header search visually identical                                       |
| F-W6-E1-3                                      | 6           | 🟢      | C 7.6                   | TODO        | Home stats strip duplicated (informational)                                 |
| F-W6-E1-4                                      | 6           | 🟡      | C 5.4                   | TODO        | Home ledger hash not a link                                                 |
| F-W6-E2-1                                      | 6           | 🟢      | C 7.1                   | TODO        | "Transactions list" vs nav "Transactions"                                   |
| F-W6-E2-2                                      | 6           | 🟢      | C 7.1                   | TODO        | "All operations type" typo — NOT closed by `06ab34cc` (TransactionFilters.tsx unchanged) |
| F-W6-E3-1                                      | 6           | 🟢      | C 7.1                   | TODO        | Memo "—" semantic improvement                                               |
| F-W6-E3-2                                      | 6           | 🟢      | C 7.1                   | TODO        | Normal/Advanced tabs no description                                         |
| F-W6-E3-3                                      | 6           | 🟡      | C 5.1 / C 8.3           | PARTIAL     | Page horiz scroll mobile (covered by responsive) — design_parity `06ab34cc` removed 802px root cause (code-verified); live re-verify pending |
| F-W6-E5-1                                      | 6           | 🟢      | C 7.1                   | TODO        | Prev/Next ledger no disabled at boundary                                    |
| F-W6-E6-1                                      | 6           | 🟡      | C 5.3                   | TODO        | Sub-section queries fire on 404 (account)                                   |
| F-W6-E6-2                                      | 6           | 🟢      | C 5.1                   | TODO        | NotFound no h1 (account)                                                    |
| F-W6-E7-1                                      | 6           | 🟡      | C 7.4                   | DONE        | Two unlabeled filter slots /assets — STALE: aria-label+placeholder present at `06ab34cc^` (pre-merge); re-verify then archive |
| F-W6-E7-2                                      | 6           | 🟢      | C 7.1                   | PARTIAL     | Asset icon "?" fallback — design_parity `06ab34cc`: AssetIcon now color-coded by kind + 2-line header; "?" fallback unchanged |
| F-W6-E7-3                                      | 6           | 🟢      | C 7.1                   | TODO        | Asset detail link uses composite ID for SAC                                 |
| F-W6-E8-1                                      | 6           | 🟢      | C 7.1                   | PARTIAL     | Asset Metadata sparse — design_parity `06ab34cc` adds Domain row (home_page hostname); still no full SEP-1 TOML |
| F-W6-E8-2                                      | 6           | 🟢      | C 7.1                   | TODO        | Holder count not linkable                                                   |
| F-W6-E9-1                                      | 6           | 🟡      | C 5.3                   | TODO        | Sub-section queries fire on 404 (contract)                                  |
| F-W6-E9-2                                      | 6           | 🟢      | C 7.1                   | TODO        | Invocations+Events no empty-state message                                   |
| F-W6-E9-3                                      | 6           | 🟡      | C 5.1                   | TODO        | NotFound h1 inconsistent (contract)                                         |
| F-W6-E10-1                                     | 6           | 🟡      | C 7.4                   | DONE        | Four unlabeled filter slots /nfts — STALE: aria-label+placeholder present at `06ab34cc^` (pre-merge); re-verify then archive |
| F-W6-E10-2                                     | 6           | 🟢      | C 7.1                   | TODO        | NFT row token IDs inline text                                               |
| F-W6-E10-3                                     | 6           | 🟡      | C 5.4                   | TODO        | NFT row Contract ID plain text                                              |
| F-W6-E11-1                                     | 6           | 🟡      | C 7.5                   | TODO        | NFT detail no h2/h3                                                         |
| F-W6-E11-2                                     | 6           | 🟢      | C 7.1                   | PARTIAL     | NFT Traits "Metadata unavailable" no guidance — design_parity `06ab34cc`: NFT *media* empty-state improved; Traits guidance NOT improved |
| F-W6-E11-3                                     | 6           | 🟡      | C 5.4                   | TODO        | NFT Contract ID in Details plain text                                       |
| F-W6-E12-1                                     | 6           | 🟡      | C 7.1                   | TODO        | Pool ID truncation twice per row                                            |
| F-W6-E12-2                                     | 6           | 🟢      | C 7.1                   | TODO        | "Any TVL" filter looks like loading                                         |
| F-W6-E13-1                                     | 6           | 🟠      | C 7.3                   | TODO        | Pool participants share % full precision — **ILLUSORY CONFIRMED LIVE 2026-05-29: pool `LD5MMO2Q…` renders `33.3333333333333333%` raw. `formatAmount(_, 2)` minDecimals ≠ rounding. NOT fixed; needs API pre-round OR FE `Number(x).toFixed(2)`. STAYS TODO** |
| F-W6-E13-2                                     | 6           | 🟢      | C 5.1                   | TODO        | Pool NotFound no h1                                                         |
| F-W6-E13-3                                     | 6           | 🟢      | C 7.1                   | TODO        | Pool tx operation type plain text — UNCHANGED by `06ab34cc` (LP-detail recent-tx + home op-type not in diff) |
| F-W6-E14-1                                     | 6           | 🟢      | C 7.1                   | TODO        | Empty-state hint at ?q= no examples                                         |
| F-W6-E14-2                                     | 6           | 🟢      | C 7.1                   | TODO        | Search has two clear buttons                                                |
| F-W6-E14-3                                     | 6           | 🟢      | —                       | SKIP        | First Tab lands on header search (informational)                            |
| F-DP-1                                         | design_parity | 🟠   | C 11.1                  | TODO        | NetworkToggle non-functional — fake Mainnet/Testnet toggle (no apiBaseUrl/query-key change), invisible on `/`. Introduced by `06ab34cc`. Wire OR hide. **VERIFIED-FAKE live 2026-05-29: clicking Testnet flips `aria-pressed` only; no URL/banner/refetch; only request is LiveIndicator poll to same Mainnet host. STILL FAKE — STAYS TODO.** See design-parity-impact-2026-05-29 §Live re-verify |
| F-DP-2                                         | design_parity | 🟠   | C 11.2                  | TODO        | AssetIcon hardcoded hex `#724311`/`#fffcc2` (sac kind) — regresses F-AK-1 (3→5). Introduced by `06ab34cc`. Move to theme tokens. **R2 (PR #224): `#724311`/`#fffcc2` confirmed token VALUES but bound raw not via theme.palette; `assetColor.ts` touch was red herring (already uses tokens). Regression persists at AssetIcon.** |
| F-DP-3                                         | design_parity | 🟠   | C 11.3                  | TODO        | Raw `zIndex: 2` added across shell (AppShell/TopNav/SecondaryNav/Footer) — regresses F-AK-2. Introduced by `06ab34cc`. Move to z-index scale |
| F-DP-4                                         | design_parity | 🟠   | C 11.4                  | TODO        | OperationFlowTree collapse/expand removed (now flat w/ dashed connectors) — verify vs Figma; restore if regression. Introduced by `06ab34cc`. **Flat render CONFIRMED LIVE 2026-05-29 (tx `7b9bacc8…` Advanced: 0 expand/collapse, no chevron); nested-tree verify BLOCKED — local dataset 0 soroban / 0 multi-op txs (all 38 single-op). Figma sign-off still pending. STAYS TODO.** |
| Z-1 Spot 5                                     | 5           | 🟢      | C 6.3                   | TODO        | Op-type enum hand-typed (cross-cite F-Z-2)                                  |
| Z-1 Spot 1                                     | 5           | A       | C 8.4                   | TODO        | Error envelope flatten (cross-cite F-AF-1)                                  |
| 0061 #4                                        | arch        | 🟢      | C 8.7                   | TODO        | Sort caret middle-ground sign-off                                           |
| 0065 #5                                        | arch        | 🟡      | C 6.4                   | TODO        | Interval labels spec drift                                                  |
| 0073 #5                                        | arch        | 🟡      | C 6.3                   | TODO        | Balances SAC vs Classic distinction (backend)                               |
| 0075 #6                                        | arch        | 🟡      | C 6.3                   | TODO        | interface_metadata hand-typed                                               |
| 0077 #9                                        | arch        | ✓       | —                       | RESOLVED    | Pool-id strkey 60 LOC justified                                             |
| 0077 #12 #13                                   | arch        | ✓       | —                       | RESOLVED    | assetLegLabel/classifyLpTx hard-fail justified                              |
| 0238 #5                                        | arch        | 🟡      | C 6.4                   | TODO        | cursorParam multi-cursor ADR gap                                            |
| 0251 B1                                        | arch        | 🟢      | C 10.3                  | TODO        | linked=false fix-by-hide root cause                                         |
| 0059 Future Work (live stats)                  | arch        | —       | —                       | RESOLVED    | Wired via 0066 (TopNav still shows MOCK_STATS — re-verify in C 7.6 / 8.6)   |
| 0059 Future Work (responsive nav)              | arch        | —       | C 8.3                   | TODO        | Hamburger menu                                                              |
| 0061 FW (libs/ui vitest)                       | arch        | —       | C 8.1                   | TODO        | 0226 promote                                                                |
| 0062 FW (validators → libs/domain)             | arch        | —       | C 6.2                   | TODO        | Spawn                                                                       |
| 0062 FW (IdentifierDisplay router Link audit)  | arch        | —       | C 6.2                   | TODO        | Spawn                                                                       |
| 0067 FW (route param validation per page)      | arch        | —       | C 6.2                   | TODO        | Partly absorbed by 0251                                                     |
| 0068 FW (table sorting)                        | arch        | —       | C 6.2                   | TODO        | Gated on backend sort param                                                 |
| 0068 FW (populated-data diff)                  | arch        | —       | —                       | RESOLVED    | Absorbed into 0251/0257                                                     |
| 0069 FW (libs/ui error/empty divergence)       | arch        | —       | C 6.2                   | TODO        | Spawn                                                                       |
| 0069 FW (operation pill colour confirm)        | arch        | —       | C 8.7                   | TODO        | Designer sign-off                                                           |
| 0069 FW (OpenAPI op_type enum backend)         | arch        | —       | C 6.3                   | TODO        | Backend task                                                                |
| 0072 FW (hoist Button + formatFee timestamp)   | arch        | —       | C 2.1 / C 2.2           | TODO        | Covered by format/folder cards                                              |
| 0072 FW (URL-synced cursor)                    | arch        | —       | —                       | RESOLVED    | 0238                                                                        |
| 0075 FW (contracts list page)                  | arch        | —       | C 1.3                   | TODO        | Launch blocker                                                              |
| 0075 FW (events count for tab pill)            | arch        | —       | C 6.2                   | TODO        | Backend task                                                                |
| 0075 FW (wasm_interface_metadata JSONB doc)    | arch        | —       | C 6.3                   | TODO        | Backend task                                                                |
| 0075 FW (SAC SEP-41 stub)                      | arch        | —       | C 6.2                   | TODO        | Spawn                                                                       |
| 0076 FW (NFT trait rarity)                     | arch        | —       | —                       | RESOLVED    | 0229 spawned                                                                |
| 0077 FW (Tx Amount column on PoolTransactions) | arch        | —       | C 6.2                   | TODO        | Gated on 0247                                                               |
| 0077 FW (chart series wiring)                  | arch        | —       | C 10.1                  | TODO        | Gated on 0199                                                               |
| 0077 FW (per-leg icon_url backend)             | arch        | —       | C 6.2                   | TODO        | Spawn                                                                       |
| 0077 FW (Playwright CLI for LP pages)          | arch        | —       | C 8.1                   | TODO        | Gated on 0226                                                               |
| 0077 FW (LP senior-eye 6 items)                | arch        | —       | C 6.2                   | TODO        | Bulk spawn batch                                                            |
| 0238 FW (backend prev_cursor)                  | arch        | —       | —                       | RESOLVED    | 0254                                                                        |
| 0238 FW (unit tests useCursorPagination)       | arch        | —       | C 8.1                   | TODO        | Gated on 0226                                                               |
| 0238 FW (Playwright smoke 11 pages)            | arch        | —       | C 8.1                   | TODO        | Gated on 0226                                                               |
| 0238 FW (ADR multi-cursor)                     | arch        | —       | C 6.4                   | TODO        | Cross-cite                                                                  |
| 0251 FW (ScVal decoder Contract Events)        | arch        | —       | C 6.2                   | TODO        | Spawn                                                                       |
| 0251 FW (network runtime toggle)               | arch        | —       | C 6.2                   | TODO        | Spawn (post-launch)                                                         |
| 0251 FW (Searchable Autocomplete ops)          | arch        | —       | C 6.2                   | TODO        | Spawn                                                                       |
| 0251 FW (B4 fake-XLM design redo)              | arch        | —       | C 6.2                   | TODO        | Spawn                                                                       |
| Out of scope O                                 | rdme        | —       | C 8.1                   | TODO        | testing baseline (= C 8.1)                                                  |
| Out of scope N                                 | rdme        | —       | C 9.1                   | TODO        | i18n                                                                        |
| Out of scope AJ                                | rdme        | —       | C 9.1                   | TODO        | Asset optimization (covered partly by C 4.1)                                |
| Out of scope AT                                | rdme        | —       | C 9.1                   | TODO        | Animation polish                                                            |
| Out of scope S                                 | rdme        | —       | C 9.1                   | TODO        | Browser compat matrix                                                       |
| Out of scope T                                 | rdme        | —       | C 9.1                   | TODO        | Production parity                                                           |
| Out of scope BR                                | rdme        | —       | C 9.1                   | TODO        | OG / Twitter cards                                                          |
| Out of scope BM                                | rdme        | —       | C 9.1                   | TODO        | Memory leaks research                                                       |
| Out of scope BJ                                | rdme        | —       | C 9.1                   | TODO        | WebSocket / SSE                                                             |
| Out of scope BV                                | rdme        | —       | C 9.1                   | TODO        | PWA                                                                         |
| Out of scope BZ                                | rdme        | —       | C 9.1                   | TODO        | GDPR                                                                        |
| Out of scope CE                                | rdme        | —       | C 9.1                   | TODO        | Command palette                                                             |
| Out of scope CF                                | rdme        | —       | C 9.1                   | TODO        | CSV/JSON export                                                             |
| Out of scope BO                                | rdme        | —       | —                       | SKIP        | Session replay (skip per user)                                              |
| Muxed M→G redirect                             | post-Gate-B | —       | —                       | SKIP        | No ecosystem precedent                                                      |
| Asset code-issuer composite redirect           | post-Gate-B | —       | —                       | SKIP        | No ecosystem precedent                                                      |
| SearchResponse::Redirect refactor              | post-Gate-B | —       | —                       | RESOLVED    | Shipped by 0271 `5d7484b1` (FE owns singleton; wire collapsed to Results)   |
| F-EX-3 PoolKpiStrip (extends F-K-2)            | 5 sweep     | 🟠      | —                       | RESOLVED    | a5f15166 (0263)                                                             |
| F-EX-4 PoolsTable reserves (extends F-K-2)     | 5 sweep     | 🟠      | —                       | RESOLVED    | a5f15166 (0263)                                                             |
| Issues Encountered worktree gotchas wiki       | arch        | —       | C 6.4                   | TODO        | Spawn DOCS wiki entry                                                       |
| NFT search-404 regression (0264 carry-over)    | 0270        | 🟠      | —                       | RESOLVED    | 6421d3d7 + 69d9f529                                                         |

(Appendix row count tracked above — see report. +4 design_parity regression rows F-DP-1..F-DP-4 appended 2026-05-27 per design-parity-impact-2026-05-27.md.)

(design_parity ROUND 2 annotation pass 2026-05-29 per design-parity-impact-2026-05-29.md / PR #224 / merge `35ac27c0`: no new rows added — no new regressions. Annotated in place: cards 1.3 / 2.2 / 4.1 / 7.3 / 11.1 / 11.2 / 11.4 / 11.5 / 11.6 / 11.7; appendix rows F-A-5, F-P-1, F-W6-E13-1, F-DP-1/2/4, F-W6-RESPONSIVE-3/4/5. Only flip: `/accounts` sub-item DONE within card 1.3 (`fce0d666`); card 1.3 stays PARTIAL — `/contracts` still PageStub. Card 7.3 stays TODO — share-% R2 fix is ILLUSORY (formatAmount minDecimals ≠ rounding). 5 R2 live-re-verify items added to Pending-live-verification block.)

(LIVE re-verify pass 2026-05-29 per design-parity-impact-2026-05-29.md §Live re-verify 2026-05-29 — live Playwright, R1+R2 merged, viewports 1280+375. Status flips applied: **card 11.7 / F-W6-RESPONSIVE-5 → RESOLVED** (page overflow GONE live, scrollWidth 364 ≤ 375; residual per-card reflow = optional NICE, same treatment as RESPONSIVE-2); **card 1.3 `/accounts` sub-item → DONE live-verified** (drop "pending re-verify"; card 1.3 OVERALL stays PARTIAL — `/contracts` confirmed live stub). Hardened-but-STILL-TODO: 7.3/F-W6-E13-1 (share-% ILLUSORY CONFIRMED LIVE `33.33…%`), 11.1/F-DP-1 (NetworkToggle VERIFIED-FAKE), 5.1/F-E-3+F-W6-NOTFOUND-1 (catch-all 404 NO main AND NO h1, live-confirmed), 11.4/F-DP-4 (flat confirmed but nested verify BLOCKED — 0 soroban/multi-op txs in local data). Pending-live-verification checklist items marked VERIFIED. No new regressions; desktop sweep 9 routes clean.)

## End of queue

When all `TODO` cards are `DONE`, this file represents the closed-state of audit 0257. The single elastic task `0XXX_FEATURE_audit-0257-closing` archives with reference to this queue.
