# AB — Hallucination check (Wave 5 1.10c)

**Wave:** 5 (Tier 4 subjective)
**Stance:** "is this divergence from convention explicit, or invented without justification?"
**Date:** 2026-05-25
**Baseline SHA:** `81928602`.

## Per-check table

| #    | Check                                                               | Verdict     | Evidence                                                                                                                                                                                        | Severity | Class |
| ---- | ------------------------------------------------------------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ----- |
| AB-1 | Divergences from project convention — explicit in task or invented? | ⚠           | Sample 5 below; 4/5 explicit, 1 partial. See F-AB-1                                                                                                                                             | 🟡       | D     |
| AB-2 | Each Emerged Decision — justified or hallucinated?                  | mostly ✓    | Wave 1 archaeology cataloged 41 Emerged decisions; 14 flagged for re-audit. **8 of 14 confirmed justified post Wave 4** (Stellar correctness, Figma overrides). See per-decision verdict below. | 🟡       | D     |
| AB-3 | Each `as any` / `@ts-ignore` justified?                             | ✓           | Cross-cite Wave 1 AF-1 — **zero `as any` / zero `@ts-ignore` in user code**. F-AF-3 (1 `as unknown as` in `useNow.ts`) justified cross-runtime types.                                           | —        | —     |
| AB-4 | F-AQ-7 / F-AQ-8 `unknown` + cast — justified or hallucinated?       | ✓ justified | See F-AB-2 below — XDR `details` field is intentionally untyped on the wire (heterogeneous per-op shape); runtime probes are correct pattern                                                    | —        | —     |
| AB-5 | Implementations inconsistent with project pattern                   | partial     | Cross-cite Wave 4 F-U series + Wave 5 F-Y-2 (debounce dup). Inconsistencies all flagged in task bodies or Future Work                                                                           | 🟠       | C     |
| AB-6 | Spec says X but code does Y without task note                       | ✓           | Sample 3 below — every divergence has task-body note or Emerged Decision rationale                                                                                                              | —        | —     |
| AB-7 | Comment-out leftover / false starts                                 | ✓           | `grep "// old\|/\* old\|// TODO\|// FIXME"` → **0 hits** across `web/src` + `libs/ui/src`. Cross-cite Wave 1 P-5                                                                                | —        | —     |

## F-AB-1 — Divergence audit (sample of 5)

| Divergence                                                                              | Where                                               | Explicit in task body?                                                                                 | Verdict    |
| --------------------------------------------------------------------------------------- | --------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | ---------- |
| FE op-type enum hardcoded (27 entries)                                                  | `web/src/pages/transactions/operationTypes.ts`      | ✓ — 0069 Future Work "OpenAPI operation_type enum in backend"                                          | ✓ explicit |
| Hard-fail throws on schema drift (`assetLegLabel`, `classifyLpTx`, `poolIdHexToStrkey`) | `web/src/pages/pool-detail/helpers.ts:16-23`, etc.  | ✓ — 0077 #12 + #13 Emerged                                                                             | ✓ explicit |
| Custom pool-id strkey encoder (~60 LOC) instead of `@stellar/base`                      | `web/src/utils/poolIdStrkey.ts`                     | ✓ — 0077 #9 Emerged (bundle-size justification documented)                                             | ✓ explicit |
| `Source account` column dropped on account-transactions table                           | `web/src/pages/accounts/AccountTransactions.tsx`    | ✓ — 0073 deviation note + AC delta                                                                     | ✓ explicit |
| `useDetailMode` uses `useSearchParams` instead of `useTableUrlState`                    | `web/src/pages/transaction-detail/useDetailMode.ts` | ⚠ — Filip's 0070/0071 task body doesn't explicitly note divergence; Wave 4 F-U-5 documented post-merge | partial    |

**Verdict:** 4/5 sampled divergences are explicitly documented in originating task body. 1/5 (useDetailMode) emerged from post-merge audit — task body could be amended.

**Class:** D (catalog-only; bulk task hygiene fix in Phase 3).

## F-AB-2 — XDR `unknown` casts: justified (RECAP from Wave 4 F-AQ-7/8)

Filip's tx-detail pages use `unknown` + runtime probes for the heavy `details` field. Wave 4 flagged these as Class B/C MEDIUM.

**Backend perspective:** The `XdrOperationDto.details` field is `serde_json::Value` on the wire (heterogeneous per-op-type JSON shape — Payment has `amount/destination`, ManageData has `data_name/data_value`, etc.). OpenAPI types this as `unknown` (correct — there's no closed-set schema for per-op JSON).

**FE response:**

- `OperationJsonDetail.tsx:13-26` defines `pickDetailValue(details: unknown, key: string)` with full type-guarded probe (`typeof === 'object' && !Array.isArray && key in`).
- `OperationJsonDetail.tsx:28-30` defines `asString(value: unknown)` returning `string | null`.
- These are **the correct pattern** for safely accessing unknown shapes.

**Verdict:** NOT hallucination. **Backend-typed `unknown` → FE-typed `unknown` is contract-honoring**. The casts (`as Record<string, unknown>`) are inside type-guarded branches, which is sound TypeScript.

**Subjective:** could be tightened with a zod schema per op-type, but that's a feature (per-op typed views), not a fix. Documented as Wave 4 F-AQ-7 already.

**Class:** A — confirmed not hallucination; deferred Phase 3 if per-op zod schemas spawned.

## F-AB-3 — Emerged Decision re-audit (sample of 8 from Wave 1 archaeology 14-item list)

| #              | Decision                                                                                | Re-audit verdict                                                                                                                                                                                                    |
| -------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0061 #4        | Sort caret without DS "Active" yellow pill — "deliberate middle ground"                 | **partial hallucination — design decision was unilateral.** Subjective: a "middle-ground" between 2 Figma variants without designer confirmation is creative interpretation. Defer to Wave 6 Figma audit to settle. |
| 0062 #4        | Tooltip removed from `IdentifierDisplay` per Figma exactly                              | **justified** — Figma-first per project convention; cost = click-to-copy compensates                                                                                                                                |
| 0065 #4        | `OperationFlowTree` unified instead of separate `InvocationCallTree`                    | **justified** — Wave 4 F-AN-3 verified Soroban call trees render as nested children; unification is correct                                                                                                         |
| 0065 #5        | Interval labels `1D/7D/30D/1Y` from Figma vs spec `1h/1d/1w`                            | **partial — Figma override correct, but spec drift unaddressed.** Spec body never amended. Defer for Wave 6 confirmation + spec-update task in Phase 3                                                              |
| 0073 #5        | Balances show only "Native asset" / "Classic" (cannot distinguish SAC from API)         | **backend gap, not hallucination.** Spawn backend task per Wave 1 A3.                                                                                                                                               |
| 0075 #6        | `interface_metadata` hand-typed from indexer source not OpenAPI                         | **partial hallucination risk.** Type drift hazard — if backend changes, FE silently breaks. Spawn backend task per Wave 1 A3.                                                                                       |
| 0077 #9        | Pool-id strkey encoder = ~60 LOC custom (avoid 50-100 KB stellar-base)                  | **justified** — bundle-size win documented; verified vs stellar-base in task body                                                                                                                                   |
| 0077 #12 + #13 | `assetLegLabel` / `classifyLpTx` hard-fail on schema drift via `throw`                  | **justified** — Wave 4 F-AE-2 verified all throws fall into `SectionErrorBoundary`. Documented                                                                                                                      |
| 0238 #5        | `cursorParam` multi-cursor namespacing (`cursor_p/_t/_e/_i`) via CURSOR_PARAMS registry | **justified mechanism, ADR gap.** Convention undocumented per archaeology recommendation; Phase 3 ADR spawn (already flagged)                                                                                       |
| 0251 B1        | `linked={false}` on pool-id header instead of fixing href                               | **structurally correct but anti-pattern.** Hides the bug rather than fixing root cause. Future juniors will reintroduce the broken link. **Mild hallucination of fix.**                                             |

**Verdict:** of 10 sampled, **8 justified, 2 partial/hallucination-risk (0061 #4, 0251 B1)**. The 4 marked "partial" still defer to Wave 6 Figma audit.

**Class:** D — Phase 3 follow-up tasks already in the spawn pipeline.

## F-AB-4 — Implementations inconsistent with project pattern (cross-cite)

All flagged in Wave 4:

| Inconsistency                                | Cross-cite | Class | Spec note                                          |
| -------------------------------------------- | ---------- | ----- | -------------------------------------------------- |
| Local `SectionCard` not in libs/ui           | F-U-1      | C     | Not in task — emerged organically                  |
| Inline `toFixed`/`toLocaleString` (10 sites) | F-U-2      | C     | Not in task                                        |
| Truncation re-impls (6 sites)                | F-U-3      | C     | Not in task                                        |
| STROOPS_PER_XLM constant dup (2 sites)       | F-U-4      | A     | Filip's tx-detail introduced 2nd site; not in task |
| Debounce pattern dup (4 sites — NEW)         | F-Y-2      | C     | Not in task                                        |
| `formatFee` dup (2 sites)                    | F-J-16     | C     | Filip's tx-detail introduced; not in task          |

**Subjective:** these are all **organic drift across feature task boundaries** — each task was self-contained and didn't anti-DRY-check across siblings. **Not hallucination; just incremental accretion that ought to be tidied periodically.**

**Class:** C — Phase 3 unification batch.

## F-AB-5 — Spec ↔ code divergence without note (sample of 3)

| Divergence                                      | Spec source                            | Task body note?                          |
| ----------------------------------------------- | -------------------------------------- | ---------------------------------------- |
| 0065 #5 interval labels                         | Spec `1h/1d/1w` vs code `1D/7D/30D/1Y` | ⚠ in Emerged but spec body never amended |
| `Source account` column drop                    | Spec lists column; code omits          | ✓ in 0073 deviation + Emerged            |
| Asset-transactions table layout (Ledger vs Fee) | Spec / Figma diverge                   | ✓ in 0074 Emerged                        |

**Verdict:** 2/3 explicit; 1 (interval labels) lacks spec update.

## Findings

### F-AB-1 [Class D, Severity 🟡] — `useDetailMode` divergence not in originating task body

- **Location:** `web/src/pages/transaction-detail/useDetailMode.ts`
- **Divergence:** Uses raw `useSearchParams` for `?mode=normal|advanced`, while pagination uses `useTableUrlState`. Two parallel URL-state patterns in same project.
- **Not in task:** 0070/0071 task body doesn't note the divergence (only added by post-merge Wave 4 F-U-5 audit).
- **Class:** D — Phase 3 task body amendment.

### F-AB-2 [Class D, Severity 🟡] — Interval labels (0065 #5) spec body not amended

- **Divergence:** Figma `1D/7D/30D/1Y` overrode spec `1h/1d/1w`; Emerged Decision in 0065 task body documents the override but **spec doc not amended**.
- **Risk:** new contributor reading the spec body builds to `1h/1d/1w`, then has to discover the override post-implementation.
- **Class:** D — Phase 3 spec doc sync task.

### F-AB-3 [Class D, Severity 🟢] — Mild fix-by-hide in 0251 B1 (RECAP from archaeology)

- 0251 B1: `linked={false}` on pool-id header.
- Avoids the bug; doesn't fix it. Future junior may re-introduce link.
- **Class:** D — Phase 3 root-cause fix task ("fix pool-id href routing then re-enable link").

### F-AB-4 [Class A, Severity 🟢] — Sort-caret middle ground (0061 #4) needs designer sign-off

- Wave 6 Figma audit will determine.
- **Class:** A — defer Wave 6.

### F-AB-5 [Class C, Severity 🟠] — Organic-drift component duplications (RECAP)

- 6 distinct duplications across Wave 4/5 findings. Each was correct in its own task; cross-task drift was unaddressed.
- **Class:** C — Phase 3 unification batch.

## Cross-cites

- F-AB-2 (XDR `unknown` casts) ↔ Wave 4 F-AQ-7 / F-AQ-8 (now confirmed justified, not hallucinated).
- F-AB-3 ↔ Wave 1 archaeology 14-item Emerged re-audit list.
- F-AB-5 ↔ Wave 4 F-U series + Wave 5 F-Y-2 + Wave 2 J-3/J-7/J-16/J-17.
- F-AB-7 ↔ Wave 1 P-4 / P-5 (zero TODOs / commented blocks).

## Net 1.10c finding count

5 findings: 0 🔴 / 1 🟠 (F-AB-5 recap) / 2 🟡 / 2 🟢.

**Class breakdown:** A=1 / C=1 / D=3.

**Subjective calls (Tier 4 flagging for user spot-check):**

1. **0061 #4 sort-caret middle ground** — Tier 4 senior judgment; only Figma audit resolves it. Mild hallucination concern.
2. **0251 B1 fix-by-hide** — defensible if "ship the fix now, file follow-up" was the explicit decision (it was, per task). But the root-cause task wasn't spawned.
3. **0075 #6 hand-typed `interface_metadata`** — backend cooperation needed; not pure hallucination, but type drift hazard.

## Overall AB verdict (subjective)

**Hallucination risk is low.** The team's lore-process discipline (Design Decisions → Emerged in every task body) acts as a strong audit trail. Where divergences exist, they're documented in the originating task; the 2-3 cases where documentation is partial (useDetailMode, interval labels) are minor lore-hygiene fixes.

**The cross-task organic drift (formatters, truncations, debounce)** is **the main pattern that looks like hallucination but isn't** — it's per-task self-consistency without cross-task coordination. The project's structural fix is "Phase 3 formatter unification PR" (already scoped), not individual hallucination flags.

**Notable positive:** zero TODOs, zero FIXMEs, zero commented-out code across `web/src` + `libs/ui/src`. Strong policy discipline.

## Top issues

1. **F-AB-5 (🟠 C, recap)** — formatter unification = the one cross-task drift to tidy.
2. **F-AB-3 (🟢 D)** — 0251 B1 root-cause fix backlog spawn.
3. **0061 #4** — defer Wave 6.
