# Y — Simplicity (Wave 5 1.9c)

**Wave:** 5 (Tier 4 subjective)
**Stance:** senior fresh-eye — "is the complexity inherent to the domain, or accidental?"
**Date:** 2026-05-25
**Baseline SHA:** `81928602`.

## Per-check table

| # | Check | Verdict | Evidence | Severity | Class |
|---|---|---|---|---|---|
| Y-1 | Most complex component — justified? | mostly ✓ | Top 5 sized files reviewed below — 4/5 justified by domain; 1 has split potential | 🟢 | D |
| Y-2 | Longest file should be split | partial | `libs/ui/src/theme/overrides.ts` 890 LOC — splittable per MUI component family. See F-Y-1 | 🟡 | D |
| Y-3 | Deepest conditional nesting | ✓ | Sampled 5 longest files; no >4-level nesting in render/handler functions | — | — |
| Y-4 | Copied blocks (3+ occurrences) → extract? | ✗ | Cross-cite F-U-3 (6 truncation re-impls), F-U-4 (2 STROOPS constants), F-J-16 (2 formatFee impls), F-J-17 (filter `useEffect` pair in 3 filter components). See F-Y-2 below | 🟠 | C |
| Y-5 | `useEffect` where `useMemo` / event handler suffices | partial | 16 `useEffect` sites across `web/src` (excluding libs/ui infrastructure). Pattern is sound (debounce + re-sync) but the **same 2-effect debounce pattern is copy-pasted 4 times across filter components**. See F-Y-3 | 🟡 | C |
| Y-6 | `useState` that should be URL state | partial | Cross-cite Wave 4 F-AL-1 (`selectedIndex` in tx-detail — borderline, deferred to Gate B) — already inventoried | 🟡 | C |
| Y-7 | Local inline components where shared component fits | ✗ | Cross-cite Wave 4 F-U-1, F-U-2, F-U-3, F-U-4 (SectionCard local; inline formatters; truncation re-impls; STROOPS dup). All Class C — defer Gate B | 🟠 | C |

## Longest files inventory

```
890  libs/ui/src/theme/overrides.ts
268  web/src/pages/pool-detail/PoolCharts.tsx
266  libs/ui/src/visualization/OperationFlowTree.tsx
249  libs/ui/src/theme/types.ts
246  libs/ui/src/layout/SearchInput.tsx
237  web/src/pages/contracts/ContractEvents.tsx
223  libs/ui/src/visualization/TimeSeriesChart.tsx
219  web/src/pages/contracts/ContractInterface.tsx
206  web/src/pages/transaction-detail/normal/toFlowNodes.tsx
204  web/src/pages/transaction-detail/sections/OperationPicker.tsx
```

## Findings

### F-Y-1 [Class D, Severity 🟡] — `libs/ui/src/theme/overrides.ts` 890 LOC is the only justifiably-splittable longest file

- **Location:** `libs/ui/src/theme/overrides.ts`
- **Composition:** Single exported `overrides: Components<Theme>` object literal with 20+ component family override blocks: `MuiCssBaseline`, `MuiButton` (l29), `MuiChip` (l171), `MuiSwitch` (l280), `MuiFormControlLabel` (l368), `MuiCheckbox` (l377), `MuiSlider` (l423), `MuiTextField` (l498), `MuiOutlinedInput` (l503), `MuiInputLabel` (l557), `MuiFormHelperText` (l576), `MuiSelect` (l596), `MuiMenu` (l608), `MuiMenuItem` (l630), `MuiListSubheader` (l658), `MuiPaper` (l673), `MuiCard` (l689), `MuiTabs` (l703), `MuiTab` (l715), `MuiTableContainer` (l742+).
- **Subjective:** the file is grouped MUI-by-MUI but doesn't have internal section comments. A reader looking for "where do Buttons get styled?" must scroll-search.
- **Recommendation:** split per family into `libs/ui/src/theme/overrides/{button,chip,switch,slider,text-input,select,menu,paper,card,tab,table,baseline}.ts`, then re-aggregate into `overrides.ts` (10-20 LOC barrel). Each file ~50-100 LOC. Single PR; easy to review chunk by chunk.
- **Cost-benefit:** modest refactor, modest payoff (better discoverability + smaller per-PR diffs when designers tweak one family).
- **Class:** D (no behavior change, no risk).

### F-Y-2 [Class C, Severity 🟠] — Copied debounce-and-re-sync pattern across 3+ filter components (NEW)

**Pattern signature:** identical 2-effect dance for "controlled-input that debounces an `onChange`":

```tsx
const [draft, setDraft] = useState(initial);

// Re-sync local input when value changes externally (e.g. "Clear filters")
useEffect(() => { setDraft(initial); }, [initial]);

// Debounce committing typed value to avoid refetch per keystroke
useEffect(() => {
  if (draft === initial) return;
  const id = setTimeout(() => onChange(draft), SEARCH_DEBOUNCE_MS);
  return () => clearTimeout(id);
}, [draft, initial, onChange]);
```

**Sites:**

| File:line | Filter |
|---|---|
| `web/src/pages/transactions/TransactionFilters.tsx:36-49` | tx search |
| `web/src/pages/assets/AssetFilters.tsx:25-40` | asset search |
| `web/src/pages/nfts/NftFilters.tsx:24-40` | NFT search |
| `web/src/pages/liquidity-pools/PoolsFilterBar.tsx:46-62` | LP asset-code search |

- **Pattern lives in `web/src/search/useDebounced.ts`** already (5-line `useDebounced` hook). Filter components don't use it; they reinvent.
- **Recommendation:** extract a `useDebouncedDraft<T>(value, onChange, delay)` hook that returns `[draft, setDraft]` + handles the 2-effect dance. Use across the 4 filter components.
- **Class:** C (visual contract; identical semantics, cleaner cross-section pattern).
- **Cross-cite:** Wave 4 F-U series — yet another component-reuse violation; **belongs to the same Gate-B batch as F-U-3 (truncation) + F-U-4 (STROOPS)**.

### F-Y-3 [Class A, Severity 🟢] — `useEffect` usage inventory: sound, no obvious over-use

16 `useEffect` sites across `web/src/` (excluding libs/ui infrastructure):

| File:line | Purpose | Justification |
|---|---|---|
| `web/src/search/useSearchResults.ts:122` | debounce + dedupe abort | ✓ legitimate side effect |
| `web/src/search/useDebounced.ts:5` | timer-driven value update | ✓ |
| `web/src/search/GlobalSearchBar.tsx:28, 40` | command-K shortcut + focus mgmt | ✓ |
| `web/src/pages/SearchResultsPage.tsx:17` | URL → input sync | ✓ |
| `web/src/pages/liquidity-pools/PoolsFilterBar.tsx:52, 57` | F-Y-2 pattern | ✓ but duplicated |
| `web/src/pages/nft-detail/NftMediaPreview.tsx:90` | iframe `load` listener attach | ✓ |
| `web/src/pages/nfts/NftNameCell.tsx:31` | (sampled — controlled-input sync) | ✓ |
| `web/src/pages/transactions/TransactionFilters.tsx:40, 45` | F-Y-2 pattern | ✓ but duplicated |
| `web/src/pages/nfts/NftFilters.tsx:30, 34` | F-Y-2 pattern | ✓ but duplicated |
| `web/src/pages/assets/AssetFilters.tsx:31, 35` | F-Y-2 pattern | ✓ but duplicated |

- **Verdict:** zero `useEffect` doing what `useMemo` / event handler should. All effects are legitimate (timers, controlled-input re-sync, DOM listeners, URL sync).
- **Only smell:** the **4× repeated debounce pattern** (F-Y-2).
- **Class:** A (informational — confirms useEffect discipline is good).

### F-Y-4 [Class C, Severity 🟢] — `web/src/pages/pool-detail/PoolCharts.tsx` 268 LOC — borderline justified

- **Composition:** 3 USD/date formatters at module level (good — see lines 28-50), tabs state, metric mapping, fixed period array, chart series transformation, the actual chart component, period selector, lazy section wiring.
- **Subjective:** could split into `PoolCharts.tsx` (~120 LOC, render scaffold) + `usePoolChartViewModel.ts` (formatters + transformation + period mapping ~100 LOC) + `PoolChartTabs.tsx` (tab UI). But not urgent — 268 LOC is below most teams' "must split" threshold.
- **Recommendation:** Phase 3 if 1.10b AA Overengineering or 1.10d AD Maintenance cost identifies that "junior changing the chart needs to understand the whole file". Otherwise accept.
- **Class:** C — defer.

### F-Y-5 [Class A, Severity 🟢] — Top remaining long files: domain-justified

- `web/src/pages/contracts/ContractEvents.tsx` 237 LOC — events table + filter + JSON-render of decoded payload + truncation. Domain complexity (event payloads are heterogenous).
- `libs/ui/src/visualization/OperationFlowTree.tsx` 266 LOC — Soroban operation flow tree visualization with nested children + 5 node kinds. Domain complexity.
- `libs/ui/src/visualization/TimeSeriesChart.tsx` 223 LOC — Recharts wrapper with axis formatters + tooltip + responsive container. Library-wrapper pattern.
- `web/src/pages/transaction-detail/normal/toFlowNodes.tsx` 206 LOC — transformation function from `OperationItem[]` → `FlowNode[]`. Per-op-type branching is inherent.
- `web/src/pages/transaction-detail/sections/OperationPicker.tsx` 204 LOC — operation picker UI with state + scroll behavior + active highlighting.
- **Verdict:** all domain-justified. No split urgency.
- **Class:** A — baseline note.

### F-Y-6 [Class C, Severity 🟡] — Cross-references to component-reuse findings (recap)

The following Wave 4 findings are simplicity findings — same root cause:

- F-U-1 [Class C 🟡] — `SectionCard` local-not-shared
- F-U-2 [Class C 🟡] — inline `toFixed`/`toLocaleString` (10 sites)
- F-U-3 [Class C 🟠] — 6 truncation re-impls
- F-U-4 [Class A 🟠] — 2 `STROOPS_PER_XLM` constants
- F-J-16 [Class C 🟠] — 2 `formatFee` impls
- F-J-17 [Class C 🟡] — `formatStroops` third stroop-display entry point

**Recommendation:** Phase 3 batch as `XXXX_REFACTOR_frontend-format-and-truncate-unification` — single PR consolidating 6 findings into 1 atomic change. Bundle with F-Y-2 (debounce hook extract) into the same family if pacing allows.

## Net 1.9c finding count

6 findings (1 new in this wave + 5 cross-cites): 0 🔴 / 1 🟠 (F-Y-2 new) / 3 🟡 / 2 🟢.

**Class breakdown:** A=2 / C=3 / D=1.

**Subjective calls (Tier 4 flagging for user spot-check):**

1. **F-Y-1 (overrides.ts split)** — split is a judgment-only refactor; user may prefer "leave 890 LOC, MUI override files often run long". Defer trivially.
2. **F-Y-2 (debounce extract)** — new finding; F-Y-2's "extract useDebouncedDraft" is a clear DRY win, but 4 sites = on the edge of "rule of 3" threshold; some teams accept 4-site duplication. **User decides.**
3. **F-Y-4 (PoolCharts split)** — borderline; 268 LOC is under most "split" thresholds. **User decides** if want strict <250 LOC.

## Top issues

1. **F-Y-2 (🟠 C)** — debounce pattern duplicated 4× across filter components. Same Gate-B batch as F-U-3/F-U-4.
2. **F-Y-1 (🟡 D)** — overrides.ts split, low-stakes refactor for Phase 3.
3. Cross-cites confirm Wave 4 component-reuse + Wave 2 J-formatting findings are the same systemic gap.
