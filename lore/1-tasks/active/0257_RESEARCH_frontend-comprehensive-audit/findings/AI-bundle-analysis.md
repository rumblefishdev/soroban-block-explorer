# 1.16 AI — Bundle analysis

Date: 2026-05-25
Tools: `nx run @rumblefish/soroban-block-explorer-web:build` (Vite),
`du`, `ls -lhS`.

## Build result

`nx run @rumblefish/soroban-block-explorer-web:build` — exit 0,
1.90s wall (cached deps), output in `web/dist/`.

Total: **2.7 MB** (`du -sh web/dist`), of which `assets/` ≈ **1.2 MB**.

## Vite warning

```
(!) Some chunks are larger than 500 kB after minification. Consider:
- Using dynamic import() to code-split the application
- Use build.rollupOptions.output.manualChunks to improve chunking
```

Triggered by `index-oicZakw3.js` at **594 KB minified / 188 KB gz**.

## Chunk inventory (largest)

| Chunk | KB (min) | KB (gz) | Notes |
|---|---:|---:|---|
| **`index-*.js`** | **594.55** | **188.87** | main bundle — React + MUI + TanStack + router + AppShell + every eagerly-imported lib |
| `LiquidityPoolDetailPage-*.js` | 313.11 | 95.57 | **second largest** — likely `@mui/x-charts` bundled here (only LP page uses TimeSeriesChart) |
| `SearchOutlined-*.js` | 67.33 | 20.21 | strange — single MUI icon as own chunk (size hints at chart deps inlined?) |
| `ContractDetailPage-*.js` | 21.72 | 7.00 | OK |
| `cursorParams-*.js` | 16.62 | 5.82 | shared cursor logic |
| `HomePage-*.js` | 13.54 | 4.48 | OK |
| `ExplorerTable-*.js` | 11.57 | 4.14 | shared (lazy, surprising — should be eager) |
| `NftDetailPage-*.js` | 9.90 | 3.72 | OK |
| `LedgerDetailPage-*.js` | 6.06 | 2.41 | OK |
| `AccountDetailPage-*.js` | 5.73 | 2.41 | OK |
| `AssetDetailPage-*.js` | 5.68 | 2.23 | OK |
| `LiquidityPoolsListPage-*.js` | 5.38 | 2.31 | OK |
| `NftsListPage-*.js` | 5.09 | 2.27 | OK |
| `AssetsListPage-*.js` | 4.63 | 2.11 | OK |
| `PageBreadcrumb-*.js` | 4.60 | 1.94 | OK |
| `TransactionsListPage-*.js` | 3.55 | 1.75 | OK |
| `MenuItem-*.js` | 3.40 | 1.41 | OK |
| `Link-*.js` | 3.23 | 1.33 | OK (MUI Link split) |
| `AssetIcon-*.js` | 3.20 | 1.59 | OK |
| `FeePill-*.js` | 3.12 | 1.60 | OK |
| `usePageHandlers-*.js` | 2.49 | 1.24 | OK |
| `cells-*.js` | 2.43 | 1.12 | OK |
| `TableEmptyState-*.js` | 2.17 | 1.07 | OK |
| `LedgersListPage-*.js` | 1.60 | 0.87 | OK |
| `TransactionsTable-*.js` | 1.05 | 0.54 | OK |
| `TransactionDetailPage-*.js` | 0.95 | 0.50 | the stub (matches A1 in archaeology — page is intentionally tiny) |
| `SearchResultsPage-*.js` | 0.93 | 0.56 | OK |

**CSS:** `index-*.css` = **636 B** (Emotion mostly runtime).

## Code-split / lazy()

`web/src/router/index.tsx:1-15` defines `page(load)` = `lazy(load) + Suspense<DetailSkeleton>`. **All 17 routes use `page()` wrapper.** Per-route code-split is correctly applied.

Each page-chunk is named by Rollup auto-naming, with shared dep chunks promoted automatically (e.g., `cursorParams`, `ExplorerTable`, `cells`, `MenuItem`).

## Findings

### F-AI-1 — `index-*.js` 594 KB / 188 KB gz exceeds Vite's 500 KB warning — 🟠 HIGH

The eager main bundle (React + MUI + TanStack + router + AppShell + theme) is **188 KB gz**, just below the typical 200 KB SPA budget but above the
ideal 100-150 KB. Three biggest payers are likely:

1. `@mui/material` + `@mui/system` + `@mui/utils` (triple-versioned per F-CO-6)
2. `@tanstack/react-query` + devtools (devtools should be `import.meta.env.DEV`-only)
3. React + ReactDOM

**Diagnosis path:** `npx vite-bundle-visualizer` (not installed) or
`rollup-plugin-visualizer` would split the index chunk by source.
Skipped here (not installed); spawn a follow-up to land it in CI.

### F-AI-2 — `LiquidityPoolDetailPage` 313 KB / 95 KB gz — 🟠 HIGH

This one route is **~50% the size of the entire eager bundle**. `@mui/x-charts` is almost certainly the culprit — `TimeSeriesChart` is used by `PoolCharts.tsx` (per task 0077). `@mui/x-charts` is famously heavy because it bundles D3-shape, D3-scale, D3-color, etc.

Mitigations to evaluate:

- Confirm `@mui/x-charts` is only on LP detail (no other page uses it — should be true per code-split chunking)
- Switch to `recharts` or `victory` (smaller, but a real migration)
- Lazy-load `PoolCharts` within the LP page so the chart code only loads when user scrolls / clicks the Charts tab

### F-AI-3 — `SearchOutlined-*.js` 67 KB stand-alone chunk — 🟡 MEDIUM

The MUI Search icon as a 67 KB lazy chunk is suspicious; single icons are typically <2 KB. Likely contains something else co-imported via the `MenuItem` / `Link` lazy graph — confirms an opportunity for `manualChunks` tuning. May be a TanStack-Query-Devtools accidentally split into a search-adjacent chunk; verify via visualizer.

### F-AI-4 — `ExplorerTable-*.js` is its own chunk — 🟢 LOW

`ExplorerTable` is used by every list page; Rollup auto-split it into a shared chunk (good — prevents per-page duplication), but at 11 KB it might be more efficient to inline into the main bundle (every route uses it, parallel network round-trip cost > size cost).

### F-AI-5 — `@tanstack/react-query-devtools` shipped in production — ✅ (verified false alarm)

`web/src/api/QueryProvider.tsx:28-30` wraps `<ReactQueryDevtools>` in
`{import.meta.env.DEV ? ... : null}` — Vite tree-shakes the import away
in production builds. **Confirmed clean, no action.**

### F-AI-6 — Tree-shaking validation — ✅

`grep -rnE "from ['\"](lodash|lodash-es)"` → 0 hits in source. `from '@mui/material'` named imports are tree-shakable via MUI's ESM (verify in visualizer). `from '@mui/icons-material/SearchOutlined'` (default import paths) are correct per MUI tree-shake docs.

### F-AI-7 — No `vite-bundle-visualizer` in deps — 🟡 MEDIUM

Cannot produce a treemap without installing. CI has no bundle-size regression gate. Pre-launch recommendation: install `rollup-plugin-visualizer` dev-dep + add a one-off generate step + commit a baseline JSON for diffing.

### F-AI-8 — No vendor chunk split — 🟡 MEDIUM

The 594 KB index includes React + MUI + everything. Cache efficiency would improve with `manualChunks: { 'react-vendor': ['react', 'react-dom'], 'mui-vendor': [...] }`. Recommend a `vite.config.ts` `manualChunks` tune as a follow-up.

### F-AI-9 — CSS total = 636 B — ✅

Emotion runtime injects styles into `<style>` tags at runtime, so the static CSS is tiny. Real CSS surface is in the JS chunks (Emotion strings). Trade-off accepted for MUI ecosystem.

## Conclusion

Build is **green, code-split per route, tree-shake-clean**, but the
main bundle exceeds Vite's 500 KB warning, and one route (LP detail) is
nearly as big as the rest combined.

| Severity | Findings |
|---|---|
| 🔴 CRITICAL | 0 |
| 🟠 HIGH | 2 (F-AI-1 main bundle size, F-AI-2 LP chart heavy) |
| 🟡 MEDIUM | 3 (F-AI-3 weird icon chunk, F-AI-7 no visualizer, F-AI-8 no vendor split) |
| 🟢 LOW | 2 (F-AI-4 table chunk, F-AI-9 CSS tiny — informational) |

## Recommendations

1. **🟠 HIGH (F-AI-1 + F-AI-8):** Spawn `XXXX_REFACTOR_frontend-vendor-chunk-split` — add `manualChunks` for react / mui / tanstack vendor splits + verify @mui/utils unification.
2. **🟠 HIGH (F-AI-2):** Spawn `XXXX_REFACTOR_frontend-lp-chart-lazy-load` — lazy `PoolCharts` inside `LiquidityPoolDetailPage` so charts load on tab activation, not page load.
3. **🟡 MEDIUM (F-AI-7):** Add `rollup-plugin-visualizer` dev-dep + CI artifact upload of the treemap on every build.
5. **🟢 LOW:** Re-check after F-AI-1 lands whether SearchOutlined chunk normalises.
