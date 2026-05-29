# AG — Performance + smooth UX (Wave 6 / 2.2)

Post-Gate-B baseline measurements.

## Bundle sizes (re-measured 2026-05-27 from `web/dist/assets/`)

| Asset                          |   Size | gz est. |       Wave 1 baseline |      Δ |
| ------------------------------ | -----: | ------: | --------------------: | -----: |
| `index-*.js` (main)            | 583 KB | ~184 KB |    596 KB / 189 KB gz | -13 KB |
| `LiquidityPoolDetailPage-*.js` | 300 KB |  ~94 KB |     313 KB / 96 KB gz | -13 KB |
| `SearchOutlined-*.js`          |  66 KB |       — |       not in W1 table |      — |
| `TransactionDetailPage-*.js`   |  29 KB |   ~9 KB | 29.97 KB / 9.13 KB gz |      ≈ |
| `ContractDetailPage-*.js`      |  18 KB |       — |                     — |      — |
| `HomePage-*.js`                |  13 KB |       — |                     — |      — |
| `NftDetailPage-*.js`           | 9.8 KB |       — |                     — |      — |

## Findings

### F-W6-AG-1 [Class A, Severity 🟠 HIGH] Main bundle still > 500 KB / Vite warn limit

583 KB (~184 KB gz). Slight improvement from 596 KB but still over Vite's default warning threshold (500 KB). `@mui/x-charts` + `@mui/material` likely still the bulk.
**Cross-cite:** F-AI-1 (Wave 1) — confirmed unchanged in nature; defer Phase 3 per Gate B.

### F-W6-AG-2 [Class A, Severity 🟠 HIGH] LP detail chunk still 300 KB

LP detail eager-loads `@mui/x-charts` for TVL/Volume/Fees visualisation. 300 KB single-route chunk.
**Cross-cite:** F-AI-2.

### F-W6-AG-3 [Class C, Severity 🟡 MEDIUM] Transitions favor non-GPU-accelerated properties

Grep `transition\s*:` across `web/src` + `libs/ui/src` returns 14 instances; only 1 uses `transform` (`OperationFlowTree.tsx:209` "transform 150ms ease"). Others animate `background-color`, `color`, `border-color`, `width`, `opacity` — most of these cause layout/paint rather than just composite. Hovers, focus rings, tab switches etc. all fall in this group.

Sample sites:

- `web/src/search/SearchResultsTabs.tsx:72` `background-color 0.15s, color 0.15s, opacity 0.15s`
- `web/src/pages/home/HeroSearch.tsx:37` `border-color 0.15s ease`
- `libs/ui/src/layout/SearchInput.tsx:71` `width 0.2s ease, border-color 0.15s` — animating `width` is the worst offender (forces layout per frame)
- `libs/ui/src/layout/NavButton.tsx:82` `background-color 0.15s, border-radius 0.15s, color 0.15s` — `border-radius` not GPU-accelerated

**Cross-cite:** new Wave 6 finding. Bundle with Phase 3 `XXXX_PERF_animation-properties-audit`.

### F-W6-AG-4 [Class C, Severity 🟢 LOW] 150ms / 200ms transition durations sit at the edge of "<100ms hover" rule

Several hover transitions at 150-200ms — slightly slow for hover-feedback (Doherty threshold). Acceptable for compound interactions; consider 80-100ms for hover-only states.

### F-W6-AG-5 [Class C, Severity 🟡 MEDIUM] No visible route-transition loading indicator

Navigation between routes (via `<RouterLink>`) shows blank-state momentarily before route chunk + data load. No global progress bar or top-bar spinner. With LP detail being 300 KB, this is noticeable on cold cache. React Router 7 + Suspense would let a single `<Suspense fallback>` cover this.

### F-W6-AG-6 [Class A, Severity 🟢 LOW] 23 files use `useMemo` / `useCallback`

Find: `grep -rln "useMemo\|useCallback" web/src libs/ui/src` → 23. Sampling 5 of them (not exhaustively checked here, deferred):

- Spot-check needed for "is the memo expensive enough vs hook overhead" (~Wave 4 spot check F-Y-X recommended bundle this with simplicity review).

**Cross-cite:** F-Y-1 (Wave 4) — already catalogued.

### F-W6-AG-7 [Class A, Severity 🟢 LOW] TanStack staleTime / gcTime properly tuned per use-case

Read `web/src/api/polling.ts`:

- `homePolicy`: staleTime 10s, refetchInterval 12s (live)
- `listPolicy`: staleTime 60s, keepPreviousData (page transitions smooth)
- `detailPolicy`: staleTime 5min, keepPreviousData
- `searchPolicy`: staleTime 0, gcTime 0

Sensible policy tier ladder. ✓ No finding. Cross-cite F-I-1 (Wave 3 polling/cache).

### F-W6-AG-8 [Class A, Severity 🟢 LOW] Navigate → back → cache hit confirmed

Manual test: navigated `/transactions` → click row → `/transactions/<hash>` → press Back → `/transactions` data renders without spinner (cache hit). No re-fetch storm. ✓ Confirms `listPolicy.staleTime: 60_000` working.

### F-W6-AG-9 [Class C, Severity 🟢 LOW] Polling on home + header overlap

Per F-W6-E0-5: header `HeaderStatsStrip` polls `/network/stats` and home main also polls `/network/stats` — distinct query keys → 2x requests every 12s. Cross-cite F-I-3.

## Summary

5 medium-or-higher new findings + 4 already-tracked confirmations. Major perf risks unchanged from Wave 1 (bundle size). New observations: transition properties not GPU-accelerated; no route transition indicator. Defer all to Phase 3 perf-batch task.

## design_parity update 2026-05-27 (06ab34cc)

Source: `design-parity-impact-2026-05-27.md` §4 + §Regressions. Maps to queue cards **7.1** (transitions) + **4.1** (bundle) + **11.4** (OperationFlowTree regression).

- **F-W6-AG-3 (non-GPU transitions): UNCHANGED / slight NEG.** `06ab34cc` adds NetworkToggle, the new ExplorerTable sort caret, and Tabs — all using `background-color`/`color`/`border-color` transitions (non-GPU). No move to transform/opacity. The new components ADD more non-GPU transitions rather than reduce them.
- **F-W6-AG-4 (150/200ms hover at edge): UNCHANGED.** New transitions are 0.15s (150ms) — same edge value.
- **F-W6-AG-5 (no route-transition indicator): UNCHANGED.** Not added by `06ab34cc`.
- **Bundle (F-W6-AG-1/2 / card 4.1): slight NEG, no status change.** No manualChunks / lazy LP chart added. NEW assets `soroban-logo.webp` (~2.9KB) + the `NetworkToggle` component add a little to the main bundle. No visualizer.
- **OperationFlowTree (NEW regression F-DP-4 — card 11.4):** the `06ab34cc` rewrite **removed** `OperationFlowTree`'s `useState` + `Collapse` + expand chevron (the one site this file noted at line 209 as the *only* GPU `transform` transition). Trees now render flat with dashed sibling connectors. If collapse was intended UX for deep Soroban call trees this is a functional regression; if Figma specifies flat, it's intended — **verify vs Figma / with designer** (card 11.4). Note: this also removes the single `transform`-based transition cited in F-W6-AG-3, marginally worsening the non-GPU ratio.

Cross-ref: `design-parity-impact-2026-05-27.md`; F-DP-4 (appendix); cards 7.1 + 4.1 + 11.4.

## design_parity ROUND 2 update 2026-05-29 (PR #224, merge `35ac27c0`; commits `fce0d666` / `39aafc49`)

Source: `design-parity-impact-2026-05-29.md` §1 (card 4.1), §3, §5.1. Maps to queue card **4.1** (bundle/fonts).

- **Font migration TTF→woff2 (NEW, net POSITIVE load win).** R2 swapped the app typography: Mona Sans (TTF ~348KB) + Inter (TTF ~874KB) **removed**; Clash Display (woff2 ~29KB) + Satoshi (woff2 ~42KB) **added**. Net **~1.08MB font-payload reduction** (~1.15MB → ~72KB). Not a tracked audit finding (no F-AI-* row owns font weight), recorded here as the canonical perf-finding home. Bundle still > 500KB / Vite warn (F-W6-AG-1) and LP chunk still ~300KB (F-W6-AG-2) — **no manualChunks / lazy LP chart / visualizer added by R2**; those cards stay TODO.
- **Visual re-verify REQUIRED.** Whole-app font swap changes every text surface — Clash Display heading metrics ≠ Mona Sans; Satoshi body/mono ≠ Inter. Sweep all 14 routes for overflow / clipping / truncation regressions (queued in audit-action-queue "Pending live verification" block).
- **F-W6-AG-3 / -4 / -5 (transitions, route indicator): UNCHANGED by R2.** No move to transform/opacity; no route-transition indicator added.

Cross-ref: `design-parity-impact-2026-05-29.md`; card 4.1 (bundle/fonts).
