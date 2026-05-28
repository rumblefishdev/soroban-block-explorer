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
