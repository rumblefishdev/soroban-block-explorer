# 1.11 P — Code quality

Date: 2026-05-25
Tools: `nx run-many -t typecheck`, `nx run-many -t lint`, grep.

## Tool runs

| Target                                 | Projects                                                                                                                 | Result                                                                        | Exit |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------- | ---- |
| `typecheck`                            | `@rumblefish/api-types`, `@rumblefish/soroban-block-explorer-ui`, `@rumblefish/soroban-block-explorer-web`, plus aws-cdk | `Successfully ran target typecheck for 4 projects and 2 tasks they depend on` | 0    |
| `lint` (JS subset: web, ui, api-types) | 3 projects                                                                                                               | `1 problem (0 errors, 1 warning)`                                             | 0    |
| `lint` (full incl. Rust)               | 5 projects                                                                                                               | green, 1 warning                                                              | 0    |
| `knip`                                 | —                                                                                                                        | **not installed**                                                             | n/a  |
| `ts-prune`                             | —                                                                                                                        | **not installed**                                                             | n/a  |

## Findings

### F-P-1 — Lint warning: forbidden non-null assertion — 🟡 MEDIUM

`web/src/pages/liquidity-pools/assetColor.ts:131:10` — rule
`@typescript-eslint/no-non-null-assertion`.

```ts
return FALLBACK_PALETTE[hash(legKey(leg)) % FALLBACK_PALETTE.length]!;
```

The `!` bypasses TS's `noUncheckedIndexedAccess` semantics (which the
project does NOT enable — see AQ findings). With a non-empty
`FALLBACK_PALETTE`, this is correct at runtime — but the rule fires
because the assertion silently breaks if the palette ever becomes empty.

→ Fix: either ensure `FALLBACK_PALETTE` is typed as a non-empty tuple
(`readonly [AssetColor, ...AssetColor[]]`) so `[0]` is inferred
non-undefined, or destructure with explicit fallback:
`return picked ?? FALLBACK_PALETTE[0];` (which then needs a `[0]`-known type).
**1 line fix.**

### F-P-2 — No dead-export detection in CI — 🟡 MEDIUM

Neither `knip` nor `ts-prune` nor `ts-unused-exports` is configured. TS
`noUnusedLocals: true` catches unused locals but NOT unused exports
between modules. Dead code accumulation risk grows as the project ages.

`grep` heuristic for "exported but never imported" not run (would
require AST-aware tool). Recommend adding `knip` to CI with a baseline
ignore-list to gate new dead code.

### F-P-3 — Zero `console.*` calls — ✅

`grep -rnE "console\.\w+" web/src libs/ui/src` → **0 hits**. No
production logging or leftover debug. Clean.

### F-P-4 — Zero literal TODO/FIXME/XXX/HACK markers — ✅ (but see archaeology)

Per archaeology audit: 0 marker hits, but 28 documented deferrals live
in task bodies. This is policy success or shadow-debt depending on
view — code is clean, lore carries the can.

### F-P-5 — Zero commented-out code blocks — ✅

Heuristic: lines starting with `//` consecutive 8+. **0 hits** across
`web/src` + `libs/ui/src`. Clean.

### F-P-6 — Cyclical imports not checked — 🟢 LOW

No `madge`/`dependency-cruiser` in deps. `nx graph` analysis not run
(heavy). Nx's TS project references already prevent cross-project cycles;
intra-project cycles undetected. Recommend `madge --circular` one-off
sanity check or `nx report` enhancement.

### F-P-7 — Longest file = `libs/ui/src/theme/overrides.ts` (867 LOC) — 🟢 LOW

| File                                              | LOC     |
| ------------------------------------------------- | ------- |
| `libs/ui/src/theme/overrides.ts`                  | **867** |
| `web/src/pages/pool-detail/PoolCharts.tsx`        | 268     |
| `libs/ui/src/visualization/OperationFlowTree.tsx` | 263     |
| `libs/ui/src/layout/SearchInput.tsx`              | 246     |
| `web/src/pages/contracts/ContractEvents.tsx`      | 233     |
| `libs/ui/src/theme/types.ts`                      | 228     |
| `libs/ui/src/visualization/TimeSeriesChart.tsx`   | 223     |

`overrides.ts` is the MUI theme component-override registry — 867 lines
of one giant config object. Likely splittable per component family
(`MuiTable*`, `MuiButton*`, `MuiCard*`, etc.). Re-audit in 1.9c
Simplicity. Not blocking, but a friction point for visual changes.

### F-P-8 — No production-bundle console-leak check in CI — 🟢 LOW

`vite-bundle-visualizer` not in CI; no explicit grep for `console.log`
in the built bundle. Mitigated by zero `console.*` in source — but a
dep could introduce it. Worth a CI grep on built `dist/**/*.js`.

## Conclusion

Code quality is **strong**. One actionable lint warning, no
console/debug leakage, no dead-marker code. Two infra gaps worth filling
pre-launch: dead-export detection (`knip` in CI) and a built-bundle
console-leak grep.

## Recommendations

1. **🟡 MEDIUM (F-P-1):** Fix the 1 lint warning in `assetColor.ts:131`
   inline (1-line PR).
2. **🟡 MEDIUM (F-P-2):** Spawn `XXXX_FEATURE_frontend-knip-dead-export-ci`
   — add `knip` with baseline + CI gate.
3. **🟢 LOW (F-P-7):** Audit `libs/ui/src/theme/overrides.ts` for split
   opportunity (cluster in 1.9c).
4. **🟢 LOW (F-P-8):** Add `grep -r "console\." web/dist/assets/*.js`
   step to CI build job.

## Post-merge update 2026-05-25 — develop @ 6b7fb558 (FilipDz tx-detail PR #215)

Re-ran `nx run-many -t typecheck` + `nx run-many -t lint` against
post-merge tree.

| Target                            | Result                                      | Δ vs Wave 1  |
| --------------------------------- | ------------------------------------------- | ------------ |
| `typecheck` (4 projects + 2 deps) | exit 0, "Successfully ran target typecheck" | 0 new errors |
| `lint` (JS subset + Rust)         | exit 0, **1 problem (0 errors, 1 warning)** | 0 new        |

**F-P-1 (🟡 MEDIUM — lint warning at `assetColor.ts:131`):** STILL STANDS,
unchanged. The 1 warning still fires at the same site. Filip's PR added
~1799 LOC across new tx-detail files with **zero new errors and zero new
warnings**. Lint discipline holds.

**F-P-2, F-P-3, F-P-4, F-P-5, F-P-6, F-P-8:** STILL STAND (no infra
changes).

**F-P-7 (longest file):** Filip's longest new file =
`web/src/pages/transaction-detail/sections/OperationPicker.tsx` 204 LOC
(below the 233-LOC `ContractEvents` baseline). New normal/toFlowNodes.tsx
= 206 LOC. Neither crosses the 250-LOC threshold. Baseline list unchanged.

**Code-quality conclusion:** Filip's PR is clean by lint+tsc baseline.
No new P findings.

## Gate B merge resolution 2026-05-26 — develop @ cdb0c81d (PR #219)

### F-P-1 — **STILL STANDS** (verified post-merge)

`web/src/pages/liquidity-pools/assetColor.ts:131` non-null assertion (`FALLBACK_PALETTE[hash(legKey(leg)) % FALLBACK_PALETTE.length]!`) NOT touched by Gate B batch despite `PoolsTable.tsx` (same directory) being heavily modified for F-K-2 link wraps. Lint baseline remains: **0 errors, 1 warning** at the same file:line.

Fix candidate: `noUncheckedIndexedAccess` flag (F-AQ-1) would catch this at type level; alternative inline fix = explicit length check or `?? fallback` ternary. Phase 3 code-quality cluster candidate.

### F-P-2 through F-P-8 — **STATUS UNCHANGED**

Other Wave 1 P findings (eslint warning count, `console.log` leftover sweep, dead code) not addressed by Gate B batch. Defer to Phase 3 code-quality cluster.
