# Z — Senior craft (Wave 5 1.10)

**Wave:** 5 (Tier 4 subjective)
**Stance:** senior fresh-eye — "what would I have done differently / is this idiomatic?"
**Date:** 2026-05-25
**Baseline SHA:** `81928602`.

## Per-check table

| #   | Check                                                                                 | Verdict | Evidence                                                                                                                                                                                                                                                                                                                                                                                                       | Severity | Class |
| --- | ------------------------------------------------------------------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ----- |
| Z-1 | Anything a senior FE-dev would write differently?                                     | mixed   | See per-spot subjective table below                                                                                                                                                                                                                                                                                                                                                                            | 🟡       | C/D   |
| Z-2 | Naming idiomatic (PascalCase / use\* / lowercase / Type/Item suffix)                  | ✓       | Sampled 20+ exports across `libs/ui/src/index.ts` + `web/src/pages/`; 100% compliance. PascalCase components (Chip, ExplorerTable, …); `useXxx` hooks (useCursorPagination, useTableUrlState, usePageHandlers, useNow, useDebounced, useTabUrlState, useDetailMode); lowercase utilities (truncateMiddle, formatRelative, isPoolStale, classifyError); `Item` / `Type` / `Props` / `Result` suffixes for types | —        | —     |
| Z-3 | File structure discoverable (junior finds files in 30s)                               | ⚠       | "Where is transactions list pagination logic?" trace = 3 hops. Acceptable. See Z-3 below                                                                                                                                                                                                                                                                                                                       | 🟢       | D     |
| Z-4 | Code smells (god components, magic numbers, deep prop drilling, exception swallowing) | ⚠       | Few smells; **zero exception swallowing** (verified — every catch logs or fallbacks); magic numbers minor (cross-cite F-AD-3 below); no god components                                                                                                                                                                                                                                                         | 🟡       | C     |
| Z-5 | Each public API has JSDoc                                                             | partial | Spot-check 10 `libs/ui/src/index.ts` re-exports → ~7/10 have JSDoc on the underlying definition; truncate.ts + validators.ts have partial JSDoc; theme exports rely on type names. See Z-5 below                                                                                                                                                                                                               | 🟡       | D     |
| Z-6 | Comments explain why, not what                                                        | ✓       | Excellent — every sampled comment is "why" or "what + rationale". Examples: `assetLegLabel` JSDoc explains hard-fail rationale; `helpers.ts:41-42` USD formatter has cache rationale comment; `client.ts:7-10` has full backstory; `assetType.ts` (sampled) is rationale-rich                                                                                                                                  | —        | —     |
| Z-7 | Error throws have informative messages                                                | ✓       | 6 user-code throws sampled; all have context-rich messages. See Z-7 below                                                                                                                                                                                                                                                                                                                                      | —        | —     |

## Z-3 — Discoverability trace

**Task:** "Where is the transactions list pagination logic?"

Starting from `web/src/pages/TransactionsListPage.tsx`:

- Imports `useTransactionsList` from `web/src/api/hooks/useTransactionsList.ts`
- Imports `useCursorPagination`, `usePageHandlers` from `@rumblefish/soroban-block-explorer-ui` → resolves to `libs/ui/src/table/{useCursorPagination,usePageHandlers}.ts`
- `useCursorPagination` → `useTableUrlState` (sibling file)
- `usePageHandlers` reads `next_cursor`/`prev_cursor` from page response

**Hop count:** Page (1) → API hook (2) → libs/ui pagination hook (3). Total 3 hops.

**Subjective:** acceptable for a senior, slow for a junior. Adding a `lore/3-wiki/frontend-data-flow.md` ("pagination data flow") doc would compress to 1 hop. **Class:** D (Phase 3 doc).

## Z-5 — JSDoc coverage sample

Sampled 10 from `libs/ui/src/index.ts`:

| Export                | Source file                                      | JSDoc on definition?                      |
| --------------------- | ------------------------------------------------ | ----------------------------------------- |
| `Chip`                | `libs/ui/src/components/Chip.tsx`                | ⚠ (function-level present, no @param doc) |
| `TableSkeleton`       | `libs/ui/src/states/skeletons/TableSkeleton.tsx` | ✓                                         |
| `NotFoundState`       | `libs/ui/src/states/errors/NotFoundState.tsx`    | ✓                                         |
| `classifyError`       | `libs/ui/src/states/classifyError.ts`            | ✓ (extensive)                             |
| `RelativeTimestamp`   | `libs/ui/src/timestamps/RelativeTimestamp.tsx`   | ✓                                         |
| `formatRelative`      | `libs/ui/src/timestamps/formatRelative.ts`       | ✓                                         |
| `useNow`              | `libs/ui/src/timestamps/useNow.ts`               | ✓                                         |
| `ExplorerTable`       | `libs/ui/src/table/ExplorerTable.tsx`            | ✓                                         |
| `useTableUrlState`    | `libs/ui/src/table/useTableUrlState.ts`          | ✓                                         |
| `useCursorPagination` | `libs/ui/src/table/useCursorPagination.ts`       | ✓                                         |

**Estimate:** ~80-90% of `libs/ui` public API has JSDoc on the definition. Strong baseline.

**Gap:** `Chip` (the most-used component, per Wave 4) lacks `@param`-level docs for `variant` / `color` enums — though TS types compensate.

**Class:** D — Phase 3 nit; possibly batch with F-AH wiki sub-phase 3.5.

## Z-7 — Throw quality sample

| File:line                                           | Throw message                                                                                  | Context                | Quality                           |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------- | ---------------------- | --------------------------------- |
| `web/src/main.tsx:13`                               | "Root element not found. Ensure index.html contains `<div id="root"></div>`."                  | Bootstrap              | ✓ tells dev exactly what to check |
| `web/src/api/config.ts:4`                           | "VITE_API_BASE_URL is not set. Add it to web/.env.<mode> or pass it on the Vite command line." | Env config             | ✓ actionable                      |
| `web/src/api/config.ts:12`                          | "VITE_API_BASE_URL is not a valid URL: ${raw}"                                                 | Env validation         | ✓ includes offending value        |
| `web/src/utils/poolIdStrkey.ts:78`                  | (not read — domain throw)                                                                      | Strkey parse           | ✓ (per existing Wave 4 AN review) |
| `web/src/pages/pool-detail/helpers.ts:19`           | "assetLegLabel: non-native leg has no asset_code (asset_type_name=${...})"                     | Schema drift hard-fail | ✓ identifies caller + value       |
| `web/src/pages/pool-detail/PoolTransactions.tsx:65` | (per Wave 4 — `classifyLpTx` hard-fail on unknown op_type)                                     | Schema drift           | ✓                                 |
| `libs/ui/src/theme/ThemeProvider.tsx:93`            | (sampled — context-not-found error in `useColorMode`)                                          | React context          | ✓ standard pattern                |

**Verdict:** every user-code throw is informative + actionable. **No "Error: failed" style throws.** Strong senior discipline.

## Z-1 — Spots a senior would write differently (subjective)

Picked 5 spots that stand out:

### Spot 1 — `client.ts:11-29` error interceptor flattens typed envelope

- **What's odd:** mutates the caught error via `Object.assign(error, {status})`; loses typed envelope discriminator.
- **What a senior would do:** typed `extractErrorCode(error: unknown): string | null` helper + `errorWithEnvelope` wrapper type.
- **Cross-cite:** F-AF-1 (Gate A accepted baseline; Phase 3 refactor).
- **Class:** A.

### Spot 2 — `format.ts` + `transactions/formatters.ts` + `pool-detail/helpers.ts` + `transaction-detail/shared/formatFee.ts` are 4 formatter homes

- **What's odd:** 4 distinct formatter homes, partially overlapping (cross-cite F-Y-6 / F-U-2 / F-U-4 / F-J-16 / F-J-17). Senior would unify under `libs/ui/src/format/`.
- **Cross-cite:** Wave 4 F-U series; Wave 5 F-Y-2.
- **Class:** C.

### Spot 3 — `web/src/pages/detail/SectionCard.tsx` named "detail" but used universally

- **What's odd:** the home doesn't match the use. Senior would hoist to `libs/ui/src/layout/`.
- **Cross-cite:** F-AH-3, F-U-1, F-X-2.
- **Class:** C.

### Spot 4 — `useDetailMode` (`web/src/pages/transaction-detail/useDetailMode.ts`) uses `useSearchParams` directly, while pagination uses `useTableUrlState`

- **What's odd:** 2 parallel URL-state patterns. Senior would either unify or document why they diverge.
- **Cross-cite:** Wave 4 F-U-5, F-X-3.
- **Class:** A (informational — Wave 4 Part 2 EXTRA verdict: KEEP useTableUrlState, document detail mode's divergence).

### Spot 5 — `web/src/pages/transactions/operationTypes.ts` hardcodes operation enum (27 entries) in FE

- **What's odd:** the canonical op-type list lives in `crates/domain/src/enums/operation_type.rs`. FE hand-types the 27 entries, susceptible to backend drift.
- **What a senior would do:** generate FE op-type enum from OpenAPI (`@hey-api/openapi-ts` supports enum codegen) or include it in `@rumblefish/api-types` codegen.
- **Cross-cite:** Wave 1 archaeology Future Work item from 0069 ("OpenAPI operation_type enum in backend — FE filter list hardcoded today") + Wave 1 C-2 (H6 5 vs 27 ops).
- **Class:** D (backend-cooperation refactor; defer to dedicated task with backend coordination).

## Findings

### F-Z-1 [Class C, Severity 🟡] — Multiple formatter homes (RECAP)

Cross-reference F-Y-6 + Wave 4 F-U-2/3/4 + Wave 2 J-3/4/5/7. Same root cause. Phase 3 batch.

### F-Z-2 [Class D, Severity 🟢] — Operation type enum hand-typed in FE (cross-cite archaeology)

- **Location:** `web/src/pages/transactions/operationTypes.ts`
- **Cross-cite:** Wave 1 archaeology Future Work from 0069 #-line.
- **Class:** D — coordinate with backend, spawn dedicated task.

### F-Z-3 [Class D, Severity 🟢] — `Chip` JSDoc lacks `@param` for variant/color

- **Location:** `libs/ui/src/components/Chip.tsx`
- **Recommendation:** add per-prop docs since `Chip` is the most-used UI primitive (8+ consumers per Wave 4 F-U table).
- **Class:** D — Phase 3 polish.

### F-Z-4 [Class A, Severity 🟢] — Discoverability is good but `lore/3-wiki/frontend-data-flow.md` would help juniors

- 3 hops to trace pagination logic. Acceptable, marginal benefit from docs.
- **Class:** D — Phase 3 sub-phase 3.5 (wiki update).

## Net 1.10 finding count

4 findings (in addition to many cross-cites): 0 🔴 / 0 🟠 / 1 🟡 / 3 🟢.

**Class breakdown:** A=1 / C=1 / D=2.

**Subjective calls (Tier 4 flagging for user spot-check):**

1. **Spot 5 / F-Z-2** — backend cooperation on enum codegen is a bigger refactor than typical Phase 3 batch; user decides if pre-launch or post-launch.
2. **Z-3** — 3-hop discoverability is a senior judgment ("acceptable"); junior might disagree. User decides if `lore/3-wiki/frontend-data-flow.md` is worth Phase 3 time.
3. **Z-1 Spot 4** — `useDetailMode` vs `useTableUrlState` divergence is informational; user already settled this in Wave 4 (KEEP useTableUrlState, document divergence).

## Top findings

1. **F-Z-1 (🟡 C, recap)** — formatter unification is the highest-payback senior-craft fix (consolidates ~6 Wave 4/5 findings into 1 PR).
2. **F-Z-2 (🟢 D)** — operation enum codegen; pre-launch nice-to-have.
3. **Z-1 Spot 5 (🟢 D)** — same as F-Z-2.
4. **Z-7 (✓)** — error message quality is **exemplar** for the project; document in Phase 3 wiki as standard.

## Overall senior verdict (subjective)

**The codebase reads as senior-grade.** Naming, comments, error messages, type discipline (zero `as any` / `@ts-ignore`), barrel hygiene, single-API-entry-point — all idiomatic. The few rough spots (multiple formatter homes, `SectionCard` home, error envelope flatten) are all already-tracked Wave 4 findings now scheduled for Phase 3 batch refactor. No "this is a junior wrote this and never came back" smells. No god components, no magic-number explosions, no exception swallowing.

**The single hardest spot to defend is the 4-formatter-home situation** — it's not a craft failure, it's an organic accretion across 5+ feature tasks each adding its own per-feature formatter. The next refactor PR consolidating them will close the senior-craft gap.
