# design_parity Impact Analysis — branch `feat/design_parity` vs audit-action-queue

**Date:** 2026-05-27
**Analyst:** code inspection only (no live Playwright — see live-re-verify queue)
**Source commit:** `06ab34cc` "feat: design parity audit + responsive pass + dev mock expansion" (1 commit, 57 files, +1405 / -656)
**Merged into:** `research/0257_frontend-comprehensive-audit` (merge tip `62c988d4`)
**Maps against:** `audit-action-queue.md` (38 cards + appendix) + Wave 6 finding files

---

## TL;DR (return numbers)

- **Cards flipped to DONE:** 0 (no card is fully closed by this commit alone)
- **Cards flipped to PARTIAL / IN-PROGRESS:** 4 (cards 1.3, 8.3, plus partial touches to 7.1 and 6.4-adjacent F-AN-6)
- **Findings RESOLVED:** ~0 fully; ~6 PARTIAL (responsive root-cause + table overflow + nav scroll + contracts nav entry + asset metadata + tab-count pills)
- **Top 3 highest-value closures:** (1) 802px page-width root cause removed (responsive); (2) `/contracts` + `/accounts` nav entries added (F-A-5 Gap 1 launch-blocker downgraded); (3) horizontal-scroll on all tables (mobile table breakage mitigated)
- **Regressions / new debt introduced:** 4 (see §Regressions) — most notable: NetworkToggle is a non-functional affordance; 2 new hardcoded hex constants
- **Live-re-verify required before any responsive card → DONE:** yes (all 14 routes × 375/768 cells)

> **Caveat on this whole analysis:** design_parity is a "Figma parity + responsive" pass authored independently of the audit. It was NOT written to close audit cards. Several apparent closures are partial, and one Wave 6 finding it appears to "fix" (filter a11y) was already fixed before this commit (stale finding, see §Stale-findings).

---

## 1. Summary table — card → status change

Legend: status is the SUGGESTED new status for `audit-action-queue.md`. **Apply only after review.**

| Card                                      | Title                                                          | Old  | Suggested                                            | Evidence (design_parity)                                                                                                                                                                                                                                                                             |
| ----------------------------------------- | -------------------------------------------------------------- | ---- | ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.1                                       | Footer legal + external links                                  | TODO | **unchanged**                                        | Footer.tsx touched (responsive only). `RESOURCES` + `LEGAL` arrays STILL have no `href` → dead `<span>`. "All systems operational" still hardcoded. CA-1/CA-2/F-W6-E0-1 NOT closed.                                                                                                                  |
| 1.2                                       | Build SHA / version stamp                                      | TODO | **unchanged**                                        | No vite `define`, no SHA surfaced. Not touched.                                                                                                                                                                                                                                                      |
| 1.3                                       | Contracts list page + `/contracts` nav                         | TODO | **PARTIAL**                                          | `/contracts` + `/accounts` added to `NAV_LINKS` (routes.ts) AND as routes — but rendered via `<PageStub>` placeholder, NOT a real list. Nav-entry half of F-A-5 Gap 1 done; list-page half open.                                                                                                     |
| 2.1                                       | Format/truncate/debounce unification                           | TODO | **unchanged**                                        | Diff still calls inline `truncateMiddle` + `toLocaleString('en-US')`; no `libs/ui/src/format/` created. Duplications untouched.                                                                                                                                                                      |
| 2.2                                       | Folder rationalization (PageStub delete)                       | TODO | **unchanged + SCOPE CONFLICT**                       | design_parity REVIVES `PageStub` as the `/accounts` + `/contracts` stub. F-AH-1 ("PageStub dead orphan, delete") is now FALSE — PageStub has 2 live consumers. Card 2.2 must drop the "delete PageStub" line or gate it behind card 1.3 shipping real pages.                                         |
| 2.3                                       | EmptyState/LoadingState primitives                             | TODO | **unchanged**                                        | No `libs/ui/src/states/` skeleton primitives added. NFT media empty-state got bespoke iconography (local, not shared) — arguably mild anti-pattern vs the consolidation goal.                                                                                                                        |
| 3.1                                       | noUncheckedIndexedAccess                                       | TODO | **unchanged**                                        | No tsconfig change.                                                                                                                                                                                                                                                                                  |
| 3.2 / 3.3                                 | Branded IDs / assertNever                                      | TODO | **unchanged**                                        | Not touched.                                                                                                                                                                                                                                                                                         |
| 4.1                                       | Bundle / LP lazy / vendor split                                | TODO | **unchanged (slight neg)**                           | No manualChunks, no lazy LP chart. NEW asset `soroban-logo.webp` (2.9KB) + new `NetworkToggle` component add a little to main bundle. No visualizer.                                                                                                                                                 |
| 5.1                                       | 404 `<main>` landmark + NotFound h1                            | TODO | **needs re-check**                                   | AppShell `<main>` restructured (now wraps `<Outlet/>` inside relative Box). Catch-all 404 routing unchanged in this diff; NotFound h1 not touched. Re-verify F-E-3 landmark still holds after AppShell refactor.                                                                                     |
| 5.2 / 5.3 / 5.4                           | URL tab state / composite NotFound / cross-entity links        | TODO | **unchanged**                                        | Not touched. (E13-1 share%, sub-section query gating, NFT contract-id link all still open.)                                                                                                                                                                                                          |
| 6.4                                       | ADR + doc-sync sweep (incl. F-AN-6 "document Mainnet/Testnet") | TODO | **unchanged + complicated**                          | design_parity adds a Mainnet/Testnet UI toggle that is NON-FUNCTIONAL (see §Regressions). F-AN-6's "document single-environment config" is now harder: there's a visible toggle implying multi-network, but config is still single `VITE_API_BASE_URL`. Doc must now explain the visual-only toggle. |
| 7.1                                       | Wave 6 visual polish micro-batch                               | TODO | **PARTIAL**                                          | Several sub-findings touched (see §4). But several others explicitly NOT (typo F-W6-E2-2, hex constants F-AK-1 regressed, chips color groups). Net: partial.                                                                                                                                         |
| 7.2                                       | Live indicator freshness + health probe                        | TODO | **unchanged**                                        | LIVE pills + "All systems operational" still hardcoded (`<LiveIndicator/>` static; footer span static). No `useLiveStatus`. DM-1/F-W6-V-1 open.                                                                                                                                                      |
| 7.3                                       | Pool participants share % precision                            | TODO | **unchanged**                                        | `pool-detail/PoolParticipants.tsx` not in diff. F-W6-E13-1 open.                                                                                                                                                                                                                                     |
| 7.4                                       | Filter slot a11y                                               | TODO | **unchanged (already-fixed pre-merge — see §Stale)** | Filters already had `aria-label`+`placeholder` BEFORE this commit. design_parity only made them responsive. NOT a design_parity closure.                                                                                                                                                             |
| 7.5                                       | NFT detail heading hierarchy                                   | TODO | **unchanged**                                        | NftMetadata/NftMediaPreview touched, but section labels still not `component="h2"`. F-W6-E11-1 open.                                                                                                                                                                                                 |
| 7.6                                       | Header polling de-dup                                          | TODO | **unchanged**                                        | Not touched.                                                                                                                                                                                                                                                                                         |
| 7.7                                       | Route transition loading indicator                             | TODO | **unchanged**                                        | Not added.                                                                                                                                                                                                                                                                                           |
| 8.3                                       | Responsive redesign (mobile/tablet/hamburger)                  | TODO | **PARTIAL**                                          | Biggest impact. 802px root cause removed; tables wrap in overflow-x; nav scrolls horizontally; heroes stack; KPI strip 2×2. BUT no hamburger menu (nav scrolls instead); touch-target ≥44px NOT audited; no table→card transform. See §2.                                                            |
| All other cards (6.1–6.3, 8.x, 9.1, 10.x) | various                                                        | TODO | **unchanged**                                        | Out of design_parity's scope (docs/deps/backend/test/MUI bump).                                                                                                                                                                                                                                      |

---

## 2. Responsive findings (F-W6-RESPONSIVE-\*) — card 8.3

### Root-cause: the 802px fixed page width

**Old AppShell** had `<main>` with `maxWidth: grid.desktop.maxWidth` and the home page used `px: 10` on inner sections (`HomeHero`, `ChainOverview`, `LatestTransactions`, `LatestLedgers` all had `sx={{ px: 10 }}`). The Wave-6 matrix attributed `documentElement.scrollWidth = 802` to the shell above `<main>`.

**design_parity changes:**

- `AppShell.tsx`: `<main>` now uses responsive padding `px: { xs: grid.mobile.margin, md: grid.desktop.margin }`, `py: { xs: 2, md: 4 }`. No fixed min-width anywhere in AppShell (verified: no `minWidth` on the shell containers).
- `TopNav.tsx` / `SecondaryNav.tsx` / `Footer.tsx`: all switched from fixed `px: grid.desktop.margin` to responsive `px: { xs: grid.mobile.margin, md: ... }`.
- Home full-bleed inner sections (`HomeHero`, `ChainOverview`, `LatestLedgers`, `LatestTransactions`) **dropped the `px: 10`** that was forcing extra width.
- `HomeHero`: subtitle `whiteSpace` is `normal` at xs (was `noWrap`-equivalent nowrap forcing doc-level scroll); kept `nowrap` at md.
- **No remaining hardcoded fixed min-width found in AppShell or the touched layout components.** (grep on AppShell/TopNav/SecondaryNav/Footer: zero `minWidth: <px>` on shells; only `minWidth: 0` which is the _opposite_ — allows shrink.)

**Verdict F-W6-RESPONSIVE-1 (802px root cause): RESOLVED-IN-CODE / needs-live-verify.** The mechanism the matrix identified is removed. Cannot confirm `documentElement.scrollWidth === clientWidth` at 375/768 without a live run.

### Table breakage — ExplorerTable overflow

- `ExplorerTable.tsx`: `<TableContainer sx={{ overflowX: 'auto' }}>` added. Standalone tx-detail tables (SignaturesTable, EventsSection) got matching treatment per the commit message.
- This is horizontal-scroll-with-overflow, NOT the "table → card layout" transform that F-W6-RESPONSIVE-2 recommended.

**Verdict F-W6-RESPONSIVE-2 (no table→card transform): PARTIAL.** Tables no longer force _page-level_ overflow (they scroll within their own container), which mitigates the worst symptom. But the recommended card-layout transform at <600px is NOT implemented. Card 8.3's "table → card OR horizontal-scroll-with-shadow" — design_parity chose plain overflow-x (no shadow affordance). Acceptable mitigation; finding stays open for the card-layout goal.

### Hamburger nav

- `SecondaryNav.tsx`: nav row now `overflowX: { xs: 'auto', md: 'visible' }` with hidden scrollbar — nav **scrolls horizontally** on mobile. There is **NO hamburger button**, no `aria-label="Open menu"`, no collapse-to-menu.
- `TopNav.tsx`: stats strip hidden on xs (`display: { xs: 'none', md: 'flex' }`); NetworkToggle + search remain.

**Verdict F-W6-RESPONSIVE-3 (no hamburger) + F-W6-E0-3: UNTOUCHED (alternative chosen).** design*parity deliberately uses a horizontal-scroll nav instead of a hamburger. This is a design decision that \_contradicts* card 8.3's "Add hamburger menu at <768px" + 0059 Future Work. **User/designer decision needed:** accept scroll-nav as the answer (then close F-W6-RESPONSIVE-3 / 0059) OR still require hamburger.

### Touch targets

- No evidence of a 44px-minimum audit. `HomeLogo` height dropped 32→24 in SecondaryNav. Nav buttons unchanged in height.

**Verdict F-W6-RESPONSIVE-4 (touch targets <44px): UNTOUCHED.** Still open. Needs live measurement.

### Card 8.3 net verdict

**TODO → PARTIAL / IN-PROGRESS.** Root cause + table overflow + responsive padding + hero/KPI stacking done. Hamburger (substituted with scroll-nav, needs sign-off), touch targets, and table→card transform NOT done. Effort remaining shrinks from "3–5d" to ~1–2d (hamburger decision + touch-target audit + optional card layout).

---

## 3. Network toggle (F-AN-6) — card 6.4-adjacent

- New `libs/ui/src/layout/NetworkToggle.tsx` (124 lines): Mainnet/Testnet segmented control with `role="group"`, `aria-pressed`, per-network palette. Exported from `libs/ui` barrel.
- Wired AppShell → TopNav → NetworkToggle: `const [network, setNetwork] = useState<Network>(Network.MAINNET)` in AppShell; `onNetworkChange={setNetwork}`.
- **It is purely visual.** `web/src/api/config.ts` `apiBaseUrl` is a static module constant from `VITE_API_BASE_URL` — it does NOT read `network`. Query keys (`queryKeys.ts`) do NOT include network. No network context/provider. Switching the toggle changes a local `useState` that only flows back into the toggle's own rendering. No API base URL change, no refetch, no data difference.
- Additional wrinkle: `TopNav` is now hidden on the home route (`{!isHome && <TopNav .../>}`), so the NetworkToggle is **invisible on `/`** and only appears on non-home routes.

**Verdict F-AN-6: PARTIAL (UI-only) — and arguably a NET-NEGATIVE.** The original finding was "single-environment config, no toggle (acceptable, document it)". design*parity adds a toggle that \_looks* functional but does nothing — a misleading affordance. This is worse for users than no toggle. See §Regressions. Card 6.4's "document Mainnet/Testnet config" line now must also document that the toggle is decorative (or the toggle should be wired / hidden).

---

## 4. Figma fidelity / visual polish — card 7.1 + B-figma-fidelity

`B-figma-fidelity.md` was **BLOCKED** (no Figma URL in Wave 6). design*parity is exactly the Figma-compare pass B recommended — authored by a dev with Figma access. So it closes visual divergences B could only \_list*. Mapping its touches to card 7.1 sub-findings:

| 7.1 sub-finding                                            | design_parity effect                                                                                                                                                                                                                                         | Verdict                                                            |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------ |
| F-W6-E8-1 — Asset metadata sparse                          | AssetMetadata now adds a **Domain** row (derived from home_page hostname) paired with Homepage. AssetIcon colored by kind. Still does NOT render full SEP-1 TOML (description shown; no conditions/contact/org/validators).                                  | **PARTIAL**                                                        |
| F-W6-E7-2 — Asset icon "?" fallback                        | AssetIcon now color-coded by kind (native/classic/sac/default) + 2-line header. The "?" letter fallback logic is unchanged but visually richer.                                                                                                              | **PARTIAL** (cosmetic improvement, "?" still the no-code fallback) |
| F-W6-E11-2 — NFT Traits "Metadata unavailable" no guidance | NftMediaPreview empty-state got an icon chip + "No media available" + "There is no image" subtext (media side). Traits empty-state copy NOT improved with actionable guidance.                                                                               | **PARTIAL** (media empty-state improved; traits guidance not)      |
| F-W6-E13-3 — Recent tx op-type as plain text, no chip      | LP-detail recent-tx section NOT in diff. Home `LatestTransactionsTable` op-type unchanged (still plain).                                                                                                                                                     | **unchanged**                                                      |
| F-W6-E2-2 — "All operations type" typo                     | `TransactionFilters.tsx` line UNCHANGED — typo `<MenuItem value={ALL_OPERATIONS}>All operations type</MenuItem>` still present.                                                                                                                              | **unchanged**                                                      |
| F-W6-E12-1 — Pool ID truncation twice per row              | PoolsTable not in diff.                                                                                                                                                                                                                                      | **unchanged**                                                      |
| F-W6-E12-2 — "Any TVL" filter looks like loading           | PoolsFilterBar not in diff.                                                                                                                                                                                                                                  | **unchanged**                                                      |
| F-W6-CH-1 — Status badges no icon cue                      | No checkmark/X icon added to status badges.                                                                                                                                                                                                                  | **unchanged**                                                      |
| F-W6-CH-2 — Op-type chips text-only / semantic color       | ContractEvents keeps event-type color chips (blue/brown/grey); AssetsTable + AccountBalances NEW Classic/SAC chips; LedgersTable NEW protocol_version chip. These are NEW semantic chips but not the op-type-on-transactions grouping the finding asked for. | **PARTIAL / tangential**                                           |
| F-W6-AG-3 — non-GPU transitions                            | NetworkToggle, ExplorerTable sort caret, Tabs use `background-color`/`color`/`border-color` transitions (non-GPU). No move to transform/opacity. New components ADD more non-GPU transitions.                                                                | **unchanged / slight neg**                                         |
| F-W6-AG-4 — 150/200ms hover at edge                        | New transitions are 0.15s (150ms) — same edge value.                                                                                                                                                                                                         | **unchanged**                                                      |
| F-AK-1 / F-W6-AK-1 — hardcoded hex constants               | `ContractInterface.tsx` still has `TYPE_REF_COLOR = '#155dfc'`. AssetIcon ADDS new hardcoded `'#724311'` + `'#fffcc2'` (sac kind). **Net: 3 → 5 hardcoded hex.**                                                                                             | **REGRESSED**                                                      |
| F-AK-2 / F-W6-AK-2 — raw z-index 0/1                       | AppShell/TopNav/SecondaryNav/Footer now add raw `zIndex: 2` in several spots. More ad-hoc z-index, no scale.                                                                                                                                                 | **REGRESSED (mildly)**                                             |

**Bonus visual-parity work NOT tracked as a finding** (net-positive Figma fidelity, credit B-figma cross-ref table): Contract detail tab count badges (Tabs.tsx `active` pill styling + ContractDetailPage wires `recent_invocations`/`recent_unique_callers`); ledger-deployer breadcrumb on contract detail; return-type accent color in ContractInterface; AssetsTable Classic/SAC chips; LedgersTable protocol_version chip; NFT trait `rarity_percent` ("X% have this") OpenSea-style; new soroban-logo.webp navbar logo; ExplorerTable sort-caret redesign (circular badge — relevant to card 8.7 designer sign-off, see §Cross-card).

**Card 7.1 net verdict: TODO → PARTIAL.** A handful of sub-findings partially improved; several explicitly untouched; 2 sub-findings (hex constants, z-index) REGRESSED. Do NOT mark 7.1 DONE.

---

## 5. Other cards touched

- **Card 2.1 (formatters):** NOT touched — verified diff still has inline `truncateMiddle` + `toLocaleString('en-US')`.
- **Card 2.2 (PageStub delete):** SCOPE CONFLICT — PageStub revived as live stub (2 consumers). F-AH-1 must be re-worded; PageStub no longer deletable until card 1.3 real pages ship.
- **Card 4.1 (bundle):** slight negative — new webp asset + NetworkToggle component; no chunking/lazy work.
- **Card 1.1 (Footer):** Footer touched but legal/resources hrefs still absent → CA-1/CA-2 open. (Note: the footer "Explorer" middle column DID gain hrefs via `FOOTER_EXPLORER_LINKS` in AppShell — but that column was never a finding; the dead spans are RESOURCES + LEGAL, both still hrefless.)
- **Card 8.7 (sort-caret designer sign-off):** design*parity REWROTE the sort caret (removed MUI `TableSortLabel` + `UnfoldMore`; new `SortableHeader` with circular badge + rotating `KeyboardArrowDownIcon`). The "middle-ground" caret the audit flagged for sign-off is now a \_different* implementation. 8.7 / F-AB-4 now needs sign-off on the NEW caret, not the old one. Status stays TODO but the artifact changed.

---

## 6. Dev mock expansion

- Commit message claims `tools/dev-mock-api.mjs` gained assets/accounts/ledgers/contracts/NFTs endpoints.
- **The mock file is NOT in the merged diff** (not in the 57-file stat; `tools/` has no tracked mock; `git log -- tools/dev-mock-api.mjs` is empty; not gitignored by any matching rule found). It is a local/untracked dev artifact that did not land on the branch.
- **Audit relevance: none.** No committed test fixture changed. It does not affect any finding or the testing-baseline card 8.1 (still zero `*.test.*`).

---

## Cards that should flip STATUS in audit-action-queue.md (for review — DO NOT auto-apply)

1. **Card 1.3** TODO → **IN-PROGRESS / PARTIAL**. Add note: "`/contracts` + `/accounts` nav entries + stub routes landed in `06ab34cc` (design_parity). F-A-5 Gap 1 nav-link half DONE; real list page still TODO. PageStub now the stub renderer."
2. **Card 8.3** TODO → **IN-PROGRESS / PARTIAL**. Add note: "802px root cause + table overflow-x + responsive padding + hero/KPI stacking landed in `06ab34cc`. Remaining: hamburger (design_parity chose horizontal-scroll nav — needs designer sign-off), touch-target ≥44px audit, optional table→card transform. NEEDS LIVE RE-VERIFY of all 14×375/768 matrix cells before marking sub-findings DONE."
3. **Card 7.1** TODO → **IN-PROGRESS / PARTIAL**. Add note: "Partial visual parity from `06ab34cc` (asset metadata Domain row, NFT media empty-state, contract tab count pills, AssetIcon color-coding). NOT closed: F-W6-E2-2 typo, F-W6-CH-1 badge icon, op-type semantic colors. REGRESSED: F-AK-1 hex constants (3→5), F-AK-2 z-index."
4. **Card 2.2** — keep TODO but **edit scope**: remove/qualify the "Delete `web/src/pages/PageStub.tsx` dead orphan" line — F-AH-1 is now FALSE (PageStub revived with 2 consumers in `06ab34cc`). Gate PageStub deletion behind card 1.3 shipping real pages.
5. **Card 6.4** — keep TODO but **add scope**: F-AN-6 doc must now also cover the non-functional NetworkToggle (decorative Mainnet/Testnet control added in `06ab34cc`); decide wire-it / hide-it / document-as-decorative.
6. **Card 8.7** — keep TODO but **update artifact**: sort caret was rewritten in `06ab34cc`; designer sign-off now applies to the new circular-badge caret.

**Appendix STATUS column:** none should flip to RESOLVED. A few sub-findings could be annotated "PARTIAL via 06ab34cc (design_parity)": F-A-5 Gap 1 (nav half), F-W6-RESPONSIVE-1 (code), F-W6-RESPONSIVE-2 (overflow mitigation), F-W6-E8-1 (Domain row).

---

## Live-Playwright re-verify queue (REQUIRED before any DONE claim)

All of these were judged from code only; confirm in a live run at 375 + 768 viewports:

1. **F-W6-RESPONSIVE-1** — confirm `document.documentElement.scrollWidth === clientWidth` (no page-level horizontal scrollbar) on all 14 routes at 375 + 768. This is THE gating check for the responsive matrix flip.
2. **F-W6-RESPONSIVE-2** — confirm embedded/list tables scroll within their container and do NOT push page width on E1/E2/E3/E4/E5/E6/E7/E8/E10/E12/E13.
3. **F-W6-RESPONSIVE-4** — measure touch targets at 375 (nav, copy buttons, pagination prev/next) for ≥44px.
4. **F-E-3 (card 5.1)** — confirm catch-all 404 still inside `<main>` after AppShell `<main>` restructure.
5. **Home route** — confirm KPI 2×2 grid + hero wrap render without overflow at 375; confirm TopNav hidden-on-home doesn't break header search/network affordances expectations.
6. **SecondaryNav scroll-nav** — confirm horizontal-scroll nav is usable at 375 (and decide if it substitutes for hamburger).
7. **NetworkToggle** — confirm clicking Testnet visibly does nothing to data (to document the decorative behavior) and is invisible on `/`.

---

## Regressions / new visual debt introduced by design_parity

1. **NetworkToggle is a non-functional affordance (HIGH-attention).** A Mainnet/Testnet switch that looks interactive (`aria-pressed`, hover, palette) but does not change API base URL, query keys, or any data. Misleading to users; worse than F-AN-6's prior no-toggle baseline. Either wire it (network → apiBaseUrl + query key namespace) or hide it until wired. Also invisible on the home page (toggle only on non-home routes).
2. **Hardcoded hex constants increased 3 → 5.** AssetIcon `sac` kind adds `'#724311'` + `'#fffcc2'` inline; `ContractInterface` `TYPE_REF_COLOR='#155dfc'` retained. Directly regresses F-AK-1 / F-W6-AK-1 (which card 7.1 was meant to _close_).
3. **More ad-hoc raw z-index.** AppShell/TopNav/SecondaryNav/Footer now sprinkle raw `zIndex: 2` (layering the shell above PageGridBackdrop). Adds to F-AK-2 / F-W6-AK-2 debt (no defined z-index scale).
4. **OperationFlowTree lost its collapse/expand.** Rewrite removed `useState` + `Collapse` + the expand chevron — operation trees now render flat with dashed sibling connectors. If collapse was intended UX (deep call trees), this is a functional regression; if Figma specifies flat, it's intended. Verify against Figma / with designer.

**Lower-severity notes:**

- Contract `events` tab count is wired to `recent_unique_callers` (callers ≠ events) — possible mislabel; verify the stat is the intended "events count" the 0075 Future Work asked for, else it's a misleading pill.
- NFT media empty-state copy "There is no image" reads awkwardly (English nit).

---

## Stale-finding note (NOT a design_parity closure)

**F-W6-F-2 / F-W6-E7-1 / F-W6-E10-1 (card 7.4 — filter slots lack accessible names):** The filter inputs already carried `aria-label` + `placeholder` at `06ab34cc^` (the commit's own parent) — verified by reading the pre-merge AssetFilters/NftFilters and the untouched PoolsFilterBar. design_parity only added responsive widths. So card 7.4's a11y finding was **already resolved by an earlier batch** (likely Gate B); the Wave 6 finding is stale as written, and design_parity is not the resolver. Recommend re-verifying card 7.4 against current `main`/develop and likely downgrading it to "verify-only" — independent of this commit.
