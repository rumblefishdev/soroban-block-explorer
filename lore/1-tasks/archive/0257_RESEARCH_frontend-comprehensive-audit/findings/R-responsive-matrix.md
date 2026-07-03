# R — Responsive matrix 14×3 (Wave 6 / 2.4) — summary

CSV: `R-responsive-matrix.csv` (42 cells, ASCII glyphs ✓/⚠/✗/?).

## Aggregate

| Result       | Count |    % |
| ------------ | ----: | ---: |
| ✓ (correct)  |    14 |  33% |
| ⚠ (partial)  |    11 |  26% |
| ✗ (broken)   |    11 |  26% |
| ? (untested) |     6 |  14% |
| **Total**    |    42 | 100% |

Of 36 tested cells: 14 ✓ / 11 ⚠ / 11 ✗ → **22 / 36 (61%) NOT clean**.

## Per-breakpoint

| Breakpoint   |   ✓ |   ⚠ |   ✗ |   ? |
| ------------ | --: | --: | --: | --: |
| 375 mobile   |   0 |   0 |  11 |   3 |
| 768 tablet   |   0 |  11 |   0 |   3 |
| 1280 desktop |  14 |   0 |   0 |   0 |

**Pattern:** desktop pristine; tablet uniformly degraded (page horiz-scroll); mobile uniformly broken.

## Root cause (single)

### F-W6-RESPONSIVE-1 [Class C, Severity 🟠 HIGH] All routes break at viewport <800px due to fixed minimum page width ~802px

Mechanism (observed via JS evaluation in browser DevTools at 375 + 768 viewports):

- `document.documentElement.scrollWidth = 802` at both 375 and 768 widths
- `document.documentElement.clientWidth` = 364 (mobile) or 757 (tablet)
- `<main>` width tracks viewport (e.g. `width: 757px` at tablet)
- So the overflow is in the layout shell above `<main>` — most likely `<header>` `<HeaderStatsStrip>` or the `<AppShell>` container has a hardcoded min-width (or `display: flex` row that won't wrap below ~800px).

Result: every route shows page-level horizontal scrollbar on mobile (severe usability) and a smaller horizontal scrollbar on tablet (still annoying — content barely fits, edge-clipping risk).

**Likely fix sites:**

- `web/src/router/AppShell.tsx`: check `Container maxWidth` / `Box minWidth` settings
- `libs/ui/src/layout/HeaderStatsStrip.tsx`: 4 stats inline — if `flex-wrap: nowrap` it forces min-width

**Fix complexity:** medium (1-2 days). Hamburger menu addition for nav at <768 separately needed.
**Cross-cite:** new Wave 6.

### F-W6-RESPONSIVE-2 [Class C, Severity 🟡 MEDIUM] No table → card-layout responsive transformation

Tables with 5-7 columns at mobile remain tables — the parent overflows horizontally. Modern responsive pattern: collapse to card-list at <600px with labeled key:value pairs. Not implemented anywhere.

**Cross-cite:** F-W6-RESPONSIVE-1 (same root layout issue).

### F-W6-RESPONSIVE-3 [Class C, Severity 🟡 MEDIUM] No hamburger / mobile nav

`<nav>` shows all 6 main links inline on every viewport. At 375 the nav links are visible but tiny (e.g. Home=54px wide, ~22px tall). No `aria-label="Open menu"` hamburger button.

### F-W6-RESPONSIVE-4 [Class A, Severity 🟢 LOW] Touch targets <44px on mobile

Sampled 30 button/link elements on home at 375 viewport: 7 measured <44px in both dimensions. WCAG 2.5.5 Target Size (Enhanced) target is 44×44 minimum. Nav links, copy buttons, "Previous"/"Next" pagination buttons all sub-44px tall.

## Phase 3 spawn recommendation

`XXXX_FEATURE_responsive-redesign-mobile-tablet.md`:

- Audit + fix the 800px min-width root cause (1 file likely)
- Add hamburger menu at <768
- Add table → card transformation OR horizontal-scroll-with-shadow pattern for embedded tables
- Audit touch targets to 44px minimum
- Effort: 3-5 days
- Class: C (pre-launch must-fix if mobile launch is a goal; otherwise prioritise post-launch)
- This is also a Track 2 audit finding; spawn unique vs bundling with format/truncate batch

## design_parity update 2026-05-27 (06ab34cc)

The `feat/design_parity` branch (commit `06ab34cc`, merge `62c988d4`) was merged into the audit branch and substantially addresses this matrix **in code** (no live re-run yet). Verdicts below are code-inspection only — see `design-parity-impact-2026-05-27.md` §2 + §Live-Playwright re-verify queue. Maps to queue card **8.3 → PARTIAL**.

- **F-W6-RESPONSIVE-1 (802px root cause): RESOLVED-IN-CODE / needs-live-verify.** AppShell `<main>` switched to responsive `px: { xs: grid.mobile.margin, md: grid.desktop.margin }`; TopNav/SecondaryNav/Footer switched off fixed `px: grid.desktop.margin`; Home full-bleed sections (HomeHero/ChainOverview/LatestLedgers/LatestTransactions) dropped the `px: 10` that forced extra width; HomeHero subtitle no longer nowrap at xs. No remaining fixed `minWidth` on shells (only `minWidth: 0` which allows shrink). **THE gating live check:** confirm `document.documentElement.scrollWidth === clientWidth` at 375 + 768 on all 14 routes.
- **F-W6-RESPONSIVE-2 (no table→card transform): PARTIAL.** ExplorerTable + standalone tx-detail tables (SignaturesTable, EventsSection) got `TableContainer sx={{ overflowX: 'auto' }}` — tables scroll within their own container instead of forcing page-level overflow. The recommended card-layout transform at <600px is still NOT implemented (and no scroll-shadow affordance). Acceptable mitigation; finding stays open for the card-layout goal.
- **F-W6-RESPONSIVE-3 (no hamburger): UNTOUCHED — alternative chosen.** SecondaryNav nav row now `overflowX: { xs: 'auto', md: 'visible' }` (hidden scrollbar) — nav **scrolls horizontally** on mobile. There is NO hamburger button / `aria-label="Open menu"` / collapse-to-menu. This contradicts card 8.3 + 0059 Future Work. **Designer decision needed:** accept scroll-nav as the answer (then close RESPONSIVE-3 / 0059) OR still require a hamburger.
- **F-W6-RESPONSIVE-4 (touch targets <44px): UNTOUCHED.** No 44px-minimum audit; HomeLogo height dropped 32→24 in SecondaryNav; nav buttons unchanged. Still open — needs live measurement.

Cross-ref: `design-parity-impact-2026-05-27.md`.

## Post-design_parity live re-verify 2026-05-27

Method: live Playwright MCP session, single browser, viewports 375 / 768 / 1280.
Gating metric per route×bp: `document.documentElement.scrollWidth > window.innerWidth`
(true = doc-level horizontal scroll = the Wave 6 802px failure still present).
All 14 routes walked at 375 + 768; desktop spot-checked (E1/E5/E14) for regressions.
Real detail IDs used: tx `7b9bacc8…`, ledger `1024`, account `GAHHHEIDIBOT…` (seed),
asset `6`, contract + NFT `CSTELLARCATS…` (+token `2`), LP `LD5MMO2Q…`, search `?q=test`.

### scrollWidth gating results (42 cells) — overflow true=FAIL, false=PASS

| Route                      | 375       | 768   | 1280   |
| -------------------------- | --------- | ----- | ------ |
| E1 `/`                     | ✓ 364     | ✓ 757 | ✓ 1269 |
| E2 `/transactions`         | ✓ 374     | ✓ 757 | ✓\*    |
| E3 `/transactions/:hash`   | ✓ 374     | ✓ 757 | ✓\*    |
| E4 `/ledgers`              | ✓ 374     | ✓ 757 | ✓\*    |
| E5 `/ledgers/:seq`         | ✓ 374     | ✓ 757 | ✓ 1269 |
| E6 `/accounts/:id`         | ✓ 374     | ✓ 757 | ✓\*    |
| E7 `/assets`               | ✓ 374     | ✓ 757 | ✓\*    |
| E8 `/assets/:id`           | ✓ 374     | ✓ 757 | ✓\*    |
| E9 `/contracts/:id`        | ✓ 374     | ✓ 757 | ✓\*    |
| E10 `/nfts`                | ✓ 374     | ✓ 757 | ✓\*    |
| E11 `/nfts/:cid/:tid`      | ✓ 374     | ✓ 757 | ✓\*    |
| E12 `/liquidity-pools`     | ✓ 374     | ✓ 757 | ✓\*    |
| E13 `/liquidity-pools/:id` | ✓ 374     | ✓ 757 | ✓\*    |
| E14 `/search`              | **✗ 644** | ✓ 768 | ✓ 1280 |

`✓*` = desktop confirmed pristine in Wave 6 + design_parity does not touch desktop;
re-verified by sampling (E1/E5/E14 = 1269/1269/1280, no overflow → no regression).

### Per-breakpoint summary

| Breakpoint   | PASS | FAIL | Notes                                             |
| ------------ | ---: | ---: | ------------------------------------------------- |
| 375 mobile   |   13 |    1 | only E14 search overflows (doc 644>375)           |
| 768 tablet   |   14 |    0 | uniformly fixed (all 757≤768, was 802 everywhere) |
| 1280 desktop |   14 |    0 | no regression                                     |

### Pre vs post comparison

- **Wave 6: 22/36 tested cells NOT clean** (11 ✗ mobile + 11 ⚠ tablet).
- **Post-design_parity: 1/42 cells FAIL** (E14@375 only).
- All 11 mobile ✗ → ✓; all 11 tablet ⚠ → ✓. The 6 previously-untested `?` cells
  (E9/E11/E14 × 375/768) are now measured: 5 PASS, 1 FAIL (E14@375).
- **The 802px AppShell root cause is gone on every route at both 375 and 768.**

### Finding verdicts

**F-W6-RESPONSIVE-1 (802px fixed-min-width root cause): RESOLVED.**
Confirmed live: zero routes show doc-level horizontal scroll attributable to the
AppShell/HeaderStatsStrip shell. At 375 docW=364/374 on all routes; at 768 docW=757
(previously 802 everywhere). The single remaining 375 overflow (E14) is a
search-results-page layout bug, NOT the shell min-width — see RESPONSIVE-5 below.

**F-W6-RESPONSIVE-2 (table responsiveness): RESOLVED (scroll-container mitigation).**
Every list + embedded table confirmed contained within an `overflowX:auto` parent:
tables measured 472–908px scroll-width inside 330px containers, doc never overflows.
Tables scroll horizontally within their own box. NOTE: the original "table→card
transform at <600px" goal is still NOT implemented — if that specific UX is the
acceptance bar, keep a separate enhancement finding open; but the _responsive-failure_
(page overflow) is fixed. Recommend: close RESPONSIVE-2 as a bug, spin the card-layout
transform into a low-priority enhancement backlog item.

**F-W6-RESPONSIVE-3 (no hamburger / mobile nav): STILL OPEN — designer decision.**
Live: zero hamburger/menu buttons exist (`aria-label menu|navigation` = none; the
only match was a false-positive "Open Tanstack query devtools" button). The
design_parity scroll-nav alternative is present but at 375 the 8 nav links happen to
fit within 364px without scrolling (short labels), each link ~32–110px wide. No
collapse-to-menu. Needs a designer call: accept scroll/inline-nav (→ close
RESPONSIVE-3 + 0059) OR require a hamburger.

**F-W6-RESPONSIVE-4 (touch targets <44px): STILL FAILING.**
Live at 375: pagination Previous=88×36, Next=64×36 (36px tall < 44); nav links
24–32px tall; **105 of 106 interactive elements <44px in at least one dimension**.
Untouched by design_parity. WCAG 2.5.5 still unmet. Stays open.

**F-W6-RESPONSIVE-5 [NEW, Class C, Severity 🟡 MEDIUM] Search-results page overflows doc at <~660px.**
At 375 the `/search` results render a category card (`DIV.css-83xvrh`, intrinsic
~628px) that does not shrink below ~660px viewport, forcing docW=644>375. Page-specific
(distinct from the AppShell root cause). Passes at 768 (644<768) and 1280. Was
untested (`?`) in Wave 6 so this is newly-surfaced, NOT a design_parity regression.
Likely fix: make the search result-category row/card wrap or use `minWidth:0` +
responsive flex on the result item container.

### Remaining responsive gaps

| Gap                                 | Status                               | Owner decision                            |
| ----------------------------------- | ------------------------------------ | ----------------------------------------- |
| Doc-level scroll (802px root cause) | RESOLVED                             | —                                         |
| Embedded/list tables overflow page  | RESOLVED (overflowX:auto)            | —                                         |
| Table→card transform <600px         | NOT done                             | enhancement backlog (optional)            |
| Hamburger / collapse mobile nav     | NOT done (scroll-nav alt present)    | designer: accept alt or require hamburger |
| Touch targets ≥44px                 | NOT done (36px pagination, 32px nav) | open — needs sizing pass                  |
| Search page <660px overflow (E14)   | NEW FAIL                             | fix search result card flex/wrap          |

### Recommendation — cells/cards/findings that can flip (for review, NOT applied)

**Can flip PARTIAL → DONE / RESOLVED:**

- Queue card **8.3** responsive root cause → its _scrollWidth/802px_ portion is DONE.
  But 8.3 likely bundles hamburger + touch targets; if so keep 8.3 PARTIAL and split.
- Appendix rows for **F-W6-RESPONSIVE-1** → **RESOLVED** (live-verified).
- Appendix rows for **F-W6-RESPONSIVE-2** → **RESOLVED as bug** (page-overflow fixed;
  card-transform = separate optional enhancement).
- CSV matrix: all 25 PASS cells from this re-verify can replace the Wave 6 ✗/⚠/? for
  those routes (kept as a separate dated section, originals preserved).

**Must STAY open / PARTIAL:**

- **F-W6-RESPONSIVE-3** (hamburger) — designer decision pending; do NOT auto-close.
- **F-W6-RESPONSIVE-4** (touch targets) — still failing live, untouched.
- **F-W6-RESPONSIVE-5** (search overflow) — NEW, open.

**Regressions:** none. Desktop (1280) clean on all sampled routes; design_parity
introduced no new desktop overflow.

## Disposition 2026-05-28

User decisions applied to the audit queue (`audit-action-queue.md`) after the live
re-verify above. Basis: design_parity `06ab34cc` + live Playwright re-verify 2026-05-28.

| Finding                                    | Disposition                                                                                                                      | Card                            |
| ------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| F-W6-RESPONSIVE-1 (802px root cause)       | **RESOLVED** — live-confirmed, 41/42 cells no doc-scroll, 768 docW=757 (was 802), 1280 pristine                                  | 8.3 (scrollWidth scope DONE)    |
| F-W6-RESPONSIVE-2 (table responsiveness)   | **RESOLVED as bug** — tables contained in `overflowX:auto`; table→card transform = separate optional enhancement (not a failure) | 8.3 (table-overflow scope DONE) |
| F-W6-RESPONSIVE-3 (hamburger / mobile nav) | **TODO** — user decision 2026-05-28: REQUIRE hamburger <768px; scroll-nav alternative rejected                                   | **C11.5 (new)**                 |
| F-W6-RESPONSIVE-4 (touch targets <44px)    | **TODO** — still failing live; 105/106 interactive elements <44px @375                                                           | **C11.6 (new)**                 |
| F-W6-RESPONSIVE-5 (search overflow <660px) | **TODO** — newly-surfaced live; search category card ~628px intrinsic                                                            | **C11.7 (new)**                 |

Card 8.3 stays **PARTIAL** (not DONE): its original scope bundled hamburger + touch
targets, which are split out to C11.5/C11.6; only the scrollWidth + table-overflow
portion is DONE. RESPONSIVE-1/2 RESOLVED are live-confirmed (not code-only).

## RESPONSIVE-5 reclassification — live re-verify 2026-05-29

**F-W6-RESPONSIVE-5 → RESOLVED (page overflow gone, live-confirmed 2026-05-29).** The
2026-05-28 "NEW FAIL" page overflow (docW=644>375) is REFUTED on the R1+R2-merged build:
`/search?q=test` @375 reports `documentElement.scrollWidth = 364 ≤ innerWidth 375` — no
page-level horizontal scroll. The ~651px category-card row now sits in an `overflow-x:auto`
container (clientWidth 332) and scrolls WITHIN the container — same scroll-within mitigation
as embedded tables (RESPONSIVE-2), does not push page width. Residual: true per-card
reflow/wrap still absent → optional NICE enhancement (same class as the RESPONSIVE-2
table→card transform), not a bug. Card 11.7 → DONE-mitigated. Screenshot:
`screenshots/search-375-no-page-overflow.png`. Source: `design-parity-impact-2026-05-29.md`
§Live re-verify 2026-05-29 (item 7).
