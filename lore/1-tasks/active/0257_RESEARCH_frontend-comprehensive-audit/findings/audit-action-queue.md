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

> **0272 closure merge note (2026-06-01, PR #230 merge `016e6a6d`, develop merge into `research/0257` = ZERO conflicts).** The elastic closure task **0272** (now `archive/`, `status: completed`) shipped the pre-launch fix subset and **self-reconciled this queue during its own closure** — card STATUS + sub-checklists + appendix already reflect its work. Post-merge impact pass (code-verified against merged prod) found **0 residual flips needed**. What 0272 landed:
>
> - **NetworkToggle removal** (`e9122732`) → **F-DP-1 / card 11.1 RESOLVED** — component + wiring deleted (not hidden); 0 source refs remain (stale `libs/ui/dist/` type defs are compiled artifacts only — non-blocking, worth a `dist/` rebuild/gitignore check).
> - **Hardcoded hex → design tokens** (`0139a8a3`) → **F-DP-2 / card 11.2 RESOLVED** — AssetIcon hex gone; only `libs/ui/src/theme/colors.ts` holds hex (correct token source).
> - **C2.1 + C2.4 formatter/truncate/debounce consolidation** → cards 2.1 + 2.4 DONE — `libs/ui/src/format/{amount,numbers,stroops}.ts` + `libs/ui/src/hooks/useDebouncedDraft.ts` created; `web/src/pages/format.ts` + `transaction-detail/shared/formatFee.ts` deleted. (`web/src/pages/transactions/formatters.ts` survives by design — now holds only `formatAbsoluteUtc`, out of 2.1 scope.)
> - **Catch-all 404 dedup + typed NFT not-found** → cards 5.1 + 5.3 DONE — `web/src/pages/NotFoundPage.tsx` + `RouteErrorBoundary.tsx`.
> - **Live status indicator** → card 7.2 DONE (FE scope) — `web/src/api/hooks/useLiveStatus.ts` + `home/LiveIndicator.tsx`.
> - **Responsive hamburger nav** (`d184457f`) → card 11.5 DONE — `SecondaryNav.tsx` drawer <768px.
> - **PoolParticipants share-% — NOW GENUINELY FIXED** (card 7.3 DONE). Under design_parity R2 this was **VERIFIED ILLUSORY** (`formatAmount(_,2)` = minDecimals, no rounding — see line above). 0272 replaced it with `formatPercent(Number(row.share_percentage))` (`PoolParticipants.tsx:54-55`, real `.toFixed(2)` cap). Code-verified real this pass. **Supersedes the 2026-05-29 ILLUSORY verdict.**
>
> **Still TODO (0272 did NOT touch):** F-DP-3 / card 11.3 (raw z-index → named scale — `d184457f` even *added* a new raw `zIndex:3` on TopNav); F-DP-4 / card 11.4 (OperationFlowTree collapse — verify still data-blocked).
>
> **New findings from 0272 session (list-page filter/sort/search audit, 2026-06-01)** — 5 items, NOT previously in this queue. Spawned as active tasks **0274** (backend) + **0275** (contracts list) on develop:
>
> 1. **Accounts list = MOCK data (root cause of "account not found").** `web/src/api/hooks/useAccountsList.ts` generates 80 synthetic G-strkey accounts; `/v1/accounts` list endpoint NOT implemented → row click → real `GET /v1/accounts/{id}` → 404. **Important:** this partially un-resolves the card 1.3 "accounts half DONE" claim — the list renders but its data is fake. Real endpoint owned by **0274**.
> 2. **LP vs assets search inconsistency (real bug).** LP `filter[asset_code]` is EXACT (`liquidity_pools/queries.rs:340`); assets `filter[code]` is partial ILIKE (`assets/queries.rs:132`). Fix → LP ILIKE both legs. Owned by **0274**.
> 3. **Dead sort UIs.** Assets total-supply + ledgers sequence sort send `order` param the API ignores (fixed-order cursor). Remove arrows or add backend sort. → spawn backlog from develop.
> 4. **Silent no-op searches.** Transactions search only fires on full G-/C- strkey; NFT collection = exact match. UX: placeholder/empty-state hints. → spawn backlog from develop.
> 5. **Transaction type dropdown** single-select only (backend `filter[operation_type]` one string). Multi-select needs backend `IN (...)`. → spawn backlog from develop.
>
> Source: `archive/0272_FEATURE_audit-0257-closing.md` §Session findings + §Future Work. Items 3-6 not yet spawned (backlog, from develop) — now captured as **card 6.5 / F-0272S-1..6** (incl. F-0272S-6 architectural no-shared-search-semantics + per-page audit table, re-verified valid+current 2026-06-01). **0274/0275 currently lack `related_tasks: ['0257']` backlink** — add from develop.
>
> **Live re-verify 2026-06-01 (fresh dev server `:4201` from this worktree, post-merge HEAD `e3fe1968`).** All 0272 UI claims confirmed live — **0 surprises, 0 new findings**:
>
> - **Catch-all 404** (`/foobar-nonexistent-route`): `<main>` landmark **present** ✓ (was VERIFIED BROKEN pre-0272). h1 **absent** (title is `<span>` MuiTypography, 0 headings) — **matches the documented WONTFIX** (card 5.1: tag-only change, no visual diff, user declined 2026-05-29). Card 5.1 DONE is correct.
> - **NetworkToggle**: **gone** ✓ — no mainnet/testnet text, no toggle element on `/transactions` shell. F-DP-1 / card 11.1 RESOLVED confirmed live.
> - **Home** `/`: h1 present (1) ✓, live indicator rendering ("…ago") ✓, `<main>` ✓, no horizontal overflow (scrollWidth ≤ clientWidth @1422). Card 7.2 confirmed.
> - **Responsive @375**: hamburger button present (`aria-label="Open navigation menu"`), nav collapsed, `scrollWidth 364 ≤ 375` (no page overflow) ✓. Card 11.5 / F-W6-RESPONSIVE-1/3 confirmed live.
> - **Share-% @ pool detail** (`LD5MMO2Q…`, the same pool that was ILLUSORY on 2026-05-29): renders `1.00%` 2-decimal-capped, **no long-decimal raw values** ✓. `formatPercent` `.toFixed(2)` real. Card 7.3 RESOLVED confirmed live — **definitively supersedes the 2026-05-29 ILLUSORY verdict**.
>
> Still data-blocked (unchanged): F-DP-4 / card 11.4 OperationFlowTree nested-tree (local dataset 0 soroban/multi-op txs).

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
- **STATUS:** DONE (2026-05-29)

**Rationale.** The footer renders Terms of Service, Privacy Policy, Cookies, and external Resources links (GitHub, Stellar docs, Soroban docs, Stellar dashboard) as plain `<span>` elements with no `href`. Shipping a public block explorer with non-functional Terms/Privacy is a legal/compliance liability. Resources are a discoverability gap. Even the project's own GitHub link is missing. This was already flagged as Gate B fix-first but deferred to this queue.

**Scope.** Edit `libs/ui/src/layout/Footer.tsx`. Either (a) fill in real hrefs for all 7 items — needs legal team content for Terms/Privacy/Cookies — or (b) hide dead `<span>` items entirely until content ready. External links must use `target="_blank" rel="noopener noreferrer"` per F-H-5 pattern.

**Findings closed (sub-checklist):**

- [x] CA-1 — Terms of Service / Privacy Policy / Cookies **removed from footer entirely** (per user 2026-05-29) — dead spans deleted rather than wired (no legal content)
- [x] CA-2 — Resources wired: GitHub → repo, Stellar docs → `developers.stellar.org/docs`, Soroban docs → `developers.stellar.org/docs/build/smart-contracts`, Stellar dashboard → `dashboard.stellar.org` (2026-05-29)
- [x] CA-3 — `FooterLink` adds `target="_blank" rel="noopener noreferrer"` for external links (href without onClick); SPA Explorer nav links unaffected
- [x] F-W6-E0-1 — footer is shared across all routes; no dead `<span>` left (legal removed, Resources wired, Explorer nav uses onClick)

**Notes:** SKIP per user 2026-05-28 → DONE 2026-05-29. **CA-1 (legal)** resolved by deletion — no legal content/destination, so Terms/Privacy/Cookies removed from footer (not wired). **CA-2/CA-3 (Resources)** wired this session with external `target/rel`. The footer "All systems operational" badge was also removed this session (see 2026-05-29 session note). Footer now has zero dead spans. Uncommitted.

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

- **0272 closure note (2026-06-01, PR #230):** `/contracts` real-list half now owned by spawned active task **0275** (`active/0275_FEATURE_contracts-list-page-design-and-impl.md`). **Caveat on the accounts half:** 0272's list-page audit found `/accounts` list renders from **MOCK data** (`useAccountsList.ts` = 80 synthetic G-strkeys; `/v1/accounts` list endpoint not implemented → row click 404s). The list page UI is DONE but its data is fake — real endpoint owned by spawned task **0274**. So "accounts half DONE" is UI-only; data path still open under 0274.

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
- **STATUS:** DONE (2026-05-29 — debounce migration completed)

**Rationale.** Per audit's #1 maintenance-cost finding (F-AD-1): a single "change how addresses truncate" today requires editing 6 files. Plus 2 STROOPS_PER_XLM constants, 2 formatFee implementations, 10 inline toLocaleString sites, 4 toFixed bypasses, 4 debounce-pattern reimplementations. All are organic accretion across feature task boundaries that each got self-consistency but no cross-task DRY check. Phase 3 single-PR consolidation cuts ~10 audit findings in one atomic change. Junior maintenance cost drops from "moderate" to "low".

**Scope.** Create `libs/ui/src/format/` directory with: `stroops.ts` (single `STROOPS_PER_XLM_BIGINT` + `stroopsToXlmString` + canonical `formatFee` + `formatStroops`), `numbers.ts` (`formatInteger`, `formatTps`, `formatPercent`). Extend `libs/ui/src/identifiers/truncate.ts` to expose all 6 ad-hoc truncate variants via canonical `truncateMiddle(value, type)`. Extract `useDebouncedDraft<T>(value, onChange, delay)` from existing `useDebounced.ts`. Migrate all consumers: 6 truncation sites + 10 toLocaleString sites + 4 toFixed sites + 2 STROOPS + 2 formatFee + 4 debounce sites. Delete duplicated impls.

**Findings closed (sub-checklist):**

- [x] F-U-3 — 6 truncation re-impls → canonical `truncateMiddle` (shortId/shortStr/shortHash/shortenStrKey/truncateHex + inline slices)
- [x] F-U-4 — STROOPS_PER_XLM → single `STROOPS_PER_XLM_BIGINT`
- [x] F-U-2 — inline toFixed/toLocaleString → shared formatters (0 left)
- [x] F-J-2 — `toLocaleString('en-US')` sites → `formatAmount`/`formatInteger` (0 left)
- [x] F-J-3 — toFixed bypasses → canonical formatter
- [x] F-J-4 — STROOPS single shared util
- [x] F-J-7 — 6 truncation re-impls (cross-cite F-U-3)
- [x] F-J-16 — single `formatFee` (BigInt) — Number variant removed
- [x] F-J-17 — `formatStroops` single entry point
- [x] F-Y-2 — `useDebouncedDraft` extracted to `libs/ui/src/hooks/`; all 4 filter components migrated (AssetFilters, NftFilters DebouncedField, TransactionFilters, PoolsFilterBar) — inline draft+setTimeout removed. Hook now has 4 consumers; debounce-commit verified live on /assets (URL → `?code=USD` after pause). (2026-05-29)
- [x] F-Y-6 — recap
- [x] F-AB-5 — recap
- [x] F-AD-1 — leaked-concern (truncation now 1-file change)
- [x] F-AN-7 — recap of F-U-4
- [x] F-Z-1 — single formatter home (`libs/ui/src/format/`): `formatAmount`/`formatCompactAmount` migrated off the web-local `web/src/pages/format.ts` (deleted) + `pool-detail/helpers.ts` compact dup (removed) onto libs/ui; 8 consumers + PoolKpiStrip repointed. Output verified identical (thousands separators on /assets). (2026-05-29)
- [x] J-3 — **NOT a true duplicate of `formatCompactAmount`** (finding premise was wrong). TopNav's `formatNumber` is a deliberate hybrid: compact only at ≥1M (`8.4M`), full thousands below (`1,024` accounts, not `1K`) — the stat strip needs exact counts. `formatCompactAmount` compacts everything (`1.2K`), which would degrade the strip. Kept TopNav's `formatNumber` local (it already delegates the <1M path to the shared `formatInteger`); NOT consolidated by design. The one genuine number-format dup that DID exist — `PoolKpiStrip`'s `COUNT_FORMATTER = new Intl.NumberFormat('en-US')` — was swapped to the shared `formatInteger` (output-identical). (2026-05-29)

**Notes:** **2026-05-29: now genuinely DONE.** Two consolidations were initially mis-marked DONE (canonical home created but consumers not migrated + duplicate left alive), caught on review, then finished: (1) **debounce** — `useDebouncedDraft` had zero consumers; migrated all 4 filters, deleted inline draft+setTimeout; (2) **formatAmount** — libs/ui `format/amount.ts` had zero consumers while 8 pages still used the web-local `web/src/pages/format.ts`; migrated 8 pages + PoolKpiStrip onto libs/ui, deleted `format.ts` and the `pool-detail/helpers.ts` compact dup. Lesson: verify _consumers_, not file existence, before marking a consolidation done. **Exhaustive re-audit 2026-05-29** (grepped every pattern in scope — toLocaleString/toFixed/STROOPS/truncate re-impls/setTimeout-debounce/Intl.NumberFormat/duplicate formatter defs): all clean EXCEPT J-3 (resolved above — TopNav kept local by design + PoolKpiStrip COUNT*FORMATTER → formatInteger). `numbers.ts` (formatInteger/Tps/Percent) confirmed to have real consumers (not orphaned). `humanizeOp.shortId` is a kept multi-use wrapper delegating to `truncateMiddle`, not a re-impl. PoolCharts USD axis formatters left local (currency/chart-specific, out of scope). 2.1 genuinely complete after this pass. Done across two parts: (a) format/numbers/stroops + number/fee migration (committed in WIP checkpoint `03c11a1e`); (b) truncation consolidation + `…` ellipsis unification (committed `c57f7c4d`). **Emerged (2026-05-28):** made the single-glyph `…` the \_default* ellipsis in `truncateMiddle`, so every truncation app-wide (incl. IdentifierDisplay, previously `...`) now matches — global but consistent. Single-use truncate wrappers inlined; multi-use kept. **Still uncommitted as of 2026-05-28:** `libs/ui/src/format/{amount,index,stroops}.ts` + `libs/ui/src/hooks/` are still untracked, so the WIP checkpoints don't typecheck standalone — a follow-up commit must land them to make HEAD green. Other emerged session work tracked under the 2026-05-28 session note below.

---

### Session emerged work — 2026-05-28 (lore-0272)

Visual / consistency work done this session that wasn't a discrete card (mostly
design-parity + identifier follow-ups surfaced live). All in working tree;
commit state noted per item.

- **Asset codes render sans, not mono** — added `mono?: boolean` to
  `IdentifierDisplay` (default true for hashes/addresses/IDs); asset-code links
  (AssetsTable, AccountBalances, PoolsTable/PoolSummary/PoolKpiStrip legs) pass
  `mono={false}` → Inter, matching the amount beside them. Confirmed against
  Figma reserves (`980,000 USDC` uniform sans). Also added `fontSize` prop so
  inline legs match the 12px amount, and dropped `tone="inherit"` (gave
  underline-hover) so legs use the canonical gold hover. **Uncommitted.**
- **Hero gold gradient bleeds full-width** — glow was inside the constrained
  `<main>`, clipped to side margins while the grid bled full-width. Extracted
  `HomeHeroGlow` and mounted it in AppShell's full-bleed wrapper (home-gated),
  so it spills past the margins like the grid. **Uncommitted (HomeHeroGlow.tsx
  untracked).**
- **Footer "All systems operational" badge — REMOVED entirely** (per user
  2026-05-29). Was briefly wired to a shared `useLiveStatus()`
  (operational/degraded/down via a Footer `status` prop); user then decided
  it should disappear completely, so the `status` prop, `FooterSystemStatus`
  type, `STATUS_TONE` map and the AppShell wiring were all deleted — the pill
  is gone from the footer. `useLiveStatus()` stays (still feeds the home
  **LiveIndicator** pills). Card 7.2 **DM-1** is now N/A (badge deleted, not
  driven). **Uncommitted.**
- **Theme defaults to dark** (per user) — `ExplorerThemeProvider` default mode.
  **Committed in `03c11a1e`.**

**Open decisions (pending user):** pill label `DELAYED` → `STALE`/`BEHIND`;
footer status _logic_ (#2 ledger-freshness vs #3 real `/health` probe) + label
vocabulary (compact vs descriptive).

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
- [ ] F-X-1 — cross-folder reach `liquidity-pools/` ↔ `pool-detail/`. **Fresh-eyes 2026-05-29: it's BIDIRECTIONAL and wider than just `assetLegLabel`** — `pool-detail/PoolKpiStrip` imports `assetColor` from `liquidity-pools/`, `PoolDetailHeader` imports `AssetAvatar`/`FeePill` from `liquidity-pools/`; reverse, `liquidity-pools/PoolsTable` + `AssetAvatar` import `assetLegLabel`/`legHref` from `pool-detail/helpers`. The two folders are one feature split in two. Fix: hoist shared pool primitives (`assetColor`, `AssetAvatar`, `FeePill`, `helpers`) into a single `pools/shared/` (or merge the folders).
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
- **STATUS:** SKIP (2026-05-30 — largely already addressed; fresh-eyes verified)
  - The primitives this card proposed already exist and are consumed
    consistently: `TableSkeleton` (11 files), `CardSkeleton` (7),
    `DetailSkeleton` (3), `SearchSpinner` (1), `EmptyState`, plus the
    `QueryErrorState`/`RateLimitState` retry states from card 2.4.
  - Zero ad-hoc `CircularProgress`/`LinearProgress` in `web/src` → nothing
    to standardise for F-W6-AP-4. Proposed `SectionSkeleton`/`LoadingState`/
    `RetryingState` have 0 callers — building them = speculative primitives
    (the exact anti-pattern SM-8 just removed). F-W6-AP-1 (skeleton-vs-spinner
    not codified) → de-facto codified in code; a wiki note is the only
    residual, tracked under the acceptance-criteria wiki bullet.

**Rationale.** Empty / loading / retry states are reimplemented per page rather than consumed from `libs/ui`. Wave 6 loading-pattern audit identified inconsistent skeleton-vs-spinner choices, no shared `<TableSkeleton>` / `<SectionSkeleton>` primitive, silent polling refresh, no distinct retry state. Consolidating into shared primitives reduces cross-page visual drift.

**Scope.** Extend `libs/ui/src/states/` with: `<TableSkeleton rows={N}>`, `<SectionSkeleton>`, `<LoadingState variant="inline|overlay|full">`, `<RetryingState attempt={N} max={N}>`. Migrate consumers. Add subtle polling-refresh pulse to `LIVE` pills (paired with card 7.2).

**Findings closed (sub-checklist):**

- [ ] F-U-5 — Minor component-reuse violation
- [ ] F-W6-AP-1 — Loading pattern inconsistency: skeleton vs spinner choice not codified
- [ ] F-W6-AP-3 — Error retry has no distinct "retrying" state
- [ ] F-W6-AP-4 — Inline vs overlay vs full-page loading not standardised

**Notes:** **\_**

---

### 2.4 Post-merge DRY smells — duplicated render blocks + primitives (fresh-eyes audit 2026-05-29)

- **Type:** REFACTOR
- **Effort:** ~1d (incremental; #SM-1 alone is the bulk)
- **Severity / Class:** 🟡 C
- **Pre-launch:** SHOULD (incremental — not launch-blocking)
- **STATUS:** DONE (2026-05-30 — SM-1..10 landed; SM-11/12 resolved, see below)
  - Commits: `bb6bfc70` (SM-1..10 + detail-error unification), `3b1f740c`
    (NFT typed entity + search-debounce hoist = SM-12 close-out).
  - SM-11 (`sx` color string-token vs callback) = NOT DONE, re-scoped as a
    lint-rule candidate (out of pure-dedup scope; left as future nit).
  - Verified via 3-lens fresh-eyes review (per-file fan-out + /code-review +
    cross-file), 17 findings confirmed + actioned; ui 45 + web 85 tests green.

**Rationale.** A fresh-eyes architecture sweep on 2026-05-29 (after the
design*parity round-2 merge + this session's consolidation work) surfaced
a batch of \_incomplete-consolidation* smells: render blocks and tiny
primitives copy-pasted across many files instead of lifted to a shared
home — the same pattern 2.1 fixed for formatters/truncation, now found in
error-state JSX, status chips, timestamp/clipboard helpers, and the `Dash`
em-dash. Mostly low-risk pure refactors; #SM-1 removes ~250 lines alone.
Not the same as card 8.4 (which is about the _error envelope / interceptor
/ boundary coverage / reporter_), nor 2.3 (loading primitives) — these are
duplicated **render/logic blocks** built on the already-good primitives.

**Findings (sub-checklist):**

- [x] SM-1 — **Query error-state switch** → `QueryErrorState` (libs/ui), 14 sites (`bb6bfc70`). **Query error-state switch duplicated ~18×**: identical
      `classifyError(error)` → `rate-limit ? <RateLimitState> : transient ?
<TransientErrorState> : <GenericErrorState>` + centering `<Box py:8>` +
      `retry` closure, verbatim across 18 list/section files (LedgersListPage,
      TransactionsListPage, AssetsListPage, NftsListPage, LiquidityPoolsListPage,
      home/LatestLedgers, home/LatestTransactions, home/ChainOverview,
      contracts/{ContractInterface,ContractEvents,ContractInvocations},
      nft-detail/NftTransfers, accounts/AccountTransactions,
      pool-detail/{PoolParticipants,PoolTransactions}, assets/AssetTransactions,
      NftDetailPage, LedgerDetailPage). Fix: `<QueryErrorState kind onRetry/>`
      (or `renderQueryError(query)`) in `libs/ui/src/states/`. ~250 lines saved.
- [x] SM-2 — **DetailErrorState** (libs/ui), all 7 detail pages; non-missing delegates to QueryErrorState (`bb6bfc70`). **Detail NotFound-vs-Generic block duplicated 5×**:
      `isMissingResource(classifyError(err)) ? <NotFoundState entity> :
<GenericErrorState onRetry>` in AccountDetailPage:50, AssetDetailPage:53,
      ContractDetailPage:63, LiquidityPoolDetailPage:69, LedgerDetailPage:58.
      Fix: `<DetailErrorState entity error onRetry identifier/>`.
- [x] SM-3 — **`Dash` hoisted to `libs/ui/components/Dash.tsx`**, 13 sites migrated, cells.tsx no longer defines it (`bb6bfc70`). **`Dash` em-dash component duplicated 5×**: canonical exported
      `transactions/cells.tsx:7`; local copies in nfts/NftsTable:15,
      nft-detail/NftTransfers:33, transaction-detail/sections/SignaturesTable:30,
      transaction-detail/sections/TransactionSummary:20. Fix: hoist `Dash` to
      `libs/ui` (pure primitive), import everywhere.
- [x] SM-4 — **`StatusChip`** (libs/ui), replaces StatusCell + 2 hand-rolled, 8 sites (`bb6bfc70`). **Status Success/Failed chip 3 impls**: canonical `StatusCell`
      (transactions/cells.tsx:35) + hand-rolled in search/SearchResultRow:21-31,99
  - inline in transaction-detail/sections/TransactionSummary:123. Fix: export
    a shared `<StatusChip successful>` from `libs/ui`.
- [x] SM-5 — **TransactionSummary now imports canonical `formatAbsoluteUtc`** (`bb6bfc70`). NOTE: HighlightedJson `pad` is a JSON-indent helper (`INDENT.repeat`), NOT a zero-pad dup — left as-is. **UTC absolute-timestamp formatter duplicated**:
      `formatUtcAbsolute` (+`pad`) in TransactionSummary:59-74 ≈ canonical
      `formatAbsoluteUtc` in transactions/formatters.ts:1-21 (only `null` vs `—`
      sentinel differs); `pad` independently defined a 3rd time in
      advanced/HighlightedJson.tsx:24. Fix: import the canonical formatter.
- [x] SM-6 — **`useCopyToClipboard`** (libs/ui/hooks), CopyButton + XdrRow delegate (`bb6bfc70`). Per user: dropped the deprecated execCommand fallback (HTTPS/CloudFront-only). **Clipboard write/reset duplicated**: XdrRow.tsx:20-24
      reimplements CopyButton's copy+1500ms-reset (without the `execCommand`
      fallback). Fix: reuse `CopyButton` or extract `useCopyToClipboard()` in
      `libs/ui`.
- [x] SM-7 — **`FeeCell`** (web/pages/detail), shared by LedgerSummary + TransactionSummary via `primaryVariant`/`secondaryVariant` props (`bb6bfc70`). **Fee "XLM + (N stroops)" two-line cell duplicated ×2**:
      LedgerSummary:31 + TransactionSummary FeeCell:93-110, identical markup.
      Fix: shared `<FeeCell>`.
- [x] SM-8 — **Dropped `entityRoutes` + `isValidIdentifier`; demoted 5 internal-only exports from the public barrel** (`bb6bfc70`); dead sub-barrel re-exports (`STROOPS_PER_XLM_BIGINT`, `DEFAULT_TIME_SERIES_INTERVALS`, `useIntersectionObserver`) also removed post-review. **Dead / over-exposed `libs/ui` exports**: `entityRoutes`
      (routes.ts:41) + `isValidIdentifier` (validators.ts:30) have zero
      consumers → drop. Internal-only but exported from the public barrel
      (demote): `getIdentifierHref`, `useIntersectionObserver`,
      `DEFAULT_TIME_SERIES_INTERVALS`, `ELLIPSIS_CHAR`, `STROOPS_PER_XLM_BIGINT`.
      (`stroopsToXlmString` already demoted 2026-05-29.)
- [x] SM-9 — **`DEFAULT_DEBOUNCE_MS`** exported from `useDebouncedDraft` home, 5 filters drop their local `SEARCH_DEBOUNCE_MS` (`bb6bfc70`). NOTE: inline truncation literals (breadcrumb 4/4, event topic/data, ledger hash) are INTENTIONALLY ≠ entity defaults — left as-is. **`SEARCH_DEBOUNCE_MS = 300` redefined 4×** (the 4 filter
      components) + inline `{prefix,suffix}` truncation literals coexisting with
      `getDefaultTruncation` (e.g. ContractDetailPage `BREADCRUMB_TRUNCATION`).
      Fix: export `DEFAULT_DEBOUNCE_MS` from the `useDebouncedDraft` home; prefer
      `getDefaultTruncation(type)` over inline literals.
- [x] SM-10 — **`capitalize`** in `web/src/utils/text.ts`, 3 real sites migrated (`bb6bfc70`). NOTE: AssetAvatar/AssetIcon use `charAt(0).toUpperCase()` = INITIALS (no slice), not a capitalize dup; `reserveDotColor` left as-is. **`capitalize` inlined 4×** (NftEventBadge:31, ContractEvents
      EventTypeBadge:42, AssetIcon:52, AssetAvatar:26) + `reserveDotColor`
      (assetColor.ts:136) thin pass-through used inconsistently (PoolsTable uses
      it, PoolKpiStrip inlines `.dot`). Fix: tiny shared `capitalize`; pick one
      side for the dot color. (Low priority.)
- [ ] SM-11 — **NOT DONE (re-scoped → lint-rule candidate, out of pure-dedup scope).** **`sx` text-color style inconsistency**: string token
      `sx={{ color: 'text.tertiary' }}` (86× / 42 files) vs theme-callback
      `sx={(theme) => ({ color: theme.palette.text.tertiary })}` (45×), even
      co-existing in single files. Fix: standardise on the string-token form;
      reserve the callback for non-palette theme reads. (Lint-rule candidate.)
- [x] SM-12 — **`useDebounced` hoisted to `libs/ui/hooks`** beside useDebouncedDraft; search shares `DEFAULT_DEBOUNCE_MS`, dead `debounceMs` param dropped (`3b1f740c`). **two debounce hooks**: `libs/ui` `useDebouncedDraft`
      (draft+commit, 4 consumers) and `web/src/search/useDebounced.ts`
      (value-only, 1 consumer). Not strict dups, but both debounce hooks in
      different homes — consider hoisting `useDebounced` into `libs/ui` alongside.

**Notes:** All findings empirically verified (consumer/dup counts) by the
2026-05-29 fresh-eyes sweep. **SM-1 is the highest-ROI item** (~250 lines,
18 files, pure refactor). Overlaps: SM-1/SM-2/SM-6 are adjacent to card 8.4
(but 8.4 is envelope/interceptor/boundary/reporter, not these render-block
dups) — coordinate so the shared error components land once. The
`pool-detail/` ↔ `liquidity-pools/` **bidirectional** sibling coupling found
in the same sweep is folded into card 2.2 (extends F-X-1, which only noted
the one-way `assetLegLabel` reach). Overall FE health judged good — these are
incomplete consolidation, not bad architecture.

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
- **STATUS:** DONE (2026-06-01) — functional half (`<main>` landmark) complete + verified; h1 half is a deliberate non-fix per user, not outstanding work.
  - **Verified in code (2026-06-01):** catch-all `path: '*'` → `NotFoundPage`
    (`router/index.tsx:93-94`) renders inside AppShell `component="main"`
    (`AppShell.tsx:170`) with nav + footer. Screen-reader `<main>` landmark
    present on every unmatched route.
  - **h1 normalization — closed as WONTFIX (user, 2026-05-29):** adding `<h1>`
    to NotFound titles changes only the HTML tag, not the rendered look
    (Typography `variant` drives styling). User declined the non-visual a11y
    tweak; NotFound titles stay `<p>`. The h1 sub-findings below are marked as
    such — not deferred work.
- **resolution:** There was no catch-all route at all — unmatched URLs fell to the root `errorElement` (`RouteErrorBoundary`), rendering _outside_ AppShell. Added a `{ path: '*' }` child route in the AppShell `/` route → new `NotFoundPage` renders inside the `<main>` landmark with nav + footer. **Verified present post-merge (2026-05-30): `router/index.tsx` catch-all + `NotFoundPage.tsx` + AppShell `component="main"`.** The h1 part (EmptyState `titleComponent` + NotFoundState `<h1>`) was added then **REVERTED per user 2026-05-29** — no visual difference (Typography `variant` drives styling, only the tag changes) and the user declined a non-visual change; NotFound titles render `<p>`. **Original finding text:** catch-all 404 bypasses the `AppShell` `<main>` landmark — screen readers skip the page main, selector tests break. Additionally, NotFound pages on 4 of 5 detail routes lack an `<h1>`. _(design_parity R2's "live re-verify 2026-05-29: no main, no h1 — STAYS TODO" note is superseded: that branch lacked our catch-all NotFoundPage, which the merge brought → main landmark now present; h1 intentionally not done.)_

**Scope.** Wrap catch-all 404 in `AppShell` `<main>` landmark. Update `libs/ui/src/states/errors/NotFoundState.tsx` to render an `<h1>` (entity-typed). Verify all detail-route NotFound paths use the canonical state component.

**Findings closed (sub-checklist):**

- [x] F-E-3 — catch-all `path:'*'` NotFoundPage inside AppShell `<main>` (lore-0272; verified 2026-06-01)
- [x] F-W6-NOTFOUND-1 — h1 **WONTFIX per user 2026-05-29** (tag-only change, no visual diff)
- [x] F-W6-E3-3 — NotFound h1 — WONTFIX (h1 dropped)
- [x] F-W6-E5- — NotFound h1 — WONTFIX (h1 dropped)
- [x] F-W6-E6-2 — NotFound h1 — WONTFIX (h1 dropped)
- [x] F-W6-E9-3 — NotFound h1 — WONTFIX (h1 dropped)
- [x] F-W6-E13-2 — Pool NotFound h1 — WONTFIX (h1 dropped)
- [x] F-D-3 — h1 consistency — WONTFIX (h1 dropped)

**Notes:** `<main>`-landmark half DONE (catch-all NotFoundPage, verified post-merge). h1 half intentionally dropped (user, no visual diff). design_parity R2's "no main, no h1, STAYS TODO" verdict was correct on the research/0257 branch (it had no catch-all) but is **superseded by this merge** — our NotFoundPage supplies the `<main>` landmark. The remaining h1 a11y gap is a deliberate non-fix, not an oversight.

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
- **STATUS:** DONE (2026-06-01, live-verified)
  - **Approach changed from the planned `enabled`-param:** instead of threading
    an `enabled` arg through 6 hooks + prop-drilling it via the section
    components, the 3 detail pages now gate the sub-section RENDER on resolved
    parent data: `{!parent.isError && …}` → `{parent.data != null && …}`
    (AccountDetailPage, ContractDetailPage, LiquidityPoolDetailPage). Sections
    mount only after the parent query succeeds, so their hooks never fire while
    the parent is still loading — same effect, far smaller surface, zero hook/
    section changes. (Initial enabled-param edits were reverted in favour of
    this.)
  - **Live-verified (Playwright, network panel):** 404 account → parent req 1,
    sub-section reqs **0**; 404 contract → interface/invocations/events **0**;
    404 pool → participants/transactions/charts **0**; **valid** account →
    transactions sub-req fires normally (happy path intact). All three 404s
    render the NotFound state.

**Rationale.** Gate B fix closed the visual side of composite NotFound (sub-section render gated on `!parent.isError`), but Wave 6 confirmed sub-section queries STILL FIRE — producing extra 404 entries in the network panel. **Resolved by render-gating on `parent.data != null`** (sections never mount until the parent resolves OK) rather than the originally-planned per-hook `enabled` arg.

**Scope (as built).** Changed the render gate on the 3 detail pages from `!parent.isError` to `parent.data != null`. No hook or section-component changes needed (the `!isError` gate was true during loading, letting sections mount + fetch before the parent 404'd; `data != null` defers mount until success).

**Findings closed (sub-checklist):**

- [x] F-W6-NOTFOUND-2 — Sub-section queries fire on parent 404, console noise — **0 sub-reqs live**
- [x] F-W6-E6-1 — Sub-section queries still fire on 404 (account) — **0 live**
- [x] F-W6-E9-1 — Same on contract detail — **0 live**
- [x] F-W6-E13- (Network requests) — Same on pool detail — **0 live**

**Notes:** Render-gate approach chosen over per-hook `enabled` for minimal surface. Happy-path (valid parent) sub-section fetch verified still firing.

---

### 5.4 Cross-entity link gaps (Wave 6 remainder)

- **Type:** BUG
- **Effort:** ~30min
- **Severity / Class:** 🟡 B
- **Pre-launch:** SHOULD
- **STATUS:** DONE

**Rationale.** Wave 6 identified a handful of remaining unlinked identifiers not closed by the F-K-2/3 Gate B batch: NFT row contract ID, NFT detail contract ID in Details section, home table ledger hash, possibly E3 tx-detail ledger link. Plus account self-link (cosmetic) and Soroban call tree destination account routing verification.

**Scope.** Wrap remaining identifier renderings in `<RouterLink>` per the canonical `IdentifierDisplay` pattern. Verify `IdentifierDisplay type="ledger"` on E3 emits an `<a href="/ledgers/:seq">`. Confirm `OperationFlowTree` exposes destination account as clickable.

**Findings closed (sub-checklist):**

- [x] F-W6-E10-3 — NFT row Contract ID is plain text → `IdentifierDisplay type="contract"` (NftsTable.tsx:45)
- [x] F-W6-E11-3 — Contract ID in NFT detail Details section → `IdentifierWithCopy type="contract"` (NftSummary.tsx:76)
- [x] F-W6-E1-4 — Ledger hash on home table → `IdentifierDisplay type="ledger"` (home/LedgersTable.tsx:36-42; sequence also linked :24-30)
- [x] F-K-7 — E3 tx-detail ledger sequence link → `type="ledger"` (TransactionSummary.tsx:151-152)
- [x] F-K-8 — Soroban call tree destination account → `IdentifierDisplay` in OperationFlowTree.tsx:151 (account/contract nodes linked)
- [x] F-EX-1 — NFT minted_at_ledger → `IdentifierDisplay type="ledger"` (NftSummary.tsx:96) — **deliberate deviation from Figma**

**Notes:** The five listed findings verified RESOLVED against current code 2026-05-28 (E10-3, E11-3, E1-4 via 0257 merge; K-7, K-8 confirmed). F-EX-1 resolved per user 2026-05-28: the "Minted at ledger" value was plain Satoshi text per the Figma mock; user chose Option B — link it like every other ledger reference for consistency/utility (gold IdentifierDisplay, formatted with thousands separators). Documented Figma deviation; revisit with designer if they object.

**Additional identifier-consistency sweep (lore-0272, 2026-05-28).** Beyond the listed Wave-6 findings, a full repo sweep found 7 ad-hoc entity links still using raw MUI `<Link component={RouterLink}>` instead of the canonical `IdentifierDisplay`. All converted (asset/ledger/nft type, gold-hover): asset-leg labels in PoolsTable / PoolSummary / PoolKpiStrip (tone="inherit"), asset code in AssetsTable, asset balance name in AccountBalances, NFT name in NftNameCell, and the "Since ledger" column in PoolParticipants. Visually verified on assets / pools list / nfts / pool detail / account detail. Legit navigation links (breadcrumbs, View-all, pager, logo, nav bar, external URLs, search-row wrapper) intentionally left as-is.

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

**Notes:** Spawn backend tasks where needed; this card mostly coordinates. **0272 closure note (2026-06-01):** spawned active task **0274** (`active/0274_FEATURE_backend-api-gaps-from-fe-audit.md`) now owns the backend API-gap follow-ups surfaced by the 0272 FE-gaps audit — `/v1/accounts` list endpoint + LP `filter[asset_code]` ILIKE + other field gaps (`docs/audits/2026-05-29-frontend-api-gaps.md`). Some of this card's items (esp. backend field/enum exposure) may fold into 0274 — reconcile when picking this card up.

---

### 6.4 ADR + doc-sync sweep (0254 pagination, cursor namespacing, evergreen sync)

- **Type:** DOCS
- **Effort:** ~2h
- **Severity / Class:** 🟡 D
- **Pre-launch:** NICE
- **STATUS:** TODO
- **design_parity note:** `06ab34cc` added a Mainnet/Testnet UI toggle (`NetworkToggle`) that was **NON-FUNCTIONAL** — now **DELETED** (`e9122732`, card 11.1 RESOLVED). F-AN-6 reverts to plain scope: document the single static `VITE_API_BASE_URL` config; no decorative toggle remains to explain. (`cursor` → `next_cursor` + `prev_cursor`) not propagated to `docs/architecture/backend/backend-overview.md`; 0238 multi-cursor namespacing (`cursor_p/_t/_e/_i`) has no ADR; per-feature wiki gaps (frontend conventions, data flow doc, error message standards as exemplar, useDetailMode vs useTableUrlState pattern, asymmetric folder split rule documented).

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
- [ ] F-AN-6 — Mainnet/Testnet config single-environment (document) — back to plain scope: NetworkToggle was deleted (card 11.1 / `e9122732`), so just document the single-environment `VITE_API_BASE_URL` config; no decorative toggle to explain
- [ ] F-AH-6 — No tests doc note (cross-cite; testing-baseline task owns code)
- [ ] F-AA-4 — `useIntersectionObserver` single-consumer note in wiki
- [ ] Issues Encountered worth re-audit (worktree gotchas → `lore/3-wiki/`)

**Notes:** **\_**

---

### 6.5 0272-session list-page findings (filter / sort / search audit, 2026-06-01)

- **Type:** BUG / FEATURE (mixed FE + backend)
- **Effort:** ~1-2d total across items
- **Severity / Class:** 🟠 B (item 1 user-visible 404) + 🟡 C/D (rest)
- **Pre-launch:** SHOULD (item 1) / NICE (items 2-5)
- **STATUS:** TODO (NOT yet spawned — captured here per user 2026-06-01 "no spawn, ensure it's in the queue")

**Rationale.** 0272's closure session ran a list-page filter/sort/search code audit (no fixes applied) and surfaced concrete dispositions. NOT spawned as backlog tasks yet (user deferred spawning). Captured here as the authoritative record so nothing is lost. Items 1-2 partly owned by spawned active tasks 0274/0275; items 3-6 fully unspawned. Source: `archive/0272_FEATURE_audit-0257-closing.md` §Session findings + §Future Work + top-of-file 0272 merge-note block.

**ALL ITEMS RE-VERIFIED VALID + CURRENT — 2026-06-01 (this session, 4-explorer fan-out + direct backend SQL + live `:4201` reads).** Verdicts below carry confirmed file:line refs. One earlier doc claim corrected: the LP `asset_code` type-stub comment said "partial" — **actual SQL is exact equality** (see F-0272S-2).

**Findings closed (sub-checklist):**

- [ ] **F-0272S-1 — Accounts list = MOCK data (account-not-found root cause). VALID+CURRENT.** `web/src/api/hooks/useAccountsList.ts:32` fabricates 80 synthetic G-strkey accounts (in-code comment: "Local fixtures… `GET /v1/accounts` endpoint not yet implemented"). No backend call. Row click → `/accounts/G…` → detail page calls real `GET /v1/accounts/{id}` → **404 "Account not found."** Link build + encoding correct; pure mock-vs-real mismatch (NOT a DB-seed gap). Global topbar search → account works (real `/v1/search`), so detail page itself OK. 🟠 user-visible. **Owned by 0274** (real endpoint) — cross-ref card 1.3.
- [ ] **F-0272S-2 — LP vs assets search inconsistency (real bug). VALID+CURRENT.** Assets `filter[code]` = partial substring `a.asset_code ILIKE '%'||$1||'%'` (`assets/queries.rs:132`); LP `filter[asset_code]` = **EXACT** `UPPER(lp.asset_a_code)=$9 OR UPPER(lp.asset_b_code)=$9` (`liquidity_pools/queries.rs:340`). That is why LP needs the whole code, assets does not. **Correction:** prior type-stub comment said LP was "partial" — wrong, SQL is equality. Note LP searches by **asset code**, not pool id. Fix → LP `ILIKE '%'||..||'%'` on both legs (1 line + regen). **Owned by 0274** (backend) — cross-ref card 6.3.
- [ ] **F-0272S-3 — Dead sort UIs (assets total-supply + ledgers sequence). VALID+CURRENT.** Assets: FE sends `order` (`useAssetsList.ts:22`) but `/v1/assets` has no `order` param → ignored; arrow = client state only, never hits DB. Ledgers: FE builds then type-casts `order` away (`useLedgersList.ts:24`); API takes only `limit`+`cursor`, backend hardcodes `ORDER BY closed_at DESC, sequence DESC`. **User's cursor intuition is correct** — cursor pagination can't client-sort one page; sort must be a backend param baked into the cursor key, which neither backend supports today. Two honest options: (1) **remove the arrows now** (FE-small, stop lying to user) — recommended near-term; (2) add backend sort param + cursor that encodes sort key (backend-medium, backlog).
- [ ] **F-0272S-4 — Silent no-op searches (transactions + NFT). VALID+CURRENT.** Transactions (`TransactionsListPage.tsx:40`): only fires a filter when input is a full G-account (`→ filter[source_account]`) or full C-contract (`→ filter[contract_id]`); tx-hash / partial / anything-else → **no filter sent, silent no-op** (no tx-hash search, no partial). NFT (`NftsListPage.tsx`): collection = exact equality on `collection_name`; contract requires valid C-strkey; partial → empty. By design but reads as broken. UX fix = placeholder + empty-state hints. **Unspawned** — FE-small.
- [ ] **F-0272S-5 — Transaction type dropdown single-select. VALID+CURRENT.** Single `<Select>`, one type at a time (`TransactionFilters.tsx:69`); backend `filter[operation_type]` accepts one string only. Multi-select would need backend to accept CSV/array (`IN (...)`) + FE multi-select UI. **Unspawned** — medium (FE + backend).
- [ ] **F-0272S-6 — No shared filter/search semantics across list pages (architectural). NEW 2026-06-01.** Each list page rolls its own match semantics — exact vs partial vs strkey-gated — with no shared abstraction; the topbar global search (`/v1/search`, broad multi-entity, redirects on single hit) is the **only** broad one. This divergence is the root of every per-page inconsistency above (and the accounts "filter" confusion). Covers user concerns "compare list-page filter logic to topbar + search endpoint" + "check the search filter on every page". **Disposition:** define a consistent search-semantics contract (recommend partial-ILIKE baseline for code/name fields; strkey-exact only for ID fields; document which pages intentionally differ). FE-medium + backend-coordination. **Unspawned.** Per-page audit table:

  | Page | Field | Match | Reaches API? | Verdict |
  | --- | --- | --- | --- | --- |
  | Transactions | account/contract strkey | exact, prefix-gated | yes (only valid strkey) | works; no partial/hash |
  | NFT | collection / contract | collection=exact, contract=strkey | yes | feels dead on partial |
  | Assets | code | partial (ILIKE) | yes | good baseline |
  | Liquidity Pools | asset_code | exact | yes | inconsistent w/ assets (F-0272S-2) |
  | Accounts | account_id | substring in-memory | **no — mock** | fake (F-0272S-1) |
  | Global (topbar) | `/v1/search` | broad multi-entity | yes | real, redirects on single hit |

- [ ] **Accounts "New accounts" control = a SORT, not a filter (decode).** Part of F-0272S-1's fake-data caveat. `AccountsListPage.tsx:30` dropdown, 3 options: "Top XLM holders" = `xlm_desc`; "Recently active" = `last_seen_desc`; "New accounts" = `first_seen_desc` (newest-created first). Operates in-memory on the 80 synthetic accounts → **means nothing real until `/v1/accounts` exists** (0274).

**Notes:** When the next 272-like closure container is spawned, fold items 3-6 into its scope (or spawn discrete backlog tasks from develop). Items 1-2 track via 0274/0275 — verify they actually cover F-0272S-1/F-0272S-2 when those tasks are picked up. **Suggested action split:** quick FE-only = remove dead sort arrows (F-0272S-3) + add search placeholder/empty-state hints (F-0272S-4); backend-small = LP `asset_code` ILIKE (F-0272S-2); backend-medium/backlog = `/v1/accounts` list endpoint (F-0272S-1, kills not-found bug + makes accounts sort real), assets/ledgers sort params w/ cursor (F-0272S-3 option 2), op-type multi-select (F-0272S-5); architectural = shared search-semantics contract (F-0272S-6).

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
- [~] F-W6-E11-2 — NFT Traits "Metadata unavailable" no actionable guidance — **PARTIAL `06ab34cc`: NFT _media_ empty-state improved (icon chip + "No media available" + subtext); Traits empty-state guidance NOT improved**
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
- **STATUS:** DONE — FE scope complete (2026-06-01); backend-dependent sub-items DEFERRED (see below)
  - **FE done:** `useLiveStatus()` hook (`web/src/api/hooks/`) is the single source
    of truth — derives `live / delayed / offline / unknown` from
    `NetworkStats.latest_ledger_closed_at` + query error/loading. Returns `unknown`
    (CONNECTING pill) while loading instead of a false `live`. `LiveIndicator`
    consumes it on all 3 pill sites (ChainOverview, LatestTransactions, LatestLedgers);
    dot colour bound to `theme.palette.stroke[...]` (type-safe, no magic string).
  - **DM-1 (footer "All systems operational") — RESOLVED AS ABSENT:** the hardcoded
    footer status string no longer exists anywhere in the codebase (grep = 0). Per
    user (2026-06-01) the footer carries **no** live/health badge at all — nothing to
    fix or remove. F-W6-V-1 fully closed.
  - **DEFERRED — backend-dependent (not FE pre-launch):** DM-2 (`/v1/health` probe),
    F-W6-V-2 (backfill-on-historical should disable LIVE — needs `is_live` field),
    F-W6-AP-2 (poll-refresh pulse, NICE). These need backend endpoints/fields that
    don't exist yet → out of FE scope. Spawn as backend-coord follow-ups if pursued.
- **progress note (2026-05-28, `2c31a87a`):** `LiveIndicator` (used by ChainOverview + the Latest transactions / Latest ledgers section headers) now derives **LIVE / DELAYED / OFFLINE** from `NetworkStats.latest_ledger_closed_at` freshness instead of a static green dot, and stays visible in every state. Closes the freshness gap for those pill sites. **Diverged from planned scope (emerged):** logic lives inline in `web/src/pages/home/LiveIndicator.tsx`, NOT a shared `useLiveStatus()` hook in `libs/ui/timestamps/`; threshold is a single `LIVE_MAX_AGE_MS = 20_000` (LIVE→DELAYED) rather than the scoped 30s/5min tiers; OFFLINE is driven by the stats query `isError` (single failure), not a 5-min age cutoff — flicker risk noted, `failureCount` variant was tried then reverted per user. **Still TODO:** footer "All systems operational" (DM-1) still hardcoded; no `/health` probe (DM-2); `PollingIndicator` still 0 consumers on detail pages (F-D-4); backfill-disables-LIVE (F-W6-V-2); poll-refresh pulse (F-W6-AP-2); consolidation into one shared source of truth.

**Rationale.** All 5 "live" indicator sites + footer "All systems operational" are hardcoded — no logic compares latest ledger close time to now or probes API health. Data on display can be hours stale while badge shows green. Backfill activity is not surfaced to FE. Universal pre-launch credibility hit.

**Scope.** Add `useLiveStatus()` hook in `libs/ui/src/timestamps/`: compares `latest_close_at` with `now()`; threshold <30s = LIVE, >30s = STALE, >5min = OFFLINE. Single source of truth for footer + all 5 LIVE pill sites. Add `/v1/health` backend endpoint check for footer status indicator. Wire `is_live` / `latest_close_at` from `/v1/network/stats` into the hook. Add subtle pulse / row-flash on poll refresh (paired with card 2.3).

**Findings closed (sub-checklist):**

- [x] DM-1 — Footer "All systems operational" — RESOLVED AS ABSENT (string no longer in codebase; per user, footer carries no health badge)
- [ ] DM-2 — No `/health` or `/status` endpoint hit anywhere — **DEFERRED (backend dependency)**
- [~] F-D-4 — Polling indicator on detail pages — PollingIndicator now has 1 consumer (LatestTransactions); detail-page coverage still open (NICE)
- [x] F-W6-V-1 — ALL live pills now have freshness logic via `useLiveStatus` (live/delayed/offline/unknown); footer DM-1 resolved-as-absent
- [ ] F-W6-V-2 — Backfill-on-historical doesn't disable LIVE — **DEFERRED (needs backend `is_live` field)**
- [x] F-W6-V-3 — Latest-ledger polling works (informational)
- [ ] F-W6-AP-2 — Polling refresh silent (no visual indicator) — **DEFERRED (NICE, post-launch)**
- [x] F-W6-E1-1 — LIVE badge freshness on Latest tx/Ledgers — DELAYED/OFFLINE/CONNECTING when stale/errored/loading

**Notes:** Requires backend `/v1/health` endpoint and `is_live` / `latest_close_at` field on `/v1/network/stats`.

---

### 7.3 Pool participants share % precision fix

- **Type:** BUG
- **Effort:** ~15min
- **Severity / Class:** 🟠 A
- **Pre-launch:** SHOULD
- **STATUS:** DONE (2026-06-01)
  - `PoolParticipants.tsx:55` now renders `formatPercent(Number(row.share_percentage))`
    = `Number.toFixed(2) + '%'` (true 2-decimal cap, not minDecimals padding).
    `33.3333…` → `33.33%`. The illusory `formatAmount(_, 2)` is gone; this is a
    real rounding fix. (The `formatPercent` migration landed with the 2.1/post-merge
    consolidation; verified live this session.)
- **design_parity R2 note (2026-05-29, PR #224, `fce0d666` / merge `35ac27c0`) — ILLUSORY FIX (now superseded — see STATUS DONE above):** R2 changed `PoolParticipants.tsx:58` to `formatAmount(row.share_percentage, 2)`, BUT `formatAmount(value, minDecimals)` (`web/src/pages/format.ts:12`) treats the 2nd arg as **minimum-decimal PADDING, NOT rounding** — it trims trailing zeros and pads UP to `minDecimals` but never caps precision. A raw `33.3333…` still renders full precision. The bug is **NOT actually fixed** unless the API pre-rounds `share_percentage` to 2dp. Card stays **TODO** (NOT done). **Needs live confirm** at `/liquidity-pools/:id` participants table before any DONE; if precision >2dp persists, switch to `.toFixed(2)` / a true max-decimals formatter. Source: `design-parity-impact-2026-05-29.md` §4 (F-W6-E13-1), §6.

**Rationale.** Pool participants "Share %" column renders at full precision (`33.3333333333333333%`). Two decimals (`33.33%`) is the universal convention. UX-degrading on every fractional share.

**Scope.** Find the share % render in `web/src/pages/pool-detail/PoolParticipants.tsx`. Apply `formatPercent(value, 2)` from card 2.1 batch (or inline `.toFixed(2)` until 2.1 lands).

**Findings closed (sub-checklist):**

- [x] F-W6-E13-1 — Pool participants Share % rendered at full precision — **FIXED 2026-06-01: `formatPercent(Number(share_percentage))` caps at 2dp (`33.33%`). The illusory `formatAmount(_, 2)` minDecimals path is replaced.**

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

### 7.9 Clickable table rows → detail view (UX + touch-target)

- **Type:** FEATURE
- **Effort:** ~2-3h
- **Severity / Class:** 🟡 C
- **Pre-launch:** NICE (deferred — revisit later, user 2026-06-01)
- **STATUS:** TODO

**Rationale.** Standard explorer affordance (Etherscan / Solscan): the whole
table row is clickable → navigates to the row's detail view, not just the
identifier link in the first column. Bigger hit area also helps mobile
touch-targets (cross-cites C11.6 / F-W6-RESPONSIVE-4). Today only the inline
`IdentifierDisplay` link (col 1) + other inline links / CopyButton are
clickable; the rest of the row is dead space.

**Scope (scoped 2026-06-01).** Add an optional `onRowClick?: (row) => void` to
the shared `ExplorerTable` (`libs/ui/src/table/ExplorerTable.tsx`) — keep it
**router-agnostic** (callback; the consuming page calls `navigate`, libs/ui must
not import react-router). On the `<TableRow>`: `cursor: pointer` + hover bg + the
handler. **Nested-click priority:** guard with `e.target.closest('a, button')`
so clicking an inner link / CopyButton runs its own action and does NOT trigger
row-nav (CopyButton already `stopPropagation`s; the closest-guard also covers the
`<a>` identifier links). Keyboard path stays the col-1 `<a>` link (row-click is a
mouse/touch convenience). Wire `onRowClick` on the ~7 list tables with a clear
detail target (Transactions, Accounts, Assets, Ledgers, Pools, NFTs,
Contracts-when-real); embedded section tables (AccountTransactions,
PoolTransactions, …) target the row's primary entity (col-1 type+value).

**Findings closed (sub-checklist):**

- [ ] (new) Whole-row click → detail across list tables
- [ ] cross-cite C11.6 — larger row hit area aids mobile touch targets

**Notes:** Investigated 2026-06-01 — ExplorerTable shared across 13 tables; rows
already carry a col-1 detail link via `IdentifierDisplay`
(`getIdentifierHref(type,value)`), so this is a convenience layer, not new
navigation. `CopyButton` already `stopPropagation`s. Deferred by user to revisit
later; spec captured so it can be picked up cold.

---

### 7.10 Route Suspense fallback shape mismatch — 3-phase load flicker

- **Type:** BUG (UX / perceived performance)
- **Effort:** ~2-4h (add per-route skeleton fallbacks)
- **Severity / Class:** 🟡 C (visual)
- **Pre-launch:** SHOULD (worst on home = first impression)
- **STATUS:** TODO — **NEW finding 2026-06-01 (user-reported + code-root-caused this session)**

**Rationale.** User observed the homepage flashes **three** distinct visual phases on refresh: (1) empty/generic skeletons that don't match the real layout, (2) skeletons of the actual components, (3) loaded data. Phases 1↔2 look jarringly inconsistent. **Root cause confirmed (code):** all routes are `React.lazy` + wrapped in a single shared `<Suspense fallback={<DetailSkeleton />}>` (`web/src/router/index.tsx:11-18`). `DetailSkeleton` (`libs/ui/src/states/skeletons/DetailSkeleton.tsx:9-21`) = title stub + 3 stacked `CardSkeleton`s — shown while the lazy JS chunk downloads (**phase 1**). Once `HomePage` mounts, its per-component skeletons render — `ChainOverview` KpiCell `<Skeleton>` + `LatestTransactions`/`LatestLedgers` `<TableSkeleton>` — which DO match the final layout (**phase 2**), then data resolves (**phase 3**). So phase-1 fallback (generic 3-card) is structurally unrelated to phase-2 (hero + 4 KPIs + 2 tables) → the jump.

**Universal, severity varies.** Same lazy+`DetailSkeleton` mechanism on every route. Detail pages: phase-1 ≈ phase-2 (both "title + cards") → mild. List pages (`TransactionsListPage`/`LedgersListPage`): 3-cards vs header+table → moderate. **Home: worst** — real layout (hero + stat row + 2 tables) bears no resemblance to `DetailSkeleton` → most jarring, and it's the first-impression route.

**Scope.** Replace the one-size `DetailSkeleton` Suspense fallback with route-appropriate fallbacks: a `HomeSkeleton` (hero + 4-up stat row + 2 table cards) for `/`, a list-shaped fallback for list routes, keep `DetailSkeleton` for detail routes. Or render the page shell synchronously and only Suspense the data region. Goal: phase-1 ≈ phase-2 so the lazy-chunk boundary is invisible.

**Findings closed (sub-checklist):**

- [ ] **F-W6-LOADSKEL-1** — Home route Suspense fallback (`DetailSkeleton`) does not match home layout → 3-phase flicker (`router/index.tsx:14`). Add `HomeSkeleton`.
- [ ] **F-W6-LOADSKEL-2** — List routes: generic `DetailSkeleton` fallback ≠ header+table content skeleton (moderate). Add list-shaped fallback.
- [ ] **F-W6-LOADSKEL-3** — Detail routes: mild mismatch (acceptable) — verify `DetailSkeleton` close enough, or per-entity fallback if cheap.

**Notes:** Distinct from card **2.3** (EmptyState/LoadingState primitive consolidation, SKIP — about primitive existence, not fallback shape) and card **7.7 / F-W6-AG-5** (route-*transition* progress indicator for nav between routes, TODO — different mechanism, doesn't fix the fallback-shape mismatch). Confirmed NOT a duplicate. Live-observable on slow chunk load; localhost too fast to reliably screenshot the sub-second phase-1, but code path is definitive + user observes it directly.

---

## Category 8 — Catalog / lore / docs

### 8.1 Test coverage baseline (libs/ui vitest + critical components)

- **Type:** FEATURE
- **Effort:** ~1w
- **Severity / Class:** 🟠 D (pre-launch maintenance risk)
- **Pre-launch:** SHOULD
- **STATUS:** PARTIAL

- **0226 landed 2026-05-29 note:** Task 0226 SHIPPED via PR #225 (ab170804) — Vitest + Testing Library infra + 132 tests across 17 files (libs/ui: PaginationControls, usePageHandlers, useTableUrlState; web: AccountDetailPage, AccountsListPage, AssetDetailPage, AssetsListPage, TransactionsListPage + format/formatters/operationTypes/assetType/interfaceMetadata/pool-detail-helpers/directRouteFor). Vitest infra + unit/component baseline = DONE. **Residual (keeps card PARTIAL):** Playwright CLI smoke for 11 paginated pages, CI test gate wiring, `useDebouncedDraft` tests (hook is C2.1 refactor output — not yet created), explicit `truncateMiddle`/`useCursorPagination` unit tests. The 0226 test files also now PROTECT the C2.1 format-truncate refactor (format/formatters/operationTypes tests guard the unification).

**Rationale.** Zero `*.test.*` / `*.spec.*` files across `web/src/` + `libs/ui/src/`. Single biggest pre-launch maintenance risk per F-AD-5. Documented as 0257 dropped scope `O`. Spawn the testing-baseline task with the inheritance chain `related_tasks: ['0238', '0254', '0257']` (per Q-7 forward-link note). **[0226 shipped — see landed note above; zero-coverage premise no longer holds.]**

**Scope.** ~~Spawn / promote task 0226 (libs/ui vitest infra).~~ **DONE (PR #225).** Residual: explicit `truncateMiddle`, `useCursorPagination`, `useDebouncedDraft` (post-C2.1) unit tests. Add Playwright CLI smoke for 11 paginated pages (blocks 0077, 0238). Wire CI gate.

**Findings closed (sub-checklist):**

- [~] F-AD-5 — Zero test coverage (cross-cite) — **PARTIAL: 132 tests shipped 0226 PR #225; zero-coverage premise gone**
- [ ] F-AH-6 — No tests collocated or in `__tests__/` — **0226 collocated `*.test.*` next to source ✓ (verify convention)**
- [x] A4 — Task 0226 backlog since 2026-05-15 unblocks 4 deferred items — **RESOLVED: 0226 shipped PR #225**
- [x] 0226 promote — blocks 0073/0074/0077/0238 Playwright CLI runs + unit tests — **DONE: 0226 landed; Playwright CLI smoke still pending (separate)**
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
- [x] F-W6-RESPONSIVE-3 — No hamburger / mobile nav — **DONE via C11.5 (slim right Drawer, live-verified 2026-06-01)**
- [→] F-W6-RESPONSIVE-4 — Touch targets <44px on mobile — **SPLIT → C11.6. Still failing live 2026-05-28: 105/106 interactive elements <44px @375 (pagination 36px, nav 24–32px)**
- [x] F-W6-E0-3 — No hamburger menu at mobile (recap) — **DONE via C11.5**
- [x] 0059 Future Work — Responsive nav (collapsible / hamburger on mobile) — **DONE via C11.5**

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
- **design_parity note:** ARTIFACT CHANGED. `06ab34cc` **rewrote the sort caret** — removed MUI `TableSortLabel` + `UnfoldMore`; new `SortableHeader` with a circular badge + rotating `KeyboardArrowDownIcon`. The "middle-ground" caret the audit flagged for sign-off is now a _different_ implementation. Designer sign-off (F-AB-4 / 0061 #4) now applies to the **new circular-badge caret**, not the old one. Status stays TODO; only the artifact under review changed.

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

> New cluster. Source: `design-parity-impact-2026-05-27.md` §Regressions. The `feat/design_parity` merge (`06ab34cc` / merge `62c988d4`) introduced 4 net-new debt items while doing its Figma-parity + responsive pass. Each is tracked as a new `F-DP-*` finding (see appendix) and clustered here. Two of these directly **regress** existing audit findings the visual-polish card 7.1 was meant to _close_ (F-AK-1 hex, F-AK-2 z-index).

### 11.1 NetworkToggle non-functional affordance

- **Type:** BUG
- **Effort:** ~4h (wire) / ~30min (hide)
- **Severity / Class:** 🟠 C
- **Pre-launch:** SHOULD
- **STATUS:** DONE

**Rationale.** `06ab34cc` added `libs/ui/src/layout/NetworkToggle.tsx` (124 lines): a Mainnet/Testnet segmented control with `role="group"`, `aria-pressed`, hover, per-network palette — wired AppShell → TopNav → NetworkToggle via a local `useState<Network>`. **It is purely visual.** `web/src/api/config.ts` `apiBaseUrl` is a static module constant from `VITE_API_BASE_URL` and does NOT read `network`; query keys (`queryKeys.ts`) do not include network; there is no network context/provider. Switching the toggle changes only the toggle's own rendering — no API base URL change, no refetch, no data difference. It is also **invisible on `/`** (TopNav is now hidden on the home route: `{!isHome && <TopNav .../>}`). This is a misleading affordance — worse for users than F-AN-6's prior no-toggle baseline.

**Scope.** DECISION NEEDED — present both options:

- **Option A (wire it).** Thread `network` into `apiBaseUrl` resolution + namespace query keys by network + add a network context/provider so switching actually changes data. Larger lift; only valid if backend serves both networks. Also surface on `/` (or accept home-route absence by design).
- **Option B (hide it).** Remove / feature-flag the toggle until multi-network is real. Restores the honest single-environment baseline; pairs with card 6.4 documenting single-env config.

**Findings closed (sub-checklist):**

- [x] F-DP-1 — NetworkToggle non-functional (wire OR hide) — **resolved by removal** (`e9122732`)
- [x] F-AN-6 (cross-cite) — no decorative toggle left to document; single-environment config doc reverts to its plain scope (card 6.4)

**Notes:** RESOLVED 2026-05-28 per user — Option B to the limit: the `NetworkToggle` component + its AppShell/TopNav wiring were **deleted entirely** (commit `e9122732`), not just hidden. Single-network explorer, no switcher in scope. **Survived the lore-0272 ↔ research/0257 merge (2026-05-30): verified zero `NetworkToggle` refs in `web/src` + `libs/ui/src`.** The "no-op confirm" verification item is now moot. — _Superseded design_parity R2 note (true on the research/0257 branch BEFORE this merge brought the deletion):_ R2 (PR #224, merge `35ac27c0`) found the toggle STILL FAKE / VERIFIED-FAKE-live 2026-05-29 (clicking Testnet flipped `aria-pressed` only, no URL/refetch). Historical now — the merge removed the toggle, so it is neither fake nor present. DONE stands. Source: `design-parity-impact-2026-05-29.md` §1 (11.1), §2 (F-DP-1).

---

### 11.2 AssetIcon hardcoded hex → theme tokens (regresses F-AK-1)

- **Type:** REFACTOR
- **Effort:** ~30min
- **Severity / Class:** 🟠 C
- **Pre-launch:** NICE
- **STATUS:** DONE (`0139a8a3`, 2026-05-28)

**Rationale.** `06ab34cc` added inline hardcoded hex `'#724311'` + `'#fffcc2'` in AssetIcon (`sac` kind), bringing the hardcoded-hex count from 3 → 5 (`ContractInterface` `TYPE_REF_COLOR='#155dfc'` retained). Directly **regresses F-AK-1 / F-W6-AK-1**, which card 7.1 was meant to close.

**Scope.** Move the 2 new AssetIcon hex values to theme tokens (e.g. a `palette.assetKind.sac` token pair) alongside the card 7.1 hex consolidation. Fold into card 7.1's hex sweep OR land standalone here.

**Findings closed (sub-checklist):**

- [x] F-DP-2 — AssetIcon `#724311` / `#fffcc2` → `colorsLight.primary[900]/[100]` (`0139a8a3`)
- [x] F-AK-1 / F-W6-AK-1 (cross-cite) — regression undone: same commit also moved `ContractInterface` `#155dfc` → `blue[600]` + Chip variant / Switch-thumb hex → `scales.*`

**Notes:** RESOLVED in `0139a8a3` (DS-token sweep, lore-0272): AssetIcon `#724311`/`#fffcc2` → `colorsLight.primary[900]/[100]`. **Survived the merge (2026-05-30): verified zero hardcoded hex in `AssetIcon.tsx`.** — _Superseded design_parity R2 note (true on research/0257 BEFORE this merge brought the revert):_ R2 reported `AssetIcon.tsx` still inlines `#724311`/`#fffcc2` (bound raw, not via `theme.palette`) — regression persisting; R2's `assetColor.ts` touch was a red herring (already token-bound; not the regression site). Historical now — the merge brought our token-binding, so the regression is gone. DONE stands. Source: `design-parity-impact-2026-05-29.md` §1 (11.2), §2 (F-DP-2).

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
- **STATUS:** DONE (2026-06-01, live-verified)
  - Built in **`SecondaryNav`** (not TopNav — that's where the nav links live;
    TopNav holds only search + stats). Below `md` (900px) the inline links hide
    (`display:{ xs:'none', md:'flex' }`) and a hamburger `IconButton` (44×44,
    `display:{ xs:'inline-flex', md:'none' }`) toggles a **slim right Drawer**
    (`min(72vw,256px)`, `surface.grayMain`, `stroke.default` left border) holding
    the same NavButton links (size lg). a11y: `aria-label` Open/Close,
    `aria-expanded`, `<nav aria-label="Primary">`, MUI focus-trap, Escape +
    backdrop close, link-click closes drawer. The same hamburger flips ☰→✕ while
    open (no separate close button — slim).
  - **Breakpoint chosen `md`/900** (not the card's loose "768") — repo had no
    `sm`-nav split; the inline nav already hid `<md` via the prior scroll-nav,
    so `md` is the clean, consistent threshold.
  - **Design iteration:** first a full Drawer w/ close button, then a
    Collapse-dropdown (user: "wysuwane z góry słabe"), settled on the slim right
    Drawer per user choice 2026-06-01.
  - **Live-verified (Playwright):** @390 hamburger 44×44 visible, inline hidden,
    drawer 256px w/ 7 links, Escape closes; @1280 hamburger hidden, inline nav
    back. ui 45 + web 86 green.

**Rationale.** design_parity removed the 802px doc-scroll root cause but left no hamburger menu at narrow viewports. At 375px the 8 nav links happen to fit in 364px without scrolling, but that's fragile — any nav label change or i18n overflows. User decision 2026-05-28: require a proper hamburger menu, not the scroll-nav fallback.

**Scope (as built).** Hamburger + slim right Drawer added to **`SecondaryNav`** (`libs/ui/src/layout/SecondaryNav.tsx`) — collapses nav links into a drawer below the `md` breakpoint. Desktop unchanged.

**Findings closed (sub-checklist):**

- [x] F-W6-RESPONSIVE-3 — hamburger nav <md (slim right Drawer, live-verified 2026-06-01)

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

## Full re-run 2026-06-01 — post-merge re-audit (Waves 1-4 code-level + deterministic baseline)

Full audit re-run against merged HEAD `e3fe1968` (0272 fixes + 0243 ClickHouse + 0273 deploy). 5-agent code fan-out (API/type-safety, routes group 1, routes group 2, cross-cutting quality, Wave-4 state matrix) + deterministic baseline + targeted live (`:4201`). Live visual/responsive Waves 5-6 NOT fully re-run here (see note at end). New IDs = **F-RR-***.

### Deterministic baseline

- **Typecheck: GREEN** once deps built (`nx build @rumblefish/soroban-block-explorer-ui` → `nx typecheck web` passes). ⚠️ Standalone `nx typecheck web` with a **stale `libs/ui` dist** throws ~dozens of false `Property 'surface'/'stroke'/'tertiary' does not exist` + `bodyXsRegular` errors — the MUI theme augmentation (`libs/ui/src/theme/types.ts`, `declare module '@mui/material/styles'`) ships via libs/ui's **emitted `.d.ts`**, so web typecheck is fragile to dist freshness. CI builds deps first → green. **Benign build-order artifact, NOT a code regression** (libs/ui typechecks clean alone). Minor hygiene note → fits card 8.8 / nx build-graph docs (no card needed).
- **Tests: 60/86 pass locally; 26 fail** with `TypeError: Cannot read properties of null (reading 'useEffect')` in `QueryClientProvider` across the 5 provider-wrapped suites (AccountDetail/AccountsList/AssetDetail/AssetsList/TransactionsList). Textbook **dual-React resolution** artifact — almost certainly triggered by building libs/ui dist (vitest normally aliases libs/ui→src; fresh dist shadows it). 0226 archive records "132 tests green on develop." **Classified local-env artifact, NEEDS-CI-CONFIRM — NOT a merged-code regression.** If CI ever shows the same → real dedupe-React/vitest-alias fix needed (would be 🟠).

### 0272 consolidation — VERIFIED clean (grep-confirmed, not file-existence)

NetworkToggle (0 source refs, dist rebuilt clean), formatter consolidation (`web/src/pages/format.ts` + `formatFee.ts` deleted, single `libs/ui/src/format/`), truncate (canonical `truncateMiddle`), debounce (`useDebouncedDraft`, no setTimeout reimpls), hex→token (AssetIcon/ContractInterface hex gone) — all **RESOLVED confirmed**. State handling: `QueryErrorState`/`DetailErrorState` landed + consumed uniformly across all 6 list pages + 7 detail pages; composite-NotFound sub-section gates present (`AccountDetailPage.tsx:91`, `ContractDetailPage.tsx:116`, `LiquidityPoolDetailPage.tsx:89`). Cards 2.1/2.4/5.3/11.1/11.2 reconfirmed DONE; SM-1/SM-2 landed.

### NEW findings (F-RR-*)

| ID | Sev | File:line | Finding | Disposition |
| --- | --- | --- | --- | --- |
| F-RR-1 | 🟠 | `useLedgersList.ts:24`, `useAccountTransactions.ts:24`, `useAssetsList.ts:26` | `order` query param injected via cast past generated query type (codegen has no `order`) — type-safety face of F-0272S-3; either backend ignores it (dead sort) or honours undocumented param | backend OpenAPI add `order`/`sort`; FE regen drops casts. Pairs w/ F-0272S-3 |
| F-RR-2 | 🟡 | `home/HeroSearch.tsx:100` | "CTRL + K" hint pill — no global Cmd/Ctrl+K handler exists anywhere; advertises dead shortcut | wire hotkey or drop pill |
| F-RR-3 | 🟡 | `home/LatestTransactions.tsx:53-68`, `LatestLedgers.tsx:51-66` | Footer "{rows.length} latest records" renders in loading+error (shows "0 latest records" under skeleton/error) | gate footer on success branch (= F-W4R-4) |
| F-RR-4 | 🟢 | `LatestTransactions.tsx:48` vs `LatestLedgers.tsx:46` | Casing "Latest transactions" vs "Latest Ledgers" | pick one |
| F-RR-5 | 🟢 | `transactions/TransactionsTable.tsx:38` vs `home/LatestTransactionsTable.tsx:28` | Same source-account field: list uses `IdentifierDisplay` (no copy), home uses `IdentifierWithCopy` | unify |
| F-RR-6 | 🟡 | `transaction-detail/sections/OperationPicker.tsx:89` | Heading hardcoded "Choose payment" but lists ALL op types (Figma copy bleed) | "Choose operation" |
| F-RR-7 | 🟡 | `transaction-detail/advanced/EventsSection.tsx:80-90` | `event.contract_id` rendered as plain `truncateMiddle` text, not a contract link — missed by the 2026-05-28 7-site identifier sweep | wrap in `IdentifierDisplay` |
| F-RR-8 | 🟢 | `normal/humanizeOp.ts:43` | PAYMENT fallback summary omits amount (it's in `heavy.details`) | include amount |
| F-RR-9 | 🟢 | `AccountDetailPage.tsx:60` | Breadcrumb 'Account' crumb has no `to` (not a link back) — likely deliberate while accounts mock | revisit w/ real `/accounts` |
| F-RR-10 | 🟢 | `accounts/AccountSummary.tsx:41`; `AccountsTable.tsx:73,86` | `sequence_number` rendered with `formatAmount` (thousands separators "123,456,789") — Stellar seq usually raw | verify vs Figma |
| F-RR-11 | 🟡 | `accounts/AccountsTable.tsx:73,86` | "Last/First Seen" ledger = plain `formatAmount` number, not a ledger link nor time (detail page links these) — decision carries into real page | link + format on real page |
| F-RR-12 | 🟢 | `useAccountsList.ts:126` | `listPolicy` polling applied to `Promise.resolve(mock)` — pointless re-resolve of static data | remove while mock |
| F-RR-13 | 🟡 | `LedgerDetailPage.tsx:80-102` | Breadcrumb hand-rolled (inline Link + "/") instead of shared `PageBreadcrumb` (TxDetail/AccountDetail use shared) | reuse `PageBreadcrumb` |
| F-RR-14 | 🟡 | `ledgers/LedgerSummary.tsx:23-68` | Summary key/value reimplemented via local `Cell`/`Row` instead of shared `SummaryRow`/`SectionCard`; also no semantic `<h2>` | reuse primitives |
| F-RR-15 | 🟢 | `ledgers/LedgerTransactions.tsx:41` | Reuses `TransactionsTable` incl. redundant "Ledger" column on a single-ledger page | hide column |
| F-RR-16 | 🟡 | `nft-detail/NftEventBadge.tsx:13-22` | `EVENT_STYLES`/`FALLBACK_STYLE` use `colorsDark.*` literals unconditionally. App defaults `mode='dark'` (`ThemeProvider.tsx:54`) → **correct in default view, latent**; wrong only if light mode reachable. **Inverse concern:** card 11.2 "fix" made `AssetIcon` hardcode `colorsLight.*` — suspect in the dark DEFAULT (needs live dark-mode visual check). Both = theme-coupling via hardcoded palette objects instead of theme-aware `Chip`/`sx` callback | reuse `Chip`/theme palette; live-check both in dark default + verify light-mode reachability |
| F-RR-17 | 🟠 | `pool-detail/PoolCharts.tsx:126,155,200-251` | `isError` destructured but never surfaced → fetch error renders as "No activity / try longer range" empty state, no retry; every sibling section shows `QueryErrorState` | add error+retry branch |
| F-RR-18 | 🟡 | `liquidity-pools/FeePill.tsx:24` | `formatPercent(Number(raw), raw)` — 2nd arg is the non-finite fallback string; NaN fee renders raw `0.300000…` instead of em-dash | drop 2nd arg / pass `'—'` |
| F-RR-19 | 🟡 | `pool-detail/PoolSummary.tsx:99` | Pool fee shown 2 ways: summary `formatAmount(fee,2)%` vs FeePill `formatPercent` (`.toFixed(2)`) — `0.305` → `0.305%` vs `0.30%` same page | reuse FeePill/formatPercent |
| F-RR-20 | 🟡 | `pool-detail/helpers.ts:21,41`; `liquidity-pools/assetColor.ts:122,128` | Native-leg detected via `asset_type === 0` (link) vs `asset_type_name === 'native'` (label/color) — diverge under schema drift | single field for native check |
| F-RR-21 | 🟡 | `search/GlobalSearchBar.tsx:81`; `search/SearchResultsTabs.tsx:20,40` | Search a11y incomplete: `role=listbox` rows lack `role=option`/`id`/`aria-activedescendant`; `role=tablist` has no `tabpanel`/`aria-controls` | complete ARIA patterns |
| F-RR-22 | 🟢 | `search/useSearchResults.ts:31-38` | Tab labels mix singular/plural (Transactions/Accounts plural; Contract/Token/NFT/Liquidity Pool singular) — all are count tabs | pluralise all |
| F-RR-23 | 🟢 | `search/SearchResultsTabs.tsx:67,105` | Active tab/badge use hardcoded `common.black` instead of token (low confidence — may be brand) | tokenise or confirm intentional |
| F-RR-24 | 🟢 | `NftsListPage.tsx:34-37` | Invalid C-strkey contract filter silently applies no filter, `hasFilters` still true → unfiltered list, no hint | validation hint (adjacent F-0272S-4) |
| F-RR-25 | 🟡 | `search/SearchResultsView.tsx:69-135` | `/search` reimplements error + 3 empty states INLINE — no retry button, swallows error classification (rate-limit/transient/generic→one msg). The one true SM-1 straggler (= F-W4R-1) | route through `QueryErrorState` + `EmptyState`; hook expose `error`/`refetch` |
| F-RR-26 | 🟡 | `libs/ui/src/index.ts`; `libs/ui/package.json` | Main barrel re-exports `TimeSeriesChart` (`@mui/x-charts`, heaviest dep) + `OperationFlowTree`; no `"sideEffects": false` → tree-shake contract absent (works today via route split, fragile) | add `sideEffects:false` + sub-path export; reinforces F-AI-2 |
| F-RR-27 | 🟢 | `accounts/AccountsTable.tsx:19`; `ledgers/LedgersTable.tsx:88` | Columns rebuilt in-render (no `useMemo`) while 12 sibling tables use module-level const; `ExplorerTable` not `React.memo` | useMemo or memo ExplorerTable |
| F-RR-28 | 🟢 | `web/src/pages/format.test.ts` | Test outlived its deleted source (`format.ts` removed in 0272); now tests a libs/ui formatter from web root | move beside `libs/ui/src/format/amount.ts` |
| F-RR-29 | 🟢 | `EmptyState` / `TableEmptyState` / `nft-detail/NftMetadata.tsx:59-73` / `assets/AssetMetadata.tsx:88-96` | 4 parallel empty-state renderers (visual drift: icon size/chip-bg/radius differ) | consolidate (= F-W4R-3, confirms F-U-5 / card 2.3) |
| F-RR-30 | 🟢 | `AccountTransactions`/`AssetTransactions`/`ContractInvocations`/`ContractEvents`/`PoolParticipants`/`PoolTransactions`/`NftTransfers` | Identical loading/error/empty/table body-switch + pagination hand-copied 7× (no `SectionTableCard` primitive) | extract primitive (= F-W4R-2, card 2.3) |
| F-RR-31 | 🟢 | `AccountDetailPage.tsx:36-53`; `AssetDetailPage.tsx:39-57`; `LiquidityPoolDetailPage.tsx:54-72` | Success-with-null-data path renders blank (no `if(!data) NotFound` guard) — other detail pages have it | add guard (= F-W4R-5, defensive) |
| F-RR-32 | 🟢 | `search/SearchResultsPage.tsx:38-43` | Singleton-redirect depends on a 2-pass auto-tab-switch effect — works today, fragile | note only, no fix |

### CONFIRMED still-open (existing cards — no new action, reconfirmed valid on merged code)

- **F-AQ-8** — `results_meta_xdr` triple-cast (`transaction-detail/index.tsx:130-137`) now provably DEAD (generated type documents the field is intentionally not surfaced) → card 6.3.
- **F-Z-2** — operation-type 27-entry enum hand-typed (`transactions/operationTypes.ts:21-58`); backend exposes `filter[operation_type]` as bare `string` → card 6.3.
- **F-AQ-1** — `noUncheckedIndexedAccess` disabled; live hazards `utils/text.ts:3`, `ledgers/LedgerSummary.tsx:146` → card 3.1.
- **F-AQ-7** — XDR `details: unknown` defensive narrowing (correct-by-design; root fix backend) → card 6.3.
- **F-AK-2 / F-DP-3** — raw z-index literals, no scale (`AppShell:182`/`SecondaryNav:49`/`TopNav:98`/`Footer:66`/`PageGridBackdrop:26`/`HomeHeroGlow:18`) → card 11.3.
- **F-AH-7 / F-X-2 / F-AH-4 / F-U-1** — folder structure (`web/src/search/` sibling, `web/src/utils/` single-file now `text.ts`, `web/src/pages/detail/` 7 generic primitives) → card 2.2.
- **F-X-1** — `liquidity-pools/` ↔ `pool-detail/` cross-folder coupling → existing.

### Live re-run scope note

Live waves 5-6 (full 42-cell responsive matrix + Tier-4 subjective visual) were NOT exhaustively re-run this pass. Targeted live done this session: 404 main/h1, NetworkToggle gone, home h1+live indicator, responsive hamburger @375 (scrollWidth 364≤375), share-% real, loading-skeleton flicker (card 7.10). Two NEW visual findings need a live light-mode check to confirm severity: **F-RR-16** (NftEventBadge dark colors — only visible if app supports light mode) and **F-RR-17** (PoolCharts error masking — needs error injection). Full live matrix re-run = separate pass if required.

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

| Finding                                        | Wave          | Sev     | Cluster                 | STATUS      | Notes                                                                                                                                                                                                                                                            |
| ---------------------------------------------- | ------------- | ------- | ----------------------- | ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A1                                             | 1             | 🔴      | —                       | RESOLVED    | TxDetail stub — a2c1b205 (FilipDz PR #215)                                                                                                                                                                                                                       |
| A2                                             | 1             | 🟠      | C 6.1                   | TODO        | 0066 task body drift                                                                                                                                                                                                                                             |
| A3                                             | 1             | 🟠      | C 6.2                   | TODO        | 25/28 Future Work un-spawned                                                                                                                                                                                                                                     |
| A4                                             | 1             | 🟡      | C 8.1                   | TODO        | 0226 test infra blocked                                                                                                                                                                                                                                          |
| A5                                             | 1             | 🟡      | C 10.1                  | TODO        | 0199/0215 LP blocked                                                                                                                                                                                                                                             |
| F-AF-1                                         | 1             | 🟡      | C 8.4                   | TODO        | Error interceptor flattens envelope                                                                                                                                                                                                                              |
| F-AF-2                                         | 1             | 🟢      | C 8.4                   | TODO        | Object.assign(error) smell                                                                                                                                                                                                                                       |
| F-AF-3                                         | 1             | 🟢      | —                       | SKIP        | as unknown as `useNow.ts` justified                                                                                                                                                                                                                              |
| F-AF-4                                         | 1             | 🟢      | C 8.4                   | TODO        | envelopeMessage object-string guard                                                                                                                                                                                                                              |
| F-AQ-1                                         | 1             | 🟠      | C 3.1                   | TODO        | noUncheckedIndexedAccess flag                                                                                                                                                                                                                                    |
| F-AQ-2                                         | 1             | 🟡      | C 3.1                   | TODO        | exactOptionalPropertyTypes flag                                                                                                                                                                                                                                  |
| F-AQ-3                                         | 1             | 🟡      | C 3.3                   | TODO        | Switch exhaustiveness + assertNever                                                                                                                                                                                                                              |
| F-AQ-4                                         | 1             | 🟠      | C 3.2                   | TODO        | Branded ID types                                                                                                                                                                                                                                                 |
| F-AQ-5                                         | 1             | 🟢      | —                       | SKIP        | Discriminated unions zero — no issue                                                                                                                                                                                                                             |
| F-AQ-6                                         | 1             | 🟢      | —                       | SKIP        | Generic constraints sensible — no issue                                                                                                                                                                                                                          |
| F-AQ-7                                         | 1             | 🟡      | C 6.3                   | TODO        | XDR unknown casts (backend coordination)                                                                                                                                                                                                                         |
| F-AQ-8                                         | 1             | 🟡      | C 6.3                   | TODO        | results_meta_xdr codegen drift                                                                                                                                                                                                                                   |
| F-P-1                                          | 1             | 🟡      | C 3.1                   | TODO        | Lint warning assetColor.ts:131                                                                                                                                                                                                                                   |
| F-P-2                                          | 1             | 🟡      | C 8.2                   | TODO        | No knip/ts-prune in CI                                                                                                                                                                                                                                           |
| F-P-3                                          | 1             | ✓       | —                       | RESOLVED    | Zero console.\* in source (baseline)                                                                                                                                                                                                                             |
| F-P-4                                          | 1             | ✓       | —                       | RESOLVED    | Zero TODO/FIXME markers (baseline)                                                                                                                                                                                                                               |
| F-P-5                                          | 1             | ✓       | —                       | RESOLVED    | Zero commented-out blocks (baseline)                                                                                                                                                                                                                             |
| F-P-6                                          | 1             | 🟢      | C 8.2                   | TODO        | Cyclical imports not checked                                                                                                                                                                                                                                     |
| F-P-7                                          | 1             | 🟢      | C 7.1 (overrides split) | DEFER-M2    | overrides.ts 867 LOC — splittable; F-Y-1                                                                                                                                                                                                                         |
| F-P-8                                          | 1             | 🟢      | C 8.2                   | TODO        | No bundle console-leak grep in CI                                                                                                                                                                                                                                |
| F-AI-1                                         | 1             | 🟠      | C 4.1                   | TODO        | Main bundle > 500KB                                                                                                                                                                                                                                              |
| F-AI-2                                         | 1             | 🟠      | C 4.1                   | TODO        | LP detail chunk 313KB                                                                                                                                                                                                                                            |
| F-AI-3                                         | 1             | 🟡      | C 4.1                   | TODO        | SearchOutlined 67KB chunk anomaly                                                                                                                                                                                                                                |
| F-AI-4                                         | 1             | 🟢      | —                       | SKIP        | ExplorerTable chunk informational                                                                                                                                                                                                                                |
| F-AI-5                                         | 1             | ✓       | —                       | RESOLVED    | Devtools tree-shake confirmed                                                                                                                                                                                                                                    |
| F-AI-6                                         | 1             | ✓       | —                       | RESOLVED    | Tree-shake validated                                                                                                                                                                                                                                             |
| F-AI-7                                         | 1             | 🟡      | C 4.1                   | TODO        | No bundle visualizer                                                                                                                                                                                                                                             |
| F-AI-8                                         | 1             | 🟡      | C 4.1                   | TODO        | No vendor chunk split                                                                                                                                                                                                                                            |
| F-AI-9                                         | 1             | ✓       | —                       | RESOLVED    | CSS total tiny (informational)                                                                                                                                                                                                                                   |
| F-AI-10                                        | 1             | 🟡      | C 4.1                   | TODO        | TxDetail chunk 30KB (Filip baseline)                                                                                                                                                                                                                             |
| F-AI-11                                        | 1             | 🟢      | —                       | SKIP        | TransactionsListPage +0.15KB (informational)                                                                                                                                                                                                                     |
| F-CO-1                                         | 1             | 🟠      | —                       | RESOLVED    | Vite 7.3.3 CVE bump — 473de2a2                                                                                                                                                                                                                                   |
| F-CO-2                                         | 1             | 🟢      | C 8.2                   | TODO        | lodash-es allowlist                                                                                                                                                                                                                                              |
| F-CO-3                                         | 1             | 🟡      | C 10.2                  | TODO        | MUI 7→9 bump                                                                                                                                                                                                                                                     |
| F-CO-4                                         | 1             | 🟢      | C 8.2                   | TODO        | react-router-dom 2 minor                                                                                                                                                                                                                                         |
| F-CO-5                                         | 1             | 🟡      | C 8.2                   | TODO        | eslint v8 EoL                                                                                                                                                                                                                                                    |
| F-CO-6                                         | 1             | 🟠      | C 10.2 / C 8.2          | TODO        | mui/utils triple-version                                                                                                                                                                                                                                         |
| F-CO-7                                         | 1             | 🟢      | C 8.2                   | TODO        | No Renovate/Dependabot                                                                                                                                                                                                                                           |
| F-CO-8                                         | 1             | 🟢      | C 8.2                   | TODO        | prettier 2→3                                                                                                                                                                                                                                                     |
| C-1                                            | 2             | ✓       | —                       | RESOLVED    | normalizeOperationType H2 root cause baseline                                                                                                                                                                                                                    |
| C-2                                            | 2             | ✓       | —                       | RESOLVED    | 27 ops parity holds                                                                                                                                                                                                                                              |
| C-3                                            | 2             | 🟢      | C 8.6                   | TODO        | Non-op enums no FE mirror (document)                                                                                                                                                                                                                             |
| C-4                                            | 2             | 🟢      | C 8.6                   | TODO        | Polymorphic ID link builders inconsistent encoding                                                                                                                                                                                                               |
| C-5                                            | 2             | 🟡      | C 3.2                   | TODO        | Missing isAssetId / isNftId validator                                                                                                                                                                                                                            |
| C-6                                            | 2             | ✓       | —                       | RESOLVED    | Pool id strkey/hex round-trip OK                                                                                                                                                                                                                                 |
| C-7                                            | 2             | partial | —                       | RESOLVED    | UTC timestamps consistent (baseline)                                                                                                                                                                                                                             |
| C-8                                            | 2             | 🟢      | C 8.6                   | TODO        | No `<time dateTime>` element                                                                                                                                                                                                                                     |
| C-9                                            | 2             | ✓       | —                       | RESOLVED    | Trailing-zero trim works                                                                                                                                                                                                                                         |
| C-10                                           | 2             | ✓       | —                       | RESOLVED    | minDecimals floor works                                                                                                                                                                                                                                          |
| C-11                                           | 2             | 🟢      | C 8.6                   | TODO        | Em-dash vs ellipsis convention undocumented                                                                                                                                                                                                                      |
| C-12                                           | 2             | ✓       | —                       | RESOLVED    | Em-dash exclusive (no hyphen)                                                                                                                                                                                                                                    |
| C-13                                           | 2             | ✓       | —                       | RESOLVED    | Cursor pagination semantic uniform                                                                                                                                                                                                                               |
| C-14                                           | 2             | ✓       | —                       | RESOLVED    | useCursorPagination single hook                                                                                                                                                                                                                                  |
| C-15                                           | 2             | 🟢      | C 8.5                   | TODO        | Polling cache headers per-endpoint (minor smell)                                                                                                                                                                                                                 |
| C-16                                           | 2             | —       | —                       | SKIP        | Polling pause check deferred to 1.22 (covered by F-I-3)                                                                                                                                                                                                          |
| C-17                                           | 2             | 🟠      | C 6.3                   | TODO        | No CorsLayer (infra coordination)                                                                                                                                                                                                                                |
| C-18                                           | 2             | ✓       | —                       | RESOLVED    | FE client credentials OK                                                                                                                                                                                                                                         |
| C-19                                           | 2             | ✓       | —                       | RESOLVED    | Error envelope shape OK                                                                                                                                                                                                                                          |
| C-20                                           | 2             | ✓       | —                       | RESOLVED    | API base URL config OK                                                                                                                                                                                                                                           |
| J-1                                            | 2             | ✓       | —                       | RESOLVED    | formatAmount usage healthy                                                                                                                                                                                                                                       |
| J-2                                            | 2             | 🟡      | C 2.1                   | TODO        | 10 toLocaleString sites bypass formatter                                                                                                                                                                                                                         |
| J-3                                            | 2             | 🟡      | C 2.1                   | RESOLVED    | TopNav.formatNumber kept local (deliberate, ≠ formatCompactAmount); PoolKpiStrip COUNT→formatInteger (lore-0272)                                                                                                                                                 |
| J-4                                            | 2             | 🟠      | C 2.1                   | TODO        | STROOPS_PER_XLM single site (drift realized)                                                                                                                                                                                                                     |
| J-5                                            | 2             | 🟡      | C 2.1                   | TODO        | Timestamp depth inconsistency                                                                                                                                                                                                                                    |
| J-6                                            | 2             | 🟢      | C 8.6                   | TODO        | No `<time>` element (cross-cite C-8)                                                                                                                                                                                                                             |
| J-7                                            | 2             | 🟠      | C 2.1                   | TODO        | Truncation re-impls (6 sites)                                                                                                                                                                                                                                    |
| J-8                                            | 2             | ✓       | —                       | RESOLVED    | Hash truncation per-type via IdentifierDisplay                                                                                                                                                                                                                   |
| J-9                                            | 2             | ✓       | —                       | RESOLVED    | Strkey vs hex pool strategy documented                                                                                                                                                                                                                           |
| J-10                                           | 2             | ✓       | —                       | RESOLVED    | Asset labels with issuer disambig OK                                                                                                                                                                                                                             |
| J-11                                           | 2             | 🟢      | C 8.6                   | TODO        | Percentages decimals no shared constant                                                                                                                                                                                                                          |
| J-12                                           | 2             | ✓       | —                       | RESOLVED    | Status badge colors consistent                                                                                                                                                                                                                                   |
| J-13                                           | 2             | ✓       | —                       | RESOLVED    | Event-type chip colors single map                                                                                                                                                                                                                                |
| J-14                                           | 2             | 🟢      | C 8.6                   | TODO        | Currency symbol XLM hardcoded                                                                                                                                                                                                                                    |
| J-15                                           | 2             | ✓       | —                       | RESOLVED    | Em-dash convention OK                                                                                                                                                                                                                                            |
| F-J-16                                         | 2             | 🟠      | C 2.1                   | RESOLVED    | Single formatFee (BigInt), stroops→XLM inlined (lore-0272)                                                                                                                                                                                                       |
| F-J-17                                         | 2             | 🟡      | C 2.1                   | RESOLVED    | formatStroops single entry point in format/stroops.ts (lore-0272)                                                                                                                                                                                                |
| Q-1                                            | 2             | ✓       | —                       | RESOLVED    | Acceptance Criteria present in archived tasks                                                                                                                                                                                                                    |
| Q-2                                            | 2             | ✓       | —                       | RESOLVED    | Design Decisions section present                                                                                                                                                                                                                                 |
| Q-3                                            | 2             | 🟢      | C 6.4                   | TODO        | 0246 missing Issues Encountered heading                                                                                                                                                                                                                          |
| Q-4                                            | 2             | 🟠      | C 6.1                   | TODO        | 0066 triple-drift                                                                                                                                                                                                                                                |
| Q-5                                            | 2             | ✓       | —                       | RESOLVED    | API commits include openapi regen                                                                                                                                                                                                                                |
| Q-6                                            | 2             | 🟢      | —                       | RESOLVED    | ADR 0032 evergreen-docs gate honored (baseline)                                                                                                                                                                                                                  |
| Q-7                                            | 2             | ✓       | —                       | RESOLVED    | ADR cross-ref density healthy                                                                                                                                                                                                                                    |
| Q-7 (post-merge)                               | 2             | 🟡      | C 6.2                   | TODO        | Forward-link expectation mismatch 0254↔0257                                                                                                                                                                                                                      |
| AR-1                                           | 2             | ✓       | —                       | RESOLVED    | Conventional Commits 81% compliance                                                                                                                                                                                                                              |
| AR-2                                           | 2             | 🟢      | C 8.8                   | TODO        | Mixed lore-scope styles                                                                                                                                                                                                                                          |
| AR-3                                           | 2             | 🟡      | C 8.8                   | TODO        | Commitlint missing                                                                                                                                                                                                                                               |
| AR-4                                           | 2             | 🟡      | C 8.8                   | TODO        | PR template missing                                                                                                                                                                                                                                              |
| AR-5                                           | 2             | ✓       | —                       | RESOLVED    | Branch naming OK                                                                                                                                                                                                                                                 |
| AR-6                                           | 2             | ✓       | —                       | RESOLVED    | Husky pre-commit OK                                                                                                                                                                                                                                              |
| AR-7                                           | 2             | 🟡      | C 8.8                   | TODO        | Branch protection check (human)                                                                                                                                                                                                                                  |
| AR-8                                           | 2             | 🟢      | C 8.8                   | TODO        | No CHANGELOG.md                                                                                                                                                                                                                                                  |
| DM-1                                           | 2             | 🟠      | C 7.2                   | TODO        | Footer "All systems operational" hardcoded                                                                                                                                                                                                                       |
| DM-2                                           | 2             | 🟢      | C 7.2                   | TODO        | No /health probe                                                                                                                                                                                                                                                 |
| DN-1                                           | 2             | 🟠      | C 1.2                   | TODO        | No build SHA in UI                                                                                                                                                                                                                                               |
| DN-2                                           | 2             | 🟡      | C 1.2                   | TODO        | No vite define block                                                                                                                                                                                                                                             |
| CA-1                                           | 2             | 🟠      | C 1.1                   | TODO        | Footer Terms/Privacy/Cookies dead spans                                                                                                                                                                                                                          |
| CA-2                                           | 2             | 🟠      | C 1.1                   | TODO        | Footer Resources dead spans                                                                                                                                                                                                                                      |
| CA-3                                           | 2             | 🟢      | C 1.1                   | TODO        | target=\_blank+rel preventive                                                                                                                                                                                                                                    |
| CA-4                                           | 2             | ✓       | —                       | RESOLVED    | Copyright line OK                                                                                                                                                                                                                                                |
| AO-1                                           | 2             | ✓       | —                       | RESOLVED    | .env.example exists                                                                                                                                                                                                                                              |
| AO-2                                           | 2             | ✓       | —                       | RESOLVED    | web/.env.example covers VITE\_\*                                                                                                                                                                                                                                 |
| AO-3                                           | 2             | ✓       | —                       | RESOLVED    | No hardcoded localhost in src                                                                                                                                                                                                                                    |
| AO-4                                           | 2             | ✓       | —                       | RESOLVED    | No console.\* leftover                                                                                                                                                                                                                                           |
| AO-5                                           | 2             | ✓       | —                       | RESOLVED    | .gitignore coverage OK                                                                                                                                                                                                                                           |
| AO-6                                           | 2             | ✓       | —                       | RESOLVED    | No secrets in history                                                                                                                                                                                                                                            |
| AO-7                                           | 2             | ✓       | —                       | RESOLVED    | CI typescript gate OK                                                                                                                                                                                                                                            |
| AO-8                                           | 2             | ✓       | —                       | RESOLVED    | CI api-types-codegen gate OK                                                                                                                                                                                                                                     |
| AO-9                                           | 2             | 🟢      | C 8.6                   | TODO        | No FE prod deploy workflow                                                                                                                                                                                                                                       |
| AO-10                                          | 2             | 🟢      | C 8.6                   | TODO        | No PR preview-deploy workflow                                                                                                                                                                                                                                    |
| AO-11                                          | 2             | —       | C 1.2                   | TODO        | Prod build version stamp (covered by DN-1)                                                                                                                                                                                                                       |
| K-1 (=F-K-1)                                   | 3             | 🟠      | —                       | RESOLVED    | TxDetail stub — a2c1b205 (Filip)                                                                                                                                                                                                                                 |
| F-K-2                                          | 3             | 🟠      | —                       | RESOLVED    | Pool reserve links — 473de2a2 + a5f15166                                                                                                                                                                                                                         |
| F-K-3                                          | 3             | 🟠      | —                       | RESOLVED    | Pool participants "Since ledger" link — 473de2a2                                                                                                                                                                                                                 |
| F-K-4                                          | 3             | 🟡      | —                       | RESOLVED    | Pool URL strkey hint — 6421d3d7 (0270)                                                                                                                                                                                                                           |
| F-K-5                                          | 3             | 🟢      | —                       | SKIP        | Account self-link cosmetic — no fix                                                                                                                                                                                                                              |
| F-K-6                                          | 3             | 🟢      | —                       | SKIP        | Account TX no source-account column (intentional)                                                                                                                                                                                                                |
| F-K-7                                          | 3             | 🟡      | C 5.4                   | TODO        | E3 tx-detail ledger link verification                                                                                                                                                                                                                            |
| F-K-8                                          | 3             | 🟡      | C 5.4                   | TODO        | Soroban call tree destination account routing                                                                                                                                                                                                                    |
| F-K-9                                          | 3             | 🟠      | —                       | RESOLVED    | PoolAssetLeg schema gap — 473de2a2                                                                                                                                                                                                                               |
| F-E-1                                          | 3             | 🔴      | —                       | RESOLVED    | URL cursor write — f646047d (0254 merge)                                                                                                                                                                                                                         |
| F-E-2                                          | 3             | 🟠      | —                       | SKIP        | URL wire contract — user-dropped 2026-05-25                                                                                                                                                                                                                      |
| F-E-3                                          | 3             | 🟡      | C 5.1                   | RESOLVED    | Catch-all `path:'*'` NotFoundPage in AppShell `<main>` (lore-0272; h1 part dropped per user)                                                                                                                                                                     |
| F-E-4                                          | 3             | ✓       | —                       | RESOLVED    | Filter URL preserves refresh OK                                                                                                                                                                                                                                  |
| F-E-5                                          | 3             | ✓       | —                       | RESOLVED    | Trailing slash tolerated                                                                                                                                                                                                                                         |
| F-E-6                                          | 3             | ✓       | —                       | RESOLVED    | Deep link from raw URL OK                                                                                                                                                                                                                                        |
| F-E-7                                          | 3             | 🟡      | C 5.2                   | TODO        | No URL state for tabs                                                                                                                                                                                                                                            |
| F-E-8                                          | 3             | 🟢      | —                       | RESOLVED    | cursor_p/\_e/\_i — same fix as F-E-1                                                                                                                                                                                                                             |
| F-L-1                                          | 3             | 🟠      | —                       | RESOLVED    | Pool strkey search — 047ce51e + 6421d3d7 (0270)                                                                                                                                                                                                                  |
| F-L-2                                          | 3             | 🟡      | C 7.1                   | TODO        | Hint enumerates 4 of 6 entity types                                                                                                                                                                                                                              |
| F-L-3                                          | 3             | ✓       | —                       | RESOLVED    | XSS escaped (baseline)                                                                                                                                                                                                                                           |
| F-L-4                                          | 3             | ✓       | —                       | RESOLVED    | Debounce confirmed                                                                                                                                                                                                                                               |
| F-L-5                                          | 3             | ✓       | —                       | RESOLVED    | Long query handled gracefully                                                                                                                                                                                                                                    |
| F-L-6                                          | 3             | 🟡      | —                       | SKIP        | treatRedirectAsResult flag (catalog-only; no bug)                                                                                                                                                                                                                |
| F-H-1                                          | 3             | ✓       | —                       | RESOLVED    | Zero console.\* (baseline)                                                                                                                                                                                                                                       |
| F-H-2                                          | 3             | ✓       | —                       | RESOLVED    | Zero dangerouslySetInnerHTML / eval                                                                                                                                                                                                                              |
| F-H-3                                          | 3             | ✓       | —                       | RESOLVED    | XSS probe escaped                                                                                                                                                                                                                                                |
| F-H-4                                          | 3             | ✓       | —                       | RESOLVED    | safeHttpUrl link injection guard                                                                                                                                                                                                                                 |
| F-H-5                                          | 3             | ✓       | —                       | RESOLVED    | target=\_blank with rel=noopener                                                                                                                                                                                                                                 |
| F-H-6                                          | 3             | ✓       | —                       | RESOLVED    | Zero iframe                                                                                                                                                                                                                                                      |
| F-H-7                                          | 3             | ✓       | —                       | RESOLVED    | localStorage minimal + non-sensitive                                                                                                                                                                                                                             |
| F-H-8                                          | 3             | ✓       | —                       | RESOLVED    | Zero sessionStorage                                                                                                                                                                                                                                              |
| F-H-9                                          | 3             | ✓       | —                       | RESOLVED    | Zero document.cookie                                                                                                                                                                                                                                             |
| F-H-10                                         | 3             | ✓       | —                       | RESOLVED    | Auth headers only in generated SDK                                                                                                                                                                                                                               |
| F-H-11                                         | 3             | ✓       | —                       | RESOLVED    | Env vars constrained                                                                                                                                                                                                                                             |
| H-12                                           | 3             | 🟢      | C 8.6                   | TODO        | Color-mode storage key naming                                                                                                                                                                                                                                    |
| F-I-1                                          | 3             | ✓       | —                       | RESOLVED    | Polling policies segmented                                                                                                                                                                                                                                       |
| F-I-2                                          | 3             | ✓       | —                       | RESOLVED    | Live verification matches intervals                                                                                                                                                                                                                              |
| F-I-3                                          | 3             | 🟡      | C 8.5                   | TODO        | No visibilitychange pause doc                                                                                                                                                                                                                                    |
| F-I-4                                          | 3             | 🟠      | C 8.5                   | TODO        | invalidateResource dead/abandoned                                                                                                                                                                                                                                |
| F-I-5                                          | 3             | ✓       | C 7.6                   | TODO        | TanStack dedup confirmed (validate same-key)                                                                                                                                                                                                                     |
| F-I-6                                          | 3             | 🟢      | C 8.5                   | TODO        | refetchIntervalInBackground not explicit                                                                                                                                                                                                                         |
| F-I-7                                          | 3             | 🟡      | C 8.5                   | TODO        | gcTime not set on listPolicy/detailPolicy                                                                                                                                                                                                                        |
| F-I-8                                          | 3             | ✓       | —                       | RESOLVED    | Retry policy excludes 4xx                                                                                                                                                                                                                                        |
| F-D-1                                          | 4             | 🔴      | —                       | RESOLVED    | API stale binary — restart 2026-05-25                                                                                                                                                                                                                            |
| F-D-2                                          | 4             | 🟠      | —                       | RESOLVED    | Composite NotFound — 473de2a2 + 9e88114b                                                                                                                                                                                                                         |
| F-D-3                                          | 4             | 🟡      | C 5.1 / C 7.1           | TODO        | Detail page H1 heading inconsistency                                                                                                                                                                                                                             |
| F-D-4                                          | 4             | 🟡      | C 7.2                   | TODO        | Polling indicator absent on detail pages                                                                                                                                                                                                                         |
| F-D-5                                          | 4             | 🟡      | —                       | SKIP        | E5 empty-state spot-check unverified (low-pri)                                                                                                                                                                                                                   |
| F-AE-1                                         | 4             | 🟢      | C 8.6                   | TODO        | favicon.ico 404                                                                                                                                                                                                                                                  |
| F-AE-2                                         | 4             | 🟢      | —                       | RESOLVED    | try/catch inventory baseline                                                                                                                                                                                                                                     |
| F-AE-3                                         | 4             | 🟡      | C 8.4                   | TODO        | SectionErrorBoundary inconsistent coverage                                                                                                                                                                                                                       |
| F-AE-4                                         | 4             | 🟡      | C 8.4                   | TODO        | Error interceptor flattens envelope (recap)                                                                                                                                                                                                                      |
| F-AE-5                                         | 4             | 🟠      | —                       | RESOLVED    | Composite NotFound err — 473de2a2 + 9e88114b                                                                                                                                                                                                                     |
| F-AE-6                                         | 4             | 🟠      | C 8.4                   | TODO        | Silent shape-mismatch no console signal                                                                                                                                                                                                                          |
| F-AE-7                                         | 4             | 🟢      | C 8.4                   | TODO        | No global error reporter                                                                                                                                                                                                                                         |
| F-U-1                                          | 4             | 🟡      | C 2.2                   | TODO        | SectionCard wrong home                                                                                                                                                                                                                                           |
| F-U-2                                          | 4             | 🟡      | C 2.1                   | RESOLVED    | Inline toFixed/toLocaleString → shared format helpers (lore-0272)                                                                                                                                                                                                |
| F-U-3                                          | 4             | 🟠      | C 2.1                   | RESOLVED    | All ad-hoc truncators → canonical truncateMiddle (`c57f7c4d`)                                                                                                                                                                                                    |
| F-U-4                                          | 4             | 🟠      | C 2.1                   | RESOLVED    | Single STROOPS_PER_XLM_BIGINT in format/stroops.ts (lore-0272)                                                                                                                                                                                                   |
| F-U-5                                          | 4             | 🟡      | C 2.3                   | TODO        | EmptyState minor reuse violation                                                                                                                                                                                                                                 |
| F-X-1                                          | 4             | 🟡      | C 2.2                   | TODO        | assetLegLabel cross-folder reach                                                                                                                                                                                                                                 |
| F-X-2                                          | 4             | 🟢      | C 2.2                   | TODO        | web/src/pages/detail/ single-file                                                                                                                                                                                                                                |
| F-X-3                                          | 4             | 🟡      | —                       | RESOLVED    | usePageHandlers shared chunk (positive baseline)                                                                                                                                                                                                                 |
| F-X-4                                          | 4             | 🟡      | C 6.4                   | TODO        | Hooks colocated in two places (document)                                                                                                                                                                                                                         |
| F-X-5                                          | 4             | 🟢      | C 2.2                   | TODO        | web/src/utils/ single-file                                                                                                                                                                                                                                       |
| F-AL-1                                         | 4             | 🟡      | C 5.2                   | DEFER-M2    | tx-detail selectedIndex useState (borderline)                                                                                                                                                                                                                    |
| F-AL-2                                         | 4             | 🟢      | C 6.4                   | TODO        | useDetailMode parallel pattern doc                                                                                                                                                                                                                               |
| F-AN-1                                         | 4             | 🟡      | —                       | DEFER-M2    | Strkey vs hex strategy (partly resolved 0264; remainder bidirectional util)                                                                                                                                                                                      |
| F-AN-2                                         | 4             | 🟢      | —                       | RESOLVED    | XDR rendering inventory clean baseline                                                                                                                                                                                                                           |
| F-AN-3                                         | 4             | 🟡      | C 7.1                   | TODO        | Op-type label single source; icon mapping absent (Figma check)                                                                                                                                                                                                   |
| F-AN-4                                         | 4             | 🟢      | —                       | RESOLVED    | SEP-1 TOML enrichment OK                                                                                                                                                                                                                                         |
| F-AN-5                                         | 4             | 🟡      | C 6.4                   | TODO        | Soroban-era ledger detection absent (document)                                                                                                                                                                                                                   |
| F-AN-6                                         | 4             | 🟢      | C 6.4 / C 11.1          | TODO        | Mainnet/Testnet config single-env — REGRESSED by `06ab34cc`: NetworkToggle added but NON-FUNCTIONAL (fake toggle, no apiBaseUrl/query-key change, invisible on `/`); worse than no-toggle baseline. See F-DP-1 / card 11.1 (wire-or-hide) + design-parity-impact |
| F-AN-7                                         | 4             | 🟠      | C 2.1                   | RESOLVED    | Stroop/XLM single place (recap F-U-4, lore-0272)                                                                                                                                                                                                                 |
| F-AN-8                                         | 4             | 🟠      | —                       | RESOLVED    | Strkey canonical convention — 473de2a2 (0264)                                                                                                                                                                                                                    |
| F-AE-1..F-AE-7                                 | 4             | various | (above)                 | (above)     | (see individual rows)                                                                                                                                                                                                                                            |
| F-A-1                                          | 5             | 🟡      | —                       | RESOLVED    | Spec drift 0246 Phase 3 dropped (positive baseline)                                                                                                                                                                                                              |
| F-A-2                                          | 5             | 🟡      | —                       | RESOLVED    | 0254 BREAKING wire rename clean (positive baseline)                                                                                                                                                                                                              |
| F-A-3                                          | 5             | 🟡      | C 6.4                   | TODO        | ADR 0032 partial gap on 0254 (doc sync)                                                                                                                                                                                                                          |
| F-A-4                                          | 5             | 🟡      | —                       | RESOLVED    | LP feature gold-standard exemplar (positive note)                                                                                                                                                                                                                |
| F-A-5                                          | 5             | 🟡      | C 1.3                   | PARTIAL     | Contract list page gap (launch blocker) — nav added, page stubbed via PageStub (design_parity `06ab34cc`); list page still TODO                                                                                                                                  |
| F-A-6                                          | 5             | 🟢      | —                       | RESOLVED    | Tx-detail spec/ship chain clean                                                                                                                                                                                                                                  |
| F-A-7                                          | 5             | 🟢      | —                       | RESOLVED    | Deviation notes discipline excellent                                                                                                                                                                                                                             |
| F-AH-1                                         | 5             | 🟡      | C 2.2                   | STALE       | PageStub.tsx dead orphan — FALSE post-`06ab34cc`: PageStub revived as `/accounts`+`/contracts` stub (2 live consumers); deletion gated behind card 1.3                                                                                                           |
| F-AH-2                                         | 5             | 🟡      | C 2.2                   | TODO        | Folder asymmetry                                                                                                                                                                                                                                                 |
| F-AH-3                                         | 5             | 🟡      | C 2.2                   | TODO        | SectionCard wrong home (recap)                                                                                                                                                                                                                                   |
| F-AH-4                                         | 5             | 🟢      | C 2.2                   | TODO        | web/src/utils/ single-file (recap)                                                                                                                                                                                                                               |
| F-AH-5                                         | 5             | 🟢      | C 2.2                   | TODO        | web/src/pages/detail/ misnamed (recap)                                                                                                                                                                                                                           |
| F-AH-6                                         | 5             | 🟢      | C 8.1                   | TODO        | No tests (cross-cite testing baseline)                                                                                                                                                                                                                           |
| F-AH-7                                         | 5             | 🟢      | C 2.2                   | TODO        | web/src/search/ parallel folder                                                                                                                                                                                                                                  |
| F-AH-8                                         | 5             | 🟢      | C 2.2                   | TODO        | Page-root helpers mixed with \*Page.tsx                                                                                                                                                                                                                          |
| F-Y-1                                          | 5             | 🟡      | —                       | DEFER-M2    | overrides.ts 890 LOC split (low stakes)                                                                                                                                                                                                                          |
| F-Y-2                                          | 5             | 🟠      | C 2.1                   | RESOLVED    | useDebouncedDraft extracted; 4 filters migrated (lore-0272)                                                                                                                                                                                                      |
| F-Y-3                                          | 5             | 🟢      | —                       | RESOLVED    | useEffect discipline good (baseline)                                                                                                                                                                                                                             |
| F-Y-4                                          | 5             | 🟢      | —                       | DEFER-M2    | PoolCharts 268 LOC borderline                                                                                                                                                                                                                                    |
| F-Y-5                                          | 5             | 🟢      | —                       | RESOLVED    | Long files domain-justified (baseline)                                                                                                                                                                                                                           |
| F-Y-6                                          | 5             | 🟡      | C 2.1                   | RESOLVED    | Cross-cites formatter/truncation (recap, lore-0272)                                                                                                                                                                                                              |
| F-Z-1                                          | 5             | 🟡      | C 2.1                   | RESOLVED    | Single formatter home libs/ui/src/format/ (lore-0272)                                                                                                                                                                                                            |
| F-Z-2                                          | 5             | 🟢      | C 6.3                   | TODO        | Op-type enum hand-typed (backend coordination)                                                                                                                                                                                                                   |
| F-Z-3                                          | 5             | 🟢      | —                       | DEFER-M2    | Chip JSDoc @param polish                                                                                                                                                                                                                                         |
| F-Z-4                                          | 5             | 🟢      | C 6.4                   | TODO        | frontend-data-flow wiki                                                                                                                                                                                                                                          |
| F-AA-1                                         | 5             | 🟢      | —                       | SKIP        | Single-consumer abstractions (keep-or-trim toss-up)                                                                                                                                                                                                              |
| F-AA-2                                         | 5             | 🟢      | —                       | RESOLVED    | Zero Redux/Zustand (positive baseline)                                                                                                                                                                                                                           |
| F-AA-3                                         | 5             | 🟢      | —                       | RESOLVED    | useDebounced will broaden in C 2.1                                                                                                                                                                                                                               |
| F-AA-4                                         | 5             | 🟢      | C 6.4                   | TODO        | useIntersectionObserver single-consumer wiki note                                                                                                                                                                                                                |
| F-AA-5                                         | 5             | 🟢      | —                       | RESOLVED    | Provider count minimal (positive baseline)                                                                                                                                                                                                                       |
| F-AA-6                                         | 5             | 🟢      | —                       | RESOLVED    | Hook proliferation bounded (positive baseline)                                                                                                                                                                                                                   |
| F-AB-1                                         | 5             | 🟡      | C 6.4                   | TODO        | useDetailMode divergence not in task body                                                                                                                                                                                                                        |
| F-AB-2                                         | 5             | 🟡      | C 6.4                   | TODO        | Interval labels 0065 #5 spec not amended                                                                                                                                                                                                                         |
| F-AB-3                                         | 5             | 🟢      | C 10.3                  | TODO        | 0251 B1 fix-by-hide root-cause fix                                                                                                                                                                                                                               |
| F-AB-4                                         | 5             | 🟢      | C 8.7                   | TODO        | Sort-caret middle-ground designer sign-off                                                                                                                                                                                                                       |
| F-AB-5                                         | 5             | 🟠      | C 2.1                   | RESOLVED    | Cross-task formatter dups closed (recap symptom, lore-0272)                                                                                                                                                                                                      |
| F-AD-1                                         | 5             | 🟠      | C 2.1                   | RESOLVED    | Leaked-concern fixed — truncation/format now 1-file change (lore-0272)                                                                                                                                                                                           |
| F-AD-2                                         | 5             | 🟢      | C 6.4                   | TODO        | Onboarding doc polish                                                                                                                                                                                                                                            |
| F-AD-3                                         | 5             | 🟢      | C 7.1                   | TODO        | 3 inline magic numbers (1500ms, 1062, 1064)                                                                                                                                                                                                                      |
| F-AD-4                                         | 5             | 🟢      | —                       | RESOLVED    | Zero implicit-context surprises (baseline)                                                                                                                                                                                                                       |
| F-AD-5                                         | 5             | 🟠      | C 8.1                   | TODO        | Zero test coverage (cross-cite)                                                                                                                                                                                                                                  |
| F-AC checks (AC-1..AC-14)                      | 5             | —       | (rolled up)             | (rolled up) | See F-A-1..F-A-7 above                                                                                                                                                                                                                                           |
| F-EX-1                                         | 5 sweep       | 🟡      | C 5.4                   | RESOLVED    | minted_at_ledger → IdentifierDisplay type=ledger (Figma deviation per user, lore-0272)                                                                                                                                                                           |
| F-EX-2                                         | 5 sweep       | 🟢      | C 5.2                   | TODO        | Pool chart metric/period useState                                                                                                                                                                                                                                |
| F-W6-AG-1                                      | 6             | 🟠      | C 4.1                   | TODO        | Main bundle >500KB (recap)                                                                                                                                                                                                                                       |
| F-W6-AG-2                                      | 6             | 🟠      | C 4.1                   | TODO        | LP detail chunk 300KB (recap)                                                                                                                                                                                                                                    |
| F-W6-AG-3                                      | 6             | 🟡      | C 7.1                   | TODO        | Transitions non-GPU — slight NEG from `06ab34cc` (NetworkToggle/sort-caret/Tabs add more `background-color`/`color`/`border-color` transitions; no move to transform/opacity)                                                                                    |
| F-W6-AG-4                                      | 6             | 🟢      | C 7.1                   | TODO        | 150ms transitions edge of hover rule                                                                                                                                                                                                                             |
| F-W6-AG-5                                      | 6             | 🟡      | C 7.7                   | TODO        | No route-transition loading indicator                                                                                                                                                                                                                            |
| F-W6-AG-6                                      | 6             | 🟢      | —                       | SKIP        | useMemo/useCallback spot-check informational                                                                                                                                                                                                                     |
| F-W6-AG-7                                      | 6             | 🟢      | —                       | RESOLVED    | TanStack staleTime/gcTime tuned (baseline)                                                                                                                                                                                                                       |
| F-W6-AG-8                                      | 6             | 🟢      | —                       | RESOLVED    | Cache hit on navigate-back confirmed                                                                                                                                                                                                                             |
| F-W6-AG-9                                      | 6             | 🟢      | C 7.6                   | TODO        | Polling home+header overlap                                                                                                                                                                                                                                      |
| F-W6-AP-1                                      | 6             | 🟡      | C 2.3                   | TODO        | Loading pattern inconsistency                                                                                                                                                                                                                                    |
| F-W6-AP-2                                      | 6             | 🟢      | C 7.2                   | TODO        | Polling refresh silent                                                                                                                                                                                                                                           |
| F-W6-AP-3                                      | 6             | 🟢      | C 2.3                   | TODO        | Error retry no distinct state                                                                                                                                                                                                                                    |
| F-W6-AP-4                                      | 6             | 🟢      | C 2.3                   | TODO        | Inline/overlay/full-page not standardised                                                                                                                                                                                                                        |
| F-W6-V-1                                       | 6             | 🟠      | C 7.2                   | TODO        | DM-1 reconfirmed + all live pills lack freshness                                                                                                                                                                                                                 |
| F-W6-V-2                                       | 6             | 🟡      | C 7.2                   | TODO        | Backfill doesn't disable LIVE                                                                                                                                                                                                                                    |
| F-W6-V-3                                       | 6             | 🟢      | C 7.2                   | TODO        | Latest-ledger polling works (informational)                                                                                                                                                                                                                      |
| F-W6-AK-1                                      | 6             | 🟡      | C 7.1 / C 11.2          | RESOLVED    | Hex → DS tokens (`0139a8a3`): AssetIcon `#724311`/`#fffcc2`→`colorsLight.primary`, ContractInterface/Chip/Switch→`scales.*`. `06ab34cc` regression reverted; survived merge (verified zero hex in AssetIcon)                                                     |
| F-W6-AK-2                                      | 6             | 🟢      | C 7.1 / C 11.3          | TODO        | Z-index raw 0/1 no scale — REGRESSED by `06ab34cc`: shell adds raw `zIndex: 2` (AppShell/TopNav/SecondaryNav/Footer). See F-DP-3 / card 11.3                                                                                                                     |
| F-W6-AK-3                                      | 6             | ✓       | —                       | RESOLVED    | Spacing scale consistent (baseline)                                                                                                                                                                                                                              |
| F-W6-AK-4                                      | 6             | 🟢      | —                       | DEFER-M2    | Border-radius/shadow audit deferred                                                                                                                                                                                                                              |
| F-W6-AK-5                                      | 6             | ✓       | —                       | RESOLVED    | CSS approach single (baseline)                                                                                                                                                                                                                                   |
| F-W6-AK-6                                      | 6             | 🟢      | C 7.1                   | TODO        | Theme tokens pervasive; tiny leakage (recap)                                                                                                                                                                                                                     |
| F-W6-F-1                                       | 6             | 🟡      | C 7.5                   | TODO        | NFT detail no h2/h3                                                                                                                                                                                                                                              |
| F-W6-F-2                                       | 6             | 🟡      | C 7.4                   | DONE        | Filter slots lack accessible names — STALE: already had `aria-label`+`placeholder` at `06ab34cc^` (pre-merge); NOT a design_parity closure. Re-verify on develop then archive                                                                                    |
| F-W6-F-3                                       | 6             | 🟢      | —                       | RESOLVED    | First Tab focus visible (baseline)                                                                                                                                                                                                                               |
| F-W6-F-4                                       | 6             | 🟢      | C 7.4                   | TODO        | Header search lacks aria-label/id — only possibly-open residual of card 7.4 (filter a11y stale-fixed); confirm in re-verify                                                                                                                                      |
| F-W6-F-5                                       | 6             | 🟢      | —                       | RESOLVED    | Copy buttons aria-label correct (baseline)                                                                                                                                                                                                                       |
| F-W6-F-6                                       | 6             | 🟢      | C 8.1                   | DEFER-M2    | Lighthouse a11y audit not run                                                                                                                                                                                                                                    |
| F-W6-F-7                                       | 6             | 🟢      | C 7.8                   | TODO        | Reduced-motion not verified                                                                                                                                                                                                                                      |
| F-W6-F-8                                       | 6             | 🟢      | C 7.8                   | TODO        | No keyboard trap test on modals                                                                                                                                                                                                                                  |
| F-W6-CH-1                                      | 6             | 🟡      | C 7.1                   | TODO        | Status badges color+text, no shape icon — NOT closed by `06ab34cc` (no checkmark/X icon added)                                                                                                                                                                   |
| F-W6-CH-2                                      | 6             | 🟢      | C 7.1                   | PARTIAL     | Operation type chips text-only (informational) — design_parity `06ab34cc` adds NEW Classic/SAC + protocol_version chips (tangential, not op-type-on-tx grouping)                                                                                                 |
| F-W6-RESPONSIVE-1                              | 6             | 🟠      | C 8.3                   | RESOLVED    | design_parity 06ab34cc + live re-verify 2026-05-28: 41/42 no doc-scroll, 802px root cause gone                                                                                                                                                                   |
| F-W6-RESPONSIVE-2                              | 6             | 🟡      | C 8.3                   | RESOLVED    | tables in overflowX:auto; table→card transform = separate optional enhancement                                                                                                                                                                                   |
| F-W6-RESPONSIVE-3                              | 6             | 🟠      | C 11.5                  | TODO        | user decision 2026-05-28: REQUIRE hamburger <768px; scroll-nav alt rejected → card 11.5                                                                                                                                                                          |
| F-W6-RESPONSIVE-4                              | 6             | 🟠      | C 11.6                  | TODO        | still failing live; 105/106 elements <44px @375 → card 11.6                                                                                                                                                                                                      |
| F-W6-RESPONSIVE-5                              | 6             | 🟡      | C 11.7                  | TODO        | search page overflow <660px, category card 628px intrinsic; newly-surfaced live 2026-05-28                                                                                                                                                                       |
| F-W6-NOTFOUND-1                                | 6             | 🟡      | C 5.1                   | TODO        | NotFound missing h1 on 4 of 5 detail                                                                                                                                                                                                                             |
| F-W6-NOTFOUND-2                                | 6             | 🟡      | C 5.3                   | TODO        | Sub-section queries fire on parent 404                                                                                                                                                                                                                           |
| F-W6-E0-1                                      | 6             | 🟠      | C 1.1                   | TODO        | Footer dead spans (recap)                                                                                                                                                                                                                                        |
| F-W6-E0-2                                      | 6             | 🟠      | C 7.2                   | TODO        | Footer hardcoded operational (recap)                                                                                                                                                                                                                             |
| F-W6-E0-3                                      | 6             | 🟡      | C 11.5                  | TODO        | No hamburger at mobile — user decision 2026-05-28: REQUIRE hamburger <768px (scroll-nav alt rejected); → card 11.5 (see F-W6-RESPONSIVE-3)                                                                                                                       |
| F-W6-E0-4                                      | 6             | 🟡      | C 7.1                   | TODO        | Header search placeholder 4 vs hint 5                                                                                                                                                                                                                            |
| F-W6-E0-5                                      | 6             | 🟢      | C 7.6                   | TODO        | Header polling duplicates home                                                                                                                                                                                                                                   |
| F-W6-E1-1                                      | 6             | 🟡      | C 7.2                   | TODO        | LIVE badge always on (recap)                                                                                                                                                                                                                                     |
| F-W6-E1-2                                      | 6             | 🟢      | C 7.1                   | TODO        | Hero+header search visually identical                                                                                                                                                                                                                            |
| F-W6-E1-3                                      | 6             | 🟢      | C 7.6                   | TODO        | Home stats strip duplicated (informational)                                                                                                                                                                                                                      |
| F-W6-E1-4                                      | 6             | 🟡      | C 5.4                   | TODO        | Home ledger hash not a link                                                                                                                                                                                                                                      |
| F-W6-E2-1                                      | 6             | 🟢      | C 7.1                   | TODO        | "Transactions list" vs nav "Transactions"                                                                                                                                                                                                                        |
| F-W6-E2-2                                      | 6             | 🟢      | C 7.1                   | TODO        | "All operations type" typo — NOT closed by `06ab34cc` (TransactionFilters.tsx unchanged)                                                                                                                                                                         |
| F-W6-E3-1                                      | 6             | 🟢      | C 7.1                   | TODO        | Memo "—" semantic improvement                                                                                                                                                                                                                                    |
| F-W6-E3-2                                      | 6             | 🟢      | C 7.1                   | TODO        | Normal/Advanced tabs no description                                                                                                                                                                                                                              |
| F-W6-E3-3                                      | 6             | 🟡      | C 5.1 / C 8.3           | PARTIAL     | Page horiz scroll mobile (covered by responsive) — design_parity `06ab34cc` removed 802px root cause (code-verified); live re-verify pending                                                                                                                     |
| F-W6-E5-1                                      | 6             | 🟢      | C 7.1                   | TODO        | Prev/Next ledger no disabled at boundary                                                                                                                                                                                                                         |
| F-W6-E6-1                                      | 6             | 🟡      | C 5.3                   | TODO        | Sub-section queries fire on 404 (account)                                                                                                                                                                                                                        |
| F-W6-E6-2                                      | 6             | 🟢      | C 5.1                   | TODO        | NotFound no h1 (account)                                                                                                                                                                                                                                         |
| F-W6-E7-1                                      | 6             | 🟡      | C 7.4                   | DONE        | Two unlabeled filter slots /assets — STALE: aria-label+placeholder present at `06ab34cc^` (pre-merge); re-verify then archive                                                                                                                                    |
| F-W6-E7-2                                      | 6             | 🟢      | C 7.1                   | PARTIAL     | Asset icon "?" fallback — design_parity `06ab34cc`: AssetIcon now color-coded by kind + 2-line header; "?" fallback unchanged                                                                                                                                    |
| F-W6-E7-3                                      | 6             | 🟢      | C 7.1                   | TODO        | Asset detail link uses composite ID for SAC                                                                                                                                                                                                                      |
| F-W6-E8-1                                      | 6             | 🟢      | C 7.1                   | PARTIAL     | Asset Metadata sparse — design_parity `06ab34cc` adds Domain row (home_page hostname); still no full SEP-1 TOML                                                                                                                                                  |
| F-W6-E8-2                                      | 6             | 🟢      | C 7.1                   | TODO        | Holder count not linkable                                                                                                                                                                                                                                        |
| F-W6-E9-1                                      | 6             | 🟡      | C 5.3                   | TODO        | Sub-section queries fire on 404 (contract)                                                                                                                                                                                                                       |
| F-W6-E9-2                                      | 6             | 🟢      | C 7.1                   | TODO        | Invocations+Events no empty-state message                                                                                                                                                                                                                        |
| F-W6-E9-3                                      | 6             | 🟡      | C 5.1                   | TODO        | NotFound h1 inconsistent (contract)                                                                                                                                                                                                                              |
| F-W6-E10-1                                     | 6             | 🟡      | C 7.4                   | DONE        | Four unlabeled filter slots /nfts — STALE: aria-label+placeholder present at `06ab34cc^` (pre-merge); re-verify then archive                                                                                                                                     |
| F-W6-E10-2                                     | 6             | 🟢      | C 7.1                   | TODO        | NFT row token IDs inline text                                                                                                                                                                                                                                    |
| F-W6-E10-3                                     | 6             | 🟡      | C 5.4                   | TODO        | NFT row Contract ID plain text                                                                                                                                                                                                                                   |
| F-W6-E11-1                                     | 6             | 🟡      | C 7.5                   | TODO        | NFT detail no h2/h3                                                                                                                                                                                                                                              |
| F-W6-E11-2                                     | 6             | 🟢      | C 7.1                   | PARTIAL     | NFT Traits "Metadata unavailable" no guidance — design*parity `06ab34cc`: NFT \_media* empty-state improved; Traits guidance NOT improved                                                                                                                        |
| F-W6-E11-3                                     | 6             | 🟡      | C 5.4                   | TODO        | NFT Contract ID in Details plain text                                                                                                                                                                                                                            |
| F-W6-E12-1                                     | 6             | 🟡      | C 7.1                   | TODO        | Pool ID truncation twice per row                                                                                                                                                                                                                                 |
| F-W6-E12-2                                     | 6             | 🟢      | C 7.1                   | TODO        | "Any TVL" filter looks like loading                                                                                                                                                                                                                              |
| F-W6-E13-1                                     | 6             | 🟠      | C 7.3                   | TODO        | Pool participants share % full precision                                                                                                                                                                                                                         |
| F-W6-E13-2                                     | 6             | 🟢      | C 5.1                   | TODO        | Pool NotFound no h1                                                                                                                                                                                                                                              |
| F-W6-E13-3                                     | 6             | 🟢      | C 7.1                   | TODO        | Pool tx operation type plain text — UNCHANGED by `06ab34cc` (LP-detail recent-tx + home op-type not in diff)                                                                                                                                                     |
| F-W6-E14-1                                     | 6             | 🟢      | C 7.1                   | TODO        | Empty-state hint at ?q= no examples                                                                                                                                                                                                                              |
| F-W6-E14-2                                     | 6             | 🟢      | C 7.1                   | TODO        | Search has two clear buttons                                                                                                                                                                                                                                     |
| F-W6-E14-3                                     | 6             | 🟢      | —                       | SKIP        | First Tab lands on header search (informational)                                                                                                                                                                                                                 |
| F-DP-1                                         | design_parity | 🟠      | C 11.1                  | RESOLVED    | NetworkToggle DELETED entirely (`e9122732`) — not hidden; survived merge (zero refs in code). R2 "still fake" note superseded. See card 11.1                                                                                                                     |
| F-DP-2                                         | design_parity | 🟠      | C 11.2                  | RESOLVED    | AssetIcon `#724311`/`#fffcc2`→`colorsLight.primary[900]/[100]` (`0139a8a3`); survived merge (zero hex). R2 "regression persists" note superseded. See card 11.2                                                                                                  |
| F-DP-3                                         | design_parity | 🟠      | C 11.3                  | TODO        | Raw `zIndex: 2` added across shell (AppShell/TopNav/SecondaryNav/Footer) — regresses F-AK-2. Introduced by `06ab34cc`. Move to z-index scale                                                                                                                     |
| F-DP-4                                         | design_parity | 🟠      | C 11.4                  | TODO        | OperationFlowTree collapse/expand removed (now flat w/ dashed connectors) — verify vs Figma; restore if regression. Introduced by `06ab34cc`                                                                                                                     |
| Z-1 Spot 5                                     | 5             | 🟢      | C 6.3                   | TODO        | Op-type enum hand-typed (cross-cite F-Z-2)                                                                                                                                                                                                                       |
| Z-1 Spot 1                                     | 5             | A       | C 8.4                   | TODO        | Error envelope flatten (cross-cite F-AF-1)                                                                                                                                                                                                                       |
| 0061 #4                                        | arch          | 🟢      | C 8.7                   | TODO        | Sort caret middle-ground sign-off                                                                                                                                                                                                                                |
| 0065 #5                                        | arch          | 🟡      | C 6.4                   | TODO        | Interval labels spec drift                                                                                                                                                                                                                                       |
| 0073 #5                                        | arch          | 🟡      | C 6.3                   | TODO        | Balances SAC vs Classic distinction (backend)                                                                                                                                                                                                                    |
| 0075 #6                                        | arch          | 🟡      | C 6.3                   | TODO        | interface_metadata hand-typed                                                                                                                                                                                                                                    |
| 0077 #9                                        | arch          | ✓       | —                       | RESOLVED    | Pool-id strkey 60 LOC justified                                                                                                                                                                                                                                  |
| 0077 #12 #13                                   | arch          | ✓       | —                       | RESOLVED    | assetLegLabel/classifyLpTx hard-fail justified                                                                                                                                                                                                                   |
| 0238 #5                                        | arch          | 🟡      | C 6.4                   | TODO        | cursorParam multi-cursor ADR gap                                                                                                                                                                                                                                 |
| 0251 B1                                        | arch          | 🟢      | C 10.3                  | TODO        | linked=false fix-by-hide root cause                                                                                                                                                                                                                              |
| 0059 Future Work (live stats)                  | arch          | —       | —                       | RESOLVED    | Wired via 0066 (TopNav still shows MOCK_STATS — re-verify in C 7.6 / 8.6)                                                                                                                                                                                        |
| 0059 Future Work (responsive nav)              | arch          | —       | C 8.3                   | TODO        | Hamburger menu                                                                                                                                                                                                                                                   |
| 0061 FW (libs/ui vitest)                       | arch          | —       | C 8.1                   | TODO        | 0226 promote                                                                                                                                                                                                                                                     |
| 0062 FW (validators → libs/domain)             | arch          | —       | C 6.2                   | TODO        | Spawn                                                                                                                                                                                                                                                            |
| 0062 FW (IdentifierDisplay router Link audit)  | arch          | —       | C 6.2                   | TODO        | Spawn                                                                                                                                                                                                                                                            |
| 0067 FW (route param validation per page)      | arch          | —       | C 6.2                   | TODO        | Partly absorbed by 0251                                                                                                                                                                                                                                          |
| 0068 FW (table sorting)                        | arch          | —       | C 6.2                   | TODO        | Gated on backend sort param                                                                                                                                                                                                                                      |
| 0068 FW (populated-data diff)                  | arch          | —       | —                       | RESOLVED    | Absorbed into 0251/0257                                                                                                                                                                                                                                          |
| 0069 FW (libs/ui error/empty divergence)       | arch          | —       | C 6.2                   | TODO        | Spawn                                                                                                                                                                                                                                                            |
| 0069 FW (operation pill colour confirm)        | arch          | —       | C 8.7                   | TODO        | Designer sign-off                                                                                                                                                                                                                                                |
| 0069 FW (OpenAPI op_type enum backend)         | arch          | —       | C 6.3                   | TODO        | Backend task                                                                                                                                                                                                                                                     |
| 0072 FW (hoist Button + formatFee timestamp)   | arch          | —       | C 2.1 / C 2.2           | TODO        | Covered by format/folder cards                                                                                                                                                                                                                                   |
| 0072 FW (URL-synced cursor)                    | arch          | —       | —                       | RESOLVED    | 0238                                                                                                                                                                                                                                                             |
| 0075 FW (contracts list page)                  | arch          | —       | C 1.3                   | TODO        | Launch blocker                                                                                                                                                                                                                                                   |
| 0075 FW (events count for tab pill)            | arch          | —       | C 6.2                   | TODO        | Backend task                                                                                                                                                                                                                                                     |
| 0075 FW (wasm_interface_metadata JSONB doc)    | arch          | —       | C 6.3                   | TODO        | Backend task                                                                                                                                                                                                                                                     |
| 0075 FW (SAC SEP-41 stub)                      | arch          | —       | C 6.2                   | TODO        | Spawn                                                                                                                                                                                                                                                            |
| 0076 FW (NFT trait rarity)                     | arch          | —       | —                       | RESOLVED    | 0229 spawned                                                                                                                                                                                                                                                     |
| 0077 FW (Tx Amount column on PoolTransactions) | arch          | —       | C 6.2                   | TODO        | Gated on 0247                                                                                                                                                                                                                                                    |
| 0077 FW (chart series wiring)                  | arch          | —       | C 10.1                  | TODO        | Gated on 0199                                                                                                                                                                                                                                                    |
| 0077 FW (per-leg icon_url backend)             | arch          | —       | C 6.2                   | TODO        | Spawn                                                                                                                                                                                                                                                            |
| 0077 FW (Playwright CLI for LP pages)          | arch          | —       | C 8.1                   | TODO        | Gated on 0226                                                                                                                                                                                                                                                    |
| 0077 FW (LP senior-eye 6 items)                | arch          | —       | C 6.2                   | TODO        | Bulk spawn batch                                                                                                                                                                                                                                                 |
| 0238 FW (backend prev_cursor)                  | arch          | —       | —                       | RESOLVED    | 0254                                                                                                                                                                                                                                                             |
| 0238 FW (unit tests useCursorPagination)       | arch          | —       | C 8.1                   | TODO        | Gated on 0226                                                                                                                                                                                                                                                    |
| 0238 FW (Playwright smoke 11 pages)            | arch          | —       | C 8.1                   | TODO        | Gated on 0226                                                                                                                                                                                                                                                    |
| 0238 FW (ADR multi-cursor)                     | arch          | —       | C 6.4                   | TODO        | Cross-cite                                                                                                                                                                                                                                                       |
| 0251 FW (ScVal decoder Contract Events)        | arch          | —       | C 6.2                   | TODO        | Spawn                                                                                                                                                                                                                                                            |
| 0251 FW (network runtime toggle)               | arch          | —       | C 6.2                   | TODO        | Spawn (post-launch)                                                                                                                                                                                                                                              |
| 0251 FW (Searchable Autocomplete ops)          | arch          | —       | C 6.2                   | TODO        | Spawn                                                                                                                                                                                                                                                            |
| 0251 FW (B4 fake-XLM design redo)              | arch          | —       | C 6.2                   | TODO        | Spawn                                                                                                                                                                                                                                                            |
| Out of scope O                                 | rdme          | —       | C 8.1                   | TODO        | testing baseline (= C 8.1)                                                                                                                                                                                                                                       |
| Out of scope N                                 | rdme          | —       | C 9.1                   | TODO        | i18n                                                                                                                                                                                                                                                             |
| Out of scope AJ                                | rdme          | —       | C 9.1                   | TODO        | Asset optimization (covered partly by C 4.1)                                                                                                                                                                                                                     |
| Out of scope AT                                | rdme          | —       | C 9.1                   | TODO        | Animation polish                                                                                                                                                                                                                                                 |
| Out of scope S                                 | rdme          | —       | C 9.1                   | TODO        | Browser compat matrix                                                                                                                                                                                                                                            |
| Out of scope T                                 | rdme          | —       | C 9.1                   | TODO        | Production parity                                                                                                                                                                                                                                                |
| Out of scope BR                                | rdme          | —       | C 9.1                   | TODO        | OG / Twitter cards                                                                                                                                                                                                                                               |
| Out of scope BM                                | rdme          | —       | C 9.1                   | TODO        | Memory leaks research                                                                                                                                                                                                                                            |
| Out of scope BJ                                | rdme          | —       | C 9.1                   | TODO        | WebSocket / SSE                                                                                                                                                                                                                                                  |
| Out of scope BV                                | rdme          | —       | C 9.1                   | TODO        | PWA                                                                                                                                                                                                                                                              |
| Out of scope BZ                                | rdme          | —       | C 9.1                   | TODO        | GDPR                                                                                                                                                                                                                                                             |
| Out of scope CE                                | rdme          | —       | C 9.1                   | TODO        | Command palette                                                                                                                                                                                                                                                  |
| Out of scope CF                                | rdme          | —       | C 9.1                   | TODO        | CSV/JSON export                                                                                                                                                                                                                                                  |
| Out of scope BO                                | rdme          | —       | —                       | SKIP        | Session replay (skip per user)                                                                                                                                                                                                                                   |
| Muxed M→G redirect                             | post-Gate-B   | —       | —                       | SKIP        | No ecosystem precedent                                                                                                                                                                                                                                           |
| Asset code-issuer composite redirect           | post-Gate-B   | —       | —                       | SKIP        | No ecosystem precedent                                                                                                                                                                                                                                           |
| SearchResponse::Redirect refactor              | post-Gate-B   | —       | —                       | RESOLVED    | Shipped by 0271 `5d7484b1` (FE owns singleton; wire collapsed to Results)                                                                                                                                                                                        |
| F-EX-3 PoolKpiStrip (extends F-K-2)            | 5 sweep       | 🟠      | —                       | RESOLVED    | a5f15166 (0263)                                                                                                                                                                                                                                                  |
| F-EX-4 PoolsTable reserves (extends F-K-2)     | 5 sweep       | 🟠      | —                       | RESOLVED    | a5f15166 (0263)                                                                                                                                                                                                                                                  |
| Issues Encountered worktree gotchas wiki       | arch          | —       | C 6.4                   | TODO        | Spawn DOCS wiki entry                                                                                                                                                                                                                                            |
| NFT search-404 regression (0264 carry-over)    | 0270          | 🟠      | —                       | RESOLVED    | 6421d3d7 + 69d9f529                                                                                                                                                                                                                                              |

(Appendix row count tracked above — see report. +4 design_parity regression rows F-DP-1..F-DP-4 appended 2026-05-27 per design-parity-impact-2026-05-27.md.)

(design_parity ROUND 2 annotation pass 2026-05-29 per design-parity-impact-2026-05-29.md / PR #224 / merge `35ac27c0`: no new rows added — no new regressions. Annotated in place: cards 1.3 / 2.2 / 4.1 / 7.3 / 11.1 / 11.2 / 11.4 / 11.5 / 11.6 / 11.7; appendix rows F-A-5, F-P-1, F-W6-E13-1, F-DP-1/2/4, F-W6-RESPONSIVE-3/4/5. Only flip: `/accounts` sub-item DONE within card 1.3 (`fce0d666`); card 1.3 stays PARTIAL — `/contracts` still PageStub. Card 7.3 stays TODO — share-% R2 fix is ILLUSORY (formatAmount minDecimals ≠ rounding). 5 R2 live-re-verify items added to Pending-live-verification block.)

(LIVE re-verify pass 2026-05-29 per design-parity-impact-2026-05-29.md §Live re-verify 2026-05-29 — live Playwright, R1+R2 merged, viewports 1280+375. Status flips applied: **card 11.7 / F-W6-RESPONSIVE-5 → RESOLVED** (page overflow GONE live, scrollWidth 364 ≤ 375; residual per-card reflow = optional NICE, same treatment as RESPONSIVE-2); **card 1.3 `/accounts` sub-item → DONE live-verified** (drop "pending re-verify"; card 1.3 OVERALL stays PARTIAL — `/contracts` confirmed live stub). Hardened-but-STILL-TODO: 7.3/F-W6-E13-1 (share-% ILLUSORY CONFIRMED LIVE `33.33…%`), 11.1/F-DP-1 (NetworkToggle VERIFIED-FAKE), 5.1/F-E-3+F-W6-NOTFOUND-1 (catch-all 404 NO main AND NO h1, live-confirmed), 11.4/F-DP-4 (flat confirmed but nested verify BLOCKED — 0 soroban/multi-op txs in local data). Pending-live-verification checklist items marked VERIFIED. No new regressions; desktop sweep 9 routes clean.)

## End of queue

When all `TODO` cards are `DONE`, this file represents the closed-state of audit 0257. The single elastic task `0XXX_FEATURE_audit-0257-closing` archives with reference to this queue.
