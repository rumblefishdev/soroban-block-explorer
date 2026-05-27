# AD — Maintenance cost (Wave 5 1.10d)

**Wave:** 5 (Tier 4 subjective)
**Stance:** "can a junior developer change something here without predecessor context?"
**Date:** 2026-05-25
**Baseline SHA:** `81928602`.

## Per-check table

| #    | Check                                                       | Verdict | Evidence                                                                                                                                                                                                                                       | Severity | Class |
| ---- | ----------------------------------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ----- |
| AD-1 | Junior can change something without predecessor context?    | partial | 3 sampled changes below; 1 easy, 1 moderate, 1 requires cross-file coordination. See AD-1 sample                                                                                                                                               | 🟡       | C     |
| AD-2 | Bug fix requires changes in 5+ files (leaked concern)?      | ⚠       | See F-AD-1 — fixing **truncation** requires 6 files (cross-cite F-U-3); fixing **STROOPS conversion** requires 2 files (F-U-4); fixing **debounce-input** requires 4 files (F-Y-2). Leaks accumulate at "formatters / helpers" layer           | 🟠       | C     |
| AD-3 | Each component has unit test protecting against regression  | ✗       | 0 test files in `web/src` + `libs/ui/src`. Cross-cite F-AH-6 / Wave 1 P + AQ. Documented dropped scope `O`, Phase 3 spawn                                                                                                                      | 🟠       | D     |
| AD-4 | Implicit dependencies (components requiring parent context) | ✓       | Only 1: `useColorMode` requires `<ExplorerThemeProvider>`. Documented via throw on missing context (`ThemeProvider.tsx:93`). Cross-cite Wave 5 Z-7 — informative throw.                                                                        | —        | —     |
| AD-5 | Magic strings/numbers without constants                     | ✓       | Strong discipline. See AD-5 inventory below — magic numbers are either named constants, Figma-node-tagged, or have comment rationale                                                                                                           | —        | —     |
| AD-6 | Onboarding docs for FE exist                                | partial | `web/README.md` 74 lines, `libs/ui/README.md` 33 lines, `libs/api-types/README.md` 31 lines, root `README.md` 105 lines. `docs/architecture/frontend/frontend-overview.md` 740 lines. **Sufficient for cold-start dev in ~1 hour.** See F-AD-2 | 🟢       | D     |

## AD-1 sample — junior change scenarios

**Scenario 1: "Add a column to the transactions list."**

1. Find `web/src/pages/transactions/TransactionsTable.tsx` (1 hop from `TransactionsListPage.tsx`).
2. Add column descriptor; `ExplorerTable` (libs/ui) renders.
3. Pull new column data from existing `TransactionListItem` (libs/api-types generated type — `Cmd-Click` to navigate).
4. **Junior effort:** ~30 minutes. **Predecessor context needed:** how `ExplorerTableColumn<T>` works (1-look at type def).

**Verdict:** ✓ easy.

**Scenario 2: "Add a new filter on the assets list."**

1. Find `web/src/pages/assets/AssetFilters.tsx`.
2. Recognize the `useEffect` debounce pattern (cross-cite F-Y-2).
3. Adapt `useTableUrlState` filter key. Wave 4 F-AL (state separation) showed `setFilter()` drops cursor → so cursor invariant is handled.
4. Add column to `AssetsTable.tsx` if filter shows in row, plus URL key registration.
5. **Junior effort:** ~1-2 hours. **Predecessor context needed:** `useTableUrlState` filterKeys param + the debounce-effect dance.

**Verdict:** ⚠ moderate — duplicated debounce pattern slows junior down (F-Y-2 dependency).

**Scenario 3: "Rename a Stat label in TopNav."**

1. Find `libs/ui/src/layout/TopNav.tsx`.
2. Recognize `formatNumber` local-reimplemented vs `formatCompactAmount` in `web/src/pages/pool-detail/helpers.ts` (cross-cite Wave 2 J-3).
3. Confirm label text source — Figma `node 3:2333` per comments.
4. **Junior effort:** ~30 minutes. **Predecessor context needed:** why TopNav has its own formatter (because libs/ui can't import from web/src/pages — boundary), and Figma node link.

**Verdict:** ⚠ moderate — duplicate-formatter context.

**Subjective:** maintenance cost is **moderate not high**. The main friction is the formatter/helper duplication (F-U / F-J / F-Y) — once unified in Phase 3, all 3 scenarios become easier.

## AD-5 — Magic numbers inventory

Found 8 numeric literals ≥4 digits in user code:

| Site                                                      | Value                                      | Justified?                                                                                                    |
| --------------------------------------------------------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------- |
| `web/src/api/polling.ts:4-25`                             | 10_000, 12_000, 60_000, 5\*60_000          | ✓ named (`homePolicy.staleTime`, etc.) with JSDoc rationale                                                   |
| `web/src/api/hooks/usePoolChart.ts:13`                    | `DAY_MS = 24 * 60 * 60 * 1000`             | ✓ named constant                                                                                              |
| `web/src/pages/pool-detail/helpers.ts:3`                  | `SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000`  | ✓ named constant + JSDoc                                                                                      |
| `web/src/search/SearchResultsTabs.tsx:60`                 | `borderRadius: 9999`                       | ✓ idiomatic CSS pill-radius marker; design-system convention                                                  |
| `web/src/pages/transaction-detail/advanced/XdrRow.tsx:33` | `setTimeout(() => setCopied(false), 1500)` | ⚠ inline `1500` ms; should be `COPIED_FEEDBACK_MS = 1500` constant. Minor.                                    |
| `web/src/pages/HomePage.tsx:59`                           | `width: 1062`                              | ⚠ Figma layout px; should be CSS variable or commented. Minor.                                                |
| `web/src/pages/home/ChainOverview.tsx:80`                 | `maxWidth: 1064`                           | ⚠ same as above. Minor.                                                                                       |
| `web/src/pages/liquidity-pools/PoolsFilterBar.tsx:22-24`  | `10000 / 100000 / 1000000`                 | ✓ Figma-tagged in nearby comment `node 267:60674`; named labels (`Min $10,000` etc.) so user sees the meaning |
| `web/src/pages/liquidity-pools/assetColor.ts:109`         | `h = 5381`                                 | ✓ djb2 hash constant — well-known number; comment could clarify                                               |

**Net:** ~3 minor magic-number nits (1500ms feedback, 1062/1064 layout px). Otherwise strong discipline.

**Class:** D — Phase 3 batch polish.

## AD-6 — Onboarding doc inventory

| File                                              | Size        | Cold-start adequacy                                                  |
| ------------------------------------------------- | ----------- | -------------------------------------------------------------------- |
| `README.md` (root)                                | 105 lines   | sets context, monorepo overview, links                               |
| `web/README.md`                                   | 74 lines    | env config, structure tree, dev commands, data-layer rules ✓         |
| `libs/ui/README.md`                               | 33 lines    | "what goes here / what doesn't" rules ✓ (concise but clear)          |
| `libs/api-types/README.md`                        | 31 lines    | codegen workflow ✓                                                   |
| `docs/architecture/frontend/frontend-overview.md` | 740 lines   | comprehensive (per-route, per-state, data flow); reference document  |
| `CLAUDE.md` (project, root)                       | (committed) | session/task gate, deletion policy, codegen workflow, evergreen docs |

**Junior cold-start time estimate:** ~1-2 hours to read all 5 docs + skim `frontend-overview.md` § for area of interest. Strong onboarding.

**Gap:** none of these docs describe the **convention** rules (PascalCase naming, hook organization, formatter homes, useTableUrlState vs useDetailMode pattern). New contributor learns these by code-reading. Phase 3 sub-phase 3.5 wiki sweep covers this.

## Findings

### F-AD-1 [Class C, Severity 🟠] — Leaked concerns: bug fixes requiring 5+ files (RECAP)

| Bug class                                                    | Files to change | Cross-cite |
| ------------------------------------------------------------ | --------------- | ---------- |
| Truncation re-impl                                           | 6 files         | F-U-3      |
| STROOPS_PER_XLM                                              | 2 files         | F-U-4      |
| `formatFee`                                                  | 2 files         | F-J-16     |
| Stroop display formatter (third entry point `formatStroops`) | 3 files         | F-J-17     |
| Debounce pattern                                             | 4 files         | F-Y-2      |
| Inline number formatters                                     | 10 files        | F-U-2      |

- **Verdict:** the **formatter/truncation/debounce family** is the project's #1 maintenance-cost leak. A future "change how addresses truncate" requires editing 6 files instead of 1.
- **Recommendation:** Phase 3 single PR consolidating into `libs/ui/src/format/` + `libs/ui/src/identifiers/truncate.ts` extensions. Estimated 0257 spawn task: `XXXX_REFACTOR_frontend-format-truncate-unification` (M-effort).
- **Class:** C — Gate B fix-first candidate (per F-Y-6 batch).

### F-AD-2 [Class D, Severity 🟢] — Onboarding doc completeness — strong baseline + Phase 3 wiki additions

- Strong cold-start adequacy via per-package READMEs + `docs/architecture/frontend/frontend-overview.md`.
- Gap: convention rules (formatters, hooks, naming) not documented. Phase 3 sub-phase 3.5 covers.
- **Class:** D — already in scope.

### F-AD-3 [Class D, Severity 🟢] — 3 inline magic numbers worth naming (AD-5 recap)

- `1500` (XdrRow copied feedback ms) → `COPIED_FEEDBACK_MS`
- `1062` (HomePage layout width) → context comment or CSS var
- `1064` (ChainOverview maxWidth) → context comment or CSS var
- **Class:** D — Phase 3 polish batch.

### F-AD-4 [Class A, Severity 🟢] — Zero implicit-context surprises (positive baseline)

- Only `useColorMode` requires `<ExplorerThemeProvider>` context, and the throw at `ThemeProvider.tsx:93` carries informative message.
- TanStack, React Router contexts are framework-required and provided at root.
- **Class:** A — positive baseline.

### F-AD-5 [Class D, Severity 🟠] — Zero test coverage (cross-cite)

- F-AH-6 / Wave 1 P / AQ confirmed.
- Dropped scope `O` — Phase 3 spawn.
- **Class:** D — known.

## Cross-cites

- F-AD-1 ↔ F-U-3, F-U-4, F-J-16, F-J-17, F-Y-2, F-U-2 (the formatter/truncation/debounce drift batch).
- F-AD-3 ↔ F-AD-5 inventory.
- F-AD-5 ↔ F-AH-6, Wave 1 P + AQ.
- F-AD-2 ↔ Z-3 (junior discoverability).

## Net 1.10d finding count

5 findings: 0 🔴 / 2 🟠 (F-AD-1 + F-AD-5) / 0 🟡 / 3 🟢.

**Class breakdown:** A=1 / C=1 / D=3.

**Subjective calls (Tier 4 flagging for user spot-check):**

1. **F-AD-1 (leaked-concern severity 🟠)** — "6-file truncation change" is the highest maintenance-cost smell. User may agree priority, or downgrade if Phase 3 unification ships next regardless.
2. **F-AD-3 (3 magic numbers)** — opinion-based whether these 3 are worth Phase 3 polish vs ignore. User decides.

## Overall AD verdict (subjective)

**Maintenance cost is moderate-low** for current size. The main driver is **cross-task formatter/truncation/debounce drift** — once Phase 3 unification PR ships (F-U / F-J / F-Y findings), maintenance cost drops to "low" tier.

**Junior contributor reaches productive change in 1-2 hours** with current onboarding docs + 1-hop code navigation. The 3 sample scenarios (column add, filter add, label rename) are all achievable without senior shadow.

**Test coverage gap** is the single biggest pre-launch maintenance risk — Phase 3 testing-baseline spawn (per dropped scope `O`) is the right move; should be high-priority among Phase 3 spawned tasks.

## Top issues

1. **F-AD-1 (🟠 C)** — leaked-concern from formatter/truncation/debounce drift. Phase 3 unification fixes 6 findings at once.
2. **F-AD-5 (🟠 D)** — test coverage gap. Pre-launch spawn priority.
3. **F-AD-3 (🟢 D)** — 3 magic-number polish.
