# AH — File / folder structure (Wave 5 1.3)

**Wave:** 5 (Tier 4 subjective)
**Stance:** senior fresh-eye; "is this file in the right place per project convention?"
**Date:** 2026-05-25
**Baseline SHA:** `81928602`.

## Per-check table

| #     | Check                                                            | Verdict | Evidence                                                                                                                                                                                                                                                                                                                                                                                                                                  | Severity   | Class |
| ----- | ---------------------------------------------------------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- | ----- |
| AH-1  | Each file in sensible location                                   | ⚠       | See F-AH-1 / F-AH-5 — 1 dead orphan, 1 single-file folder                                                                                                                                                                                                                                                                                                                                                                                 | 🟡 / 🟢    | D / C |
| AH-2  | Folder name matches concept                                      | ⚠       | Sample 5: `detail/` (1-file utility), `pool-detail/`, `transaction-detail/`, `nft-detail/`, `liquidity-pools/`, `pool-detail/` — see F-AH-2 (list-vs-detail folder split inconsistency)                                                                                                                                                                                                                                                   | 🟡         | C     |
| AH-3  | Shared utils in `libs/`, feature-specific in `web/src/pages/`    | ⚠       | See F-AH-3 — `web/src/pages/detail/SectionCard.tsx` is used by 16+ files but lives under `web/src/pages/detail/` (not `libs/ui`). Cross-cite Wave 4 F-U-1                                                                                                                                                                                                                                                                                 | 🟡         | C     |
| AH-4  | No "lonely" files                                                | ⚠       | `web/src/pages/PageStub.tsx` is orphan (zero importers, see below); `web/src/utils/poolIdStrkey.ts` is single-file folder (cross-cite Wave 4 F-X-5)                                                                                                                                                                                                                                                                                       | 🟡 / 🟢    | D     |
| AH-5  | Filename matches exported symbol (PascalCase.tsx / camelCase.ts) | ✓       | Sample 10 (LedgersTable.tsx → `LedgersTable`, useCursorPagination.ts → `useCursorPagination`, etc.) — all match. No mismatches found in sampled batch                                                                                                                                                                                                                                                                                     | —          | —     |
| AH-6  | List/detail folder symmetry across features                      | ✗       | See F-AH-2 — asymmetric splits: tx/pool/nft have separate list/detail folders, contracts has single folder, ledgers has single folder, transactions has single folder; rationale = "list = single feature folder; detail = separate folder if multi-section". Convention readable but not uniform                                                                                                                                         | 🟡         | C     |
| AH-7  | `index.ts` barrels consistent                                    | ✓       | 14 barrel files; uniform within each top-level lib boundary. `libs/ui/src/{theme,states,states/empty,states/errors,states/skeletons,timestamps,layout,identifiers,table,visualization}/index.ts` + main `libs/ui/src/index.ts` re-exports. `web/src/api/index.ts` + `web/src/api/hooks/index.ts`. `web/src/pages/**` has **no** barrels (page modules consumed directly by router) — intentional, matches React Router code-split pattern | —          | —     |
| AH-8  | Tests next to code or in `__tests__/`                            | N/A     | `find web/src libs/ui/src -name '*.test.*' -o -name '*.spec.*'` → **0 hits.** No test files anywhere in `web/src` or `libs/ui/src`. Cross-cite Wave 1 P-findings: test coverage is the dropped scope `O` from 0257 task README, slated for spawn in Phase 3 (`XXXX_FEATURE_frontend-testing-baseline`)                                                                                                                                    | (deferred) | —     |
| AH-9  | Assets in `public/` vs imported                                  | partial | `web/public/` contains: `fonts/`, `rumblefish-logo.svg`. SVG used via `<img src="/rumblefish-logo.svg">`. **No favicon** (F-AE-1 known finding). Other assets (icons) come from `@mui/icons-material` imports — no per-asset commitment                                                                                                                                                                                                   | —          | —     |
| AH-10 | `libs/ui` / `libs/api-types` / `web/` boundaries clear           | ⚠       | Boundaries are clean **between packages** (cross-cite Wave 4 X-coupling F-X table). But: `web/src/pages/detail/SectionCard.tsx` is the chrome of every detail page yet lives in `web/` not `libs/ui` (F-AH-3)                                                                                                                                                                                                                             | 🟡         | C     |
| AH-11 | Naming `page` vs `view` vs `screen`                              | ✓       | `Page` is the universal suffix: 15 `*Page.tsx` files at `web/src/pages/` top. Zero `View` / `Screen` files. Router calls them `page()` factory function (`web/src/router/index.tsx:9`). Convention airtight                                                                                                                                                                                                                               | —          | —     |
| AH-12 | Dead orphan files                                                | ✗       | See F-AH-4 — `web/src/pages/PageStub.tsx` (35-line component) has zero importers (`grep -rln "PageStub" web/ libs/` → only the file itself)                                                                                                                                                                                                                                                                                               | 🟡         | D     |

## Findings

### F-AH-1 [Class D, Severity 🟡] — `web/src/pages/PageStub.tsx` is a dead orphan post-tx-detail merge

- **Location:** `web/src/pages/PageStub.tsx` (35 LOC)
- **Importers:** **0** (verified `grep -rln "PageStub" web/ libs/` returns only the file itself)
- **Why it's dead:** Was the placeholder for `TransactionDetailPage` before the FilipDz tx-detail merge (a2c1b205). Now `web/src/pages/TransactionDetailPage.tsx` is a 1-line re-export shim → real detail at `web/src/pages/transaction-detail/index.tsx`. PageStub is no longer wired anywhere.
- **Impact:** Dead code; not in any bundle path (Vite tree-shakes unused exports), but lives in source tree and accumulates noise.
- **Class:** D — defer Phase 3; bundle into "post-tx-detail cleanup" follow-up task.
- **Recommendation:** `mv web/src/pages/PageStub.tsx .trash/` (per project deletion policy).
- **Cross-cite:** Wave 1 archaeology A1 (now RESOLVED post-merge) — PageStub was the symptom of the stub baseline.

### F-AH-2 [Class C, Severity 🟡] — Folder structure asymmetry across feature areas

**Feature folder taxonomy:**

| Feature         | List folder                                                                                                               | Detail folder                                                                                                                       | List page file                                 | Detail page file                                  |
| --------------- | ------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- | ------------------------------------------------- |
| Transactions    | `web/src/pages/transactions/` (TransactionFilters, TransactionTime, TransactionsTable, cells, formatters, operationTypes) | `web/src/pages/transaction-detail/` (index.tsx + advanced/ + normal/ + sections/ + shared/, 16 files)                               | `TransactionsListPage.tsx`                     | `TransactionDetailPage.tsx` (shim)                |
| Ledgers         | `web/src/pages/ledgers/` (LedgerNav, LedgerSummary, LedgerTransactions, LedgersTable)                                     | (none — uses `ledgers/`)                                                                                                            | `LedgersListPage.tsx`                          | `LedgerDetailPage.tsx` (composes from `ledgers/`) |
| Accounts        | (none — single page)                                                                                                      | `web/src/pages/accounts/` (AccountBalances, AccountSummary, AccountTransactions)                                                    | (no list page)                                 | `AccountDetailPage.tsx`                           |
| Assets          | `web/src/pages/assets/` (AssetFilters, AssetIcon, AssetMetadata, AssetSummary, AssetTransactions, AssetsTable, assetType) | (none — uses `assets/`)                                                                                                             | `AssetsListPage.tsx`                           | `AssetDetailPage.tsx`                             |
| Contracts       | (none — no list page!)                                                                                                    | `web/src/pages/contracts/` (ContractEvents, ContractInvocations, ContractInterface, ContractSummary, interfaceMetadata)             | (no list page — known gap, Wave 1 archaeology) | `ContractDetailPage.tsx`                          |
| NFTs            | `web/src/pages/nfts/` (NftFilters, NftNameCell, NftsTable)                                                                | `web/src/pages/nft-detail/` (NftEventBadge, NftMediaPreview, NftMetadata, NftSummary, NftTransfers)                                 | `NftsListPage.tsx`                             | `NftDetailPage.tsx`                               |
| Liquidity Pools | `web/src/pages/liquidity-pools/` (AssetAvatar, FeePill, PoolsFilterBar, PoolsTable, assetColor)                           | `web/src/pages/pool-detail/` (PoolCharts, PoolDetailHeader, PoolKpiStrip, PoolParticipants, PoolSummary, PoolTransactions, helpers) | `LiquidityPoolsListPage.tsx`                   | `LiquidityPoolDetailPage.tsx`                     |
| Home            | `web/src/pages/home/` (9 files — Hero, ChainOverview, LatestTransactions, etc.)                                           | n/a                                                                                                                                 | `HomePage.tsx`                                 | —                                                 |

**Patterns:**

1. **Two-folder split (list folder + detail folder)** when detail is multi-section: transaction-detail, nft-detail, pool-detail, liquidity-pools (list-side). 4 features.
2. **One-folder shared** when detail composes from list-area components: ledgers, assets. 2 features.
3. **Detail-only folder** when there's no list page: accounts (`accounts/`), contracts (`contracts/`). 2 features.
4. **Home is its own kind:** all sections in one folder. 1 case.

**Subjective:** the split rule **is** consistent ("separate detail folder iff detail is multi-section with no list-side reuse"). But it requires a reader to internalize that rule. The contracts gap (no list page) makes "contracts/" feel detail-folder-like which is structurally confusing.

**Recommendation Phase 3:**

- Hoist contracts list page (Wave 1 archaeology launch blocker) — fixes the contracts gap by giving it a list-side counterpart.
- Document convention in `lore/3-wiki/frontend-conventions.md`: "When detail composes from list-area components, share folder; otherwise split."
- **Class:** C (visual / not behaviour) — defer Gate B / Phase 3.

### F-AH-3 [Class C, Severity 🟡] — `web/src/pages/detail/` is a logically-wrong home for `SectionCard`

- **Location:** `web/src/pages/detail/SectionCard.tsx` (and `PageBreadcrumb.tsx`, `SummaryRow.tsx`)
- **Consumers:** 16+ files across all detail pages (account, asset, contract, ledger, NFT, LP, tx-detail). Cross-cite Wave 4 F-U-1.
- **Wrong home rationale:**
  - `web/src/pages/detail/` suggests "stuff used on detail pages" — but it's used by **every** detail page, making it shared UI chrome.
  - The naming pattern across the project: shared UI chrome → `libs/ui/src/layout/` (TopNav, Footer, NavButton, SecondaryNav, SearchInput, PageGridBackdrop).
  - `SectionCard` is functionally identical to a layout primitive in `libs/ui/src/layout/`.
- **Recommendation:** Hoist `web/src/pages/detail/{SectionCard,PageBreadcrumb,SummaryRow}.tsx` → `libs/ui/src/layout/`. Update 16+ imports.
- **Class:** C (visual contract; promoting to libs/ui doesn't change DOM but makes the home truthful).
- **Cross-cite:** Wave 4 F-U-1 (already known) + Wave 4 F-X-2 (single-file folder concern).
- **Net:** F-AH-3 = the structural finding that motivates the F-U-1 refactor.

### F-AH-4 [Class C, Severity 🟢] — `web/src/utils/` is a single-file folder (1 util)

- **Location:** `web/src/utils/poolIdStrkey.ts` (only file in `web/src/utils/`)
- **Subjective:** "utils" as a single-file folder hints that future utils were anticipated but never materialized. Either:
  - Move `poolIdStrkey.ts` to `libs/ui` (per Wave 4 F-X-5 trade-off — bundle cost from `@stellar/stellar-sdk`'s `StrKey` already present), OR
  - Move to `web/src/pages/format.ts` companion location, OR
  - Keep and accept the single-file folder pattern as "domain of cross-page utilities".
- **Class:** C — Phase 3 decision (cross-cite Wave 4 F-X-5).

### F-AH-5 [Class C, Severity 🟢] — `web/src/pages/detail/` only has 3 files, and is the unfortunate home of shared chrome

- Already covered in F-AH-3 — folder is conceptually misnamed and should evaporate after hoist.
- **Recommendation:** delete folder once SectionCard / PageBreadcrumb / SummaryRow promoted to `libs/ui/src/layout/`.
- **Class:** C — Phase 3 with F-AH-3.

### F-AH-6 [Class D, Severity 🟢] — No tests collocated or in `__tests__/`

- **Evidence:** 0 `*.test.*` / `*.spec.*` files in `web/src/` + `libs/ui/src/`
- **Status:** Documented dropped scope `O` in 0257 task README. Spawn `XXXX_FEATURE_frontend-testing-baseline` in Phase 3 sub-phase 3.2.
- **Convention for spawned task:** prefer collocated `Component.test.tsx` next to `Component.tsx` (per task README 1.3 checklist preference) — gives juniors immediate co-location signal. Could also use `__tests__/` subfolder per feature; both are React community conventions, both defensible. Pick one in the spawned task body.
- **Class:** D (Phase 3 deferral known).

### F-AH-7 [Class C, Severity 🟢] — `web/src/search/` is a parallel top-level folder to `web/src/pages/`

- **Location:** `web/src/search/` (GlobalSearchBar, SearchResultRow, SearchResultsTabs, SearchResultsView, routeForHit, useDebounced, useSearchResults — 7 files)
- **Inconsistency:** `web/src/pages/SearchResultsPage.tsx` exists at page-level but uses `web/src/search/` for the body. Other features keep their sub-components inside `web/src/pages/<feature>/`.
- **Justification:** Search has cross-page surface — `GlobalSearchBar` is used in `libs/ui/src/layout/TopNav.tsx` (likely) — so it can't fully live under `web/src/pages/search/`.
- **Alternative:** Move `GlobalSearchBar` to `libs/ui/src/layout/` (where TopNav lives) + move rest into `web/src/pages/search/`. Then `web/src/search/` evaporates.
- **Verify:** Run `grep -rn 'GlobalSearchBar' web/ libs/` to confirm consumers.
- **Class:** C — Phase 3 structural cleanup.

### F-AH-8 [Class D, Severity 🟢] — `web/src/pages/cursorParams.ts` + `web/src/pages/format.ts` + `web/src/pages/url.ts` are page-root helpers

- **Location:** 3 .ts files at `web/src/pages/` root, alongside 15 `*Page.tsx` files.
- **Concern:** Helpers and pages mixed at one level. Pages are React entry points; helpers are utility libs. Mixing them violates "files per concept type at a level".
- **Recommendation:** Move helpers to `web/src/pages/_shared/{cursorParams,format,url}.ts` or hoist into `libs/ui/src/` if cross-package shareable. (`cursorParams.ts` likely couples to multi-cursor naming `cursor_p/_t/_e/_i` from 0238 / cross-cite Wave 1 A3.)
- **Subjective:** small cost today, increasing maintenance friction as page count grows.
- **Class:** D — Phase 3 batch with hoisting (F-AH-3, F-AH-7).

## Cross-cites

- F-AH-3 ↔ Wave 4 F-U-1 / F-X-2 (SectionCard hoist).
- F-AH-4 ↔ Wave 4 F-X-5 (`web/src/utils/` single-file).
- F-AH-1 (PageStub orphan) is **new finding** from Wave 5 — post-tx-detail-merge orphan.
- F-AH-7 (search/ parallel folder) is **new finding** from Wave 5.
- F-AH-8 (page-root helpers) is **new finding** from Wave 5.
- F-AH-6 (no tests) — cross-cite 0257 task README dropped scope `O`.

## Net 1.3 finding count

8 findings (3 new + 5 cross-cite): 0 🔴 / 0 🟠 / 4 🟡 / 4 🟢.

**Class breakdown:** C=5 / D=3.

**Subjective calls (Tier 4 flagging for user spot-check):**

1. **F-AH-2** — "asymmetric folder split is fine because the rule is consistent" is a senior judgment. User may prefer enforced uniformity (every feature gets list-folder + detail-folder regardless of multi-section need). **User decides.**
2. **F-AH-3** — hoisting SectionCard to `libs/ui/src/layout/` requires Phase 3 effort. User may rather accept the `web/src/pages/detail/` home and only normalize naming. **User decides.**
3. **F-AH-7** — `web/src/search/` is structural debt of unclear urgency. User decides priority.
4. **F-AH-8** — page-root helpers may be fine ("colocate with pages, away from libs/ui"). User decides.

## Top issues

1. **F-AH-1 (🟡 D)** — `PageStub.tsx` dead orphan, simple `mv .trash/` in Phase 3.
2. **F-AH-3 (🟡 C)** — `SectionCard` hoist, cross-cite Wave 4 F-U-1 ; consolidates 2 findings into 1 refactor PR.
3. **F-AH-7 (🟢 C)** — `web/src/search/` parallel folder cleanup.
