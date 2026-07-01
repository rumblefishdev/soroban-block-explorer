# Gate B — Triage Decisions

**Date:** 2026-05-25
**Stage:** end of Wave 5 (all 5 Tier 1-4 sub-phases of Track 1 complete)
**Findings inventoried:** ~156 actionable across 24 files in `findings/`
**Cumulative severity:** 2 🔴 / 31 🟠 / 63 🟡 / 60 🟢

## Scope of this gate

Per task README "Triage gates" section:

- **Class A (baseline-breaker)** that affects Wave 6 measurements — fix-first or accept baseline + document
- **Class B (routing/contract)** that Wave 6 Playwright 2.0 will re-hit — fix-first to avoid duplicate findings; defer to Phase 3 only if change wouldn't alter what Wave 6 measures
- **Class C (visual/layout)** that affects Wave 6 Figma fidelity (2.1) or responsive (2.4) or a11y (2.5) — fix-first; defer if visual finding is its own thing Wave 6 wouldn't double-record
- **Pre-launch must-fixes** (legal/discoverability/dead-link surface) — fix-first regardless of audit invalidation, since user is staring down the launch
- **Class D (catalog-only)** — defer Phase 3 bulk-spawn (3.2), no Gate B action
- **Class E (off-band)** — separate immediate fix track, not gate-blocking

## Wave 6 sub-phases at risk

Wave 6 runs:

1. **2.0 Playwright MCP full re-pass** — 14 routes, all states; will re-hit every link target + every error path + every console log
2. **2.1 Figma fidelity** — pixel-perfect 1:1 per view (no time-box per user); reimplemented component changes its rendered DOM → Figma fidelity finding may flip post-fix
3. **2.2 + 2.2b Performance + Loading patterns** — bundle + render perf measurement; depends on current state
4. **2.3 V Live indicator** — already known broken (DM-1)
5. **2.4 Responsive 14×3 = 42 cells** — responsive layout measurement; depends on current DOM
6. **2.5 F+CH A11y** — keyboard + screen reader + color contrast + color blindness
7. **2.6 AK CSS theme** — token usage consistency

A finding becomes Gate B fix-first only if it changes what Wave 6 measures **or** is a pre-launch must-fix the team would not ship.

---

## Decision matrix — by finding

### Class A baseline-breakers — remaining (8)

| ID                              | Finding                                                    | Wave 6 impact                                                           | Decision                        | Rationale                                                                                                                                                                                                                                  |
| ------------------------------- | ---------------------------------------------------------- | ----------------------------------------------------------------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| F-AQ-1 🟠                       | `noUncheckedIndexedAccess` off in tsconfig                 | None for Wave 6 (visual + UX, not type-system)                          | **DEFER Phase 3 refactor task** | Per Gate A: do NOT toggle mid-audit (regenerates tsc errors, unwinds 1.11/1.11b baseline). Spawn dedicated Phase 3 task: enable flag + fix new errors + separate PR.                                                                       |
| F-AQ-4 🟠                       | Zero branded ID types                                      | None for Wave 6                                                         | **DEFER Phase 3 refactor task** | Type-system refactor; Wave 6 doesn't measure type-level concerns. Bundle with F-AQ-7/8 in Phase 3 type-safety hardening batch.                                                                                                             |
| F-AQ-7 + F-AQ-8 🟡              | Heavy XDR `details` as `unknown` + `results_meta_xdr` cast | None for Wave 6 (Tx detail renders, audit doesn't second-guess types)   | **DEFER Phase 3**               | OpenAPI codegen gap — needs discriminated union per `op_type` for `details` + schema fix for `results_meta_xdr`. New task with `related_tasks: ['0257', '0070', '0071']`.                                                                  |
| F-AI-1 🟠                       | Main bundle 596KB / 189KB gz (>500KB Vite warn)            | Wave 6 2.2 perf will measure this; fix-first means measurement post-fix | **DEFER Phase 3 perf task**     | Wave 6 2.2 records current baseline as "measured" finding; Phase 3 spawns `XXXX_PERF_bundle-size-reduction` with concrete target. Fix-first not justified because the audit value of 2.2 is capturing exact numbers, not validating fixes. |
| F-AI-2 🟠                       | LP detail chunk 307KB / 94KB gz                            | Same as F-AI-1                                                          | **DEFER Phase 3 perf task**     | Same logic. Bundles with F-AI-1 + F-AI-10.                                                                                                                                                                                                 |
| F-AI-10 🟡                      | E3 (TxDetail) chunk 29.97KB / 9.13KB gz                    | Same                                                                    | **DEFER Phase 3 perf task**     | Bundles.                                                                                                                                                                                                                                   |
| F-AH (Wave 5) — 8 Class A items | File/folder structure baseline drifts                      | None for Wave 6 (visual)                                                | **DEFER Phase 3 batch**         | Convention-level — Wave 6 doesn't measure file organization. Bundles into `XXXX_REFACTOR_folder-structure-rationalization`.                                                                                                                |

### Class B routing/contract — remaining (10)

| ID                    | Finding                                                                                                                                              | Wave 6 impact                                                                                                                                                                                                                                                        | Decision                                                                                                                                                                                         | Rationale                                                                                         |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| **F-D-2 + F-AE-5** 🟠 | Composite NotFound on E6/E8/E9 — sub-section queries fire alongside parent 404 → dual error blocks (E9 contract NotFound = 4 error blocks)           | **2.0 Playwright** will re-record dual error blocks on every E6/E8/E9 NotFound probe (visual mess). **2.1 Figma** will compare to Figma single-error mockup → fidelity finding triggers. **2.5 a11y** screen-reader will announce 4 error blocks consecutively on E9 | **FIX-FIRST** ✅ Spawn `audit-blocker` task. Single-pattern fix: gate sub-section queries on parent 404 status (don't fire if parent NotFound). Reduces Wave 6 Track 2 noise; pre-launch UX win. |
| **F-K-2 + F-K-3** 🟠  | Pool detail reserve labels (USDCOIN/EUR) no `<a href="/assets/:id">`; participants "Since ledger" no `<a href="/ledgers/X">`                         | **2.0 Playwright** will re-walk pool detail cross-entity links → re-report same. **2.5 a11y** will flag non-link text styled as link (color cue without semantic).                                                                                                   | **FIX-FIRST** ✅ Spawn `audit-blocker` task. Wrap labels in router `<Link to={...}>`. Cheap fix (~30 LOC), high Wave 6 noise reduction.                                                          |
| **F-L-1 + F-K-4** 🟠  | Pool strkey (`L...`) search → 0 results across 6 tabs; empty-state hint omits `L...` from supported formats. Same strkey ↔ hex canonicalisation root | **2.0 Playwright** search re-pass will hit identical 0 results. Search is on Wave 6 1.14 already in scope as full e2e revisit.                                                                                                                                       | **FIX-FIRST** ✅ Spawn `audit-blocker` task. Single shared "strkey canonicalisation" helper used at search-input boundary + empty-state hint update. Mid-sized fix (~100 LOC).                   |
| F-E-3 🟡              | Catch-all 404 `<main>` landmark gap                                                                                                                  | 2.5 a11y will flag landmark                                                                                                                                                                                                                                          | **DEFER Phase 3**                                                                                                                                                                                | Pure a11y; Wave 6 2.5 will catch and consolidate with other landmark findings. Cheaper to bundle. |
| F-E-7 🟡              | URL state edge case (tab restoration cross-refresh)                                                                                                  | 2.0 will re-test                                                                                                                                                                                                                                                     | **DEFER Phase 3**                                                                                                                                                                                | Pure UX; bundle with state-separation refactor task.                                              |
| F-I-3 🟡              | Polling cache logic gap (TanStack dedup misses across same query key with different transform)                                                       | None for Wave 6 (perf-adjacent but not measured visually)                                                                                                                                                                                                            | **DEFER Phase 3**                                                                                                                                                                                | Cache hygiene; Phase 3 dedicated task.                                                            |
| C-5 🟠                | Missing `isAssetId` / `isNftId` validator                                                                                                            | None directly for Wave 6                                                                                                                                                                                                                                             | **DEFER Phase 3**                                                                                                                                                                                | Type-system; bundle with F-AQ-4 branded types task.                                               |
| F-K-7 + F-K-8 🟡      | E3 outbound link verification (deferred to Wave 4 delta)                                                                                             | Already absorbed in Wave 4 delta-audit per earlier scope; remaining work bundles into Wave 6 2.0 anyway                                                                                                                                                              | **DEFER Phase 3 (delta-absorbed)**                                                                                                                                                               | Wave 6 2.0 will re-walk E3 outbound links; no fix-first needed.                                   |

### Class C visual/layout — remaining (12)

| ID                                 | Finding                                                                                                                 | Wave 6 impact                                                                                                                                                                                                                                                                                                    | Decision                                                                                                                                                                                                                                                                 | Rationale                                                                                                                                                                                                                                                           |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **CA-1 + CA-2** 🟠                 | Footer Terms/Privacy/Cookies + Resources (GitHub/Stellar docs/Soroban docs/Stellar dashboard) dead `<span>` (no `href`) | **2.0 Playwright** would record dead links on every page footer (= 14× same finding). **2.1 Figma fidelity** comparison to mockup with hrefs. **PLUS pre-launch legal must-fix** — shipping a public block explorer with non-functional Terms/Privacy is a legal/compliance liability regardless of audit scope. | **FIX-FIRST** ✅ Spawn `audit-blocker` task. **Pre-launch must-fix priority** independent of audit invalidation. Either fill in real hrefs (legal-approved Terms + Privacy URLs from team, Resources external URLs) or hide the dead links entirely until content ready. |
| DM-1 🟠                            | Footer hardcoded "All systems operational" (no health probe)                                                            | **2.3 V** live indicator subphase will independently confirm this. Wave 6 doesn't need DM-1 fixed to measure correctly — the audit value of 2.3 IS measuring "indicator is hardcoded, not connected."                                                                                                            | **ACCEPT BASELINE + DEFER Phase 3**                                                                                                                                                                                                                                      | Confirms 2.3 V finding twice = OK, single Phase 3 task spawned: `XXXX_FEATURE_footer-status-health-probe` (wires `/health` endpoint poll). Per Gate A decision stands.                                                                                              |
| J-3 🟡                             | TopNav.formatNumber duplicate                                                                                           | None for Wave 6 (rendering correct, dup is code-level)                                                                                                                                                                                                                                                           | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle with `XXXX_REFACTOR_format-truncate-unification`.                                                                                                                                                                                                            |
| J-5 🟡                             | Timestamp depth inconsistency                                                                                           | 2.1 Figma may flag if Figma specifies single timestamp depth; otherwise no                                                                                                                                                                                                                                       | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle with formatter batch. If Wave 6 2.1 finds Figma divergence, will cross-cite J-5.                                                                                                                                                                             |
| **J-7 🟠 (escalated from Wave 1)** | Truncation re-impls now 6× across pages (head/tail drift 4/4, 5/4, 6/4, 10/10, 12/12)                                   | **2.1 Figma fidelity** likely flags inconsistent truncation across pages — single Figma source-of-truth specifies one truncation depth                                                                                                                                                                           | **DEFER Phase 3 (high-priority)**                                                                                                                                                                                                                                        | Bundle with `XXXX_REFACTOR_format-truncate-unification` (now combined with F-U-3, F-J-16, F-Y-2, F-AD-1, F-AB-5 per Wave 5 consolidation plan). Wave 6 will document Figma divergences if any; cross-cite the batch task. Single fix, doesn't multiply Wave 6 work. |
| F-L-2 🟢                           | Search no-results layout                                                                                                | 2.0 will note                                                                                                                                                                                                                                                                                                    | **DEFER Phase 3**                                                                                                                                                                                                                                                        | LOW; bundle with general visual polish batch.                                                                                                                                                                                                                       |
| F-U-1 🟠                           | SectionCard wrong home (in `web/src/pages/detail/` instead of `libs/ui/src/`)                                           | Hoist refactor changes rendered DOM minimally (same component, different import path) — Wave 6 Figma fidelity unaffected                                                                                                                                                                                         | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle with `XXXX_REFACTOR_folder-structure-rationalization` (F-U-1 + F-AH-3 + F-AH-2 + F-X-1 + others).                                                                                                                                                            |
| F-U-2 🟠                           | EmptyState reimplemented locally per page                                                                               | Similar — hoist doesn't change visuals                                                                                                                                                                                                                                                                           | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle with folder refactor + component-reuse batch.                                                                                                                                                                                                                |
| F-U-3 🟠 (re-confirmed)            | 6 truncation re-impls (paired with J-7)                                                                                 | Same as J-7                                                                                                                                                                                                                                                                                                      | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Consolidates with `XXXX_REFACTOR_format-truncate-unification`.                                                                                                                                                                                                      |
| F-U-4 🟠                           | STROOPS_PER_XLM constant duplicated (number + bigint variants)                                                          | None — both produce same display                                                                                                                                                                                                                                                                                 | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle with `XXXX_REFACTOR_stroop-conversion-canonical-util` (paired with F-AN-7).                                                                                                                                                                                  |
| F-U-5 🟡                           | Minor component-reuse violation                                                                                         | None                                                                                                                                                                                                                                                                                                             | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle.                                                                                                                                                                                                                                                             |
| F-X-1 🟠                           | `assetLegLabel` cross-folder reach `liquidity-pools/` → `pool-detail/`                                                  | None — same rendered output                                                                                                                                                                                                                                                                                      | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle with `XXXX_REFACTOR_folder-structure-rationalization`.                                                                                                                                                                                                       |
| F-X-2 + F-X-3 + F-X-5 🟡           | Various coupling smells                                                                                                 | None                                                                                                                                                                                                                                                                                                             | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle with folder refactor.                                                                                                                                                                                                                                        |
| F-J-16 🟠                          | Duplicate `formatFee` BigInt vs Number, 2 implementations                                                               | None (both produce same output)                                                                                                                                                                                                                                                                                  | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle with `XXXX_REFACTOR_format-truncate-unification`.                                                                                                                                                                                                            |
| F-Y-2 🟠 NEW (Wave 5)              | Debounce pattern duplicated 4× across filter components                                                                 | None                                                                                                                                                                                                                                                                                                             | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle with `XXXX_REFACTOR_format-truncate-unification` OR spawn own micro-task `XXXX_REFACTOR_debounce-hook-extract`.                                                                                                                                              |
| F-AH-3 🟡 NEW (Wave 5)             | SectionCard wrong home (same as F-U-1)                                                                                  | None                                                                                                                                                                                                                                                                                                             | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Consolidates F-U-1; one task.                                                                                                                                                                                                                                       |
| F-AH-2 🟡 NEW (Wave 5)             | Folder asymmetry — some features subfolders, others flat                                                                | None                                                                                                                                                                                                                                                                                                             | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle with `XXXX_REFACTOR_folder-structure-rationalization`.                                                                                                                                                                                                       |
| F-AD-1 🟠 NEW (Wave 5)             | Leaked-concern (6-file truncation, 4-file debounce changes)                                                             | None — symptom of underlying dups already counted                                                                                                                                                                                                                                                                | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Resolved by bundling formatter + truncation + debounce extractions.                                                                                                                                                                                                 |
| F-AB-5 🟠 NEW (Wave 5)             | 6 cross-task formatter/truncation duplications                                                                          | None — symptom of underlying dups                                                                                                                                                                                                                                                                                | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Same as F-AD-1.                                                                                                                                                                                                                                                     |
| Wave 5 Class C remaining (~5 misc) | Mixed                                                                                                                   | None                                                                                                                                                                                                                                                                                                             | **DEFER Phase 3**                                                                                                                                                                                                                                                        | Bundle into appropriate Phase 3 batch task.                                                                                                                                                                                                                         |

### Class D catalog-only (~85 findings)

**All DEFER Phase 3 bulk-spawn (3.2).** No Gate B action. Includes:

- All archaeology Future Work cross-refs (A2/A3/A4/A5/Q-4 + 25 unspawned)
- All lore process / commit conventions findings (Q + AR series + Q-7 forward-link drift)
- All dependency hygiene non-CVE (F-CO-6 mui triple-version etc.)
- Build/deploy hygiene sub-Medium + Low
- AR-7 branch protection (needs human GitHub check)
- DN-1 build version SHA display
- F-AE-1/2/3/4/7 console hygiene sub-items
- All Wave 5 Class D findings (21 items)
- All H-security sub-LOW (1 item)
- All I-polling sub-Yellow + LOW (4)
- All K/E/L sub-Yellow + LOW (~6)
- All AO build hygiene sub-Yellow + LOW (2)

Bulk-spawn pattern: cluster small fixes into 1-2 batch tasks per area; spawn dedicated task only for items that don't naturally cluster.

### Class E off-band (2 — both still pending)

| ID        | Finding                                                                                                               | Action                                                                                                                                                                                                                                                                            |
| --------- | --------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| F-CO-1 🟠 | Vite 7.3.1 has 3 high-severity dev-server CVEs; fix at 7.3.3                                                          | **Still pending.** Spawn dedicated `XXXX_CHORE_vite-7.3.3-cve-bump.md` backlog task → promote → single-line bump → merge. **Does not block Gate B.** Recommended: do this in parallel with Gate B fix-first batch.                                                                |
| C-17 🟠   | No `tower_http::cors::CorsLayer` in `crates/api/src/` — prod browser CORS depends on infra (API GW / ALB) terminating | **Still pending.** Comms task: ping backend/infra owner (Filip Mazur or Staś per recent work history). Question: "API GW / ALB terminuje CORS w prod, czy potrzebny `tower_http::cors::CorsLayer` w `crates/api/`?" Audit cannot resolve unilaterally. **Does not block Gate B.** |

---

## Summary table

| Class                          | Total at gate |                                          Fix-first @ B | Defer Phase 3 | Off-band |
| ------------------------------ | ------------: | -----------------------------------------------------: | ------------: | -------: |
| A (baseline-breaker remaining) |            11 |                                                      0 |            11 |        0 |
| B (routing/contract remaining) |            10 | **6** (F-D-2 + F-AE-5 + F-K-2 + F-K-3 + F-L-1 + F-K-4) |             4 |        0 |
| C (visual/layout remaining)    |           ~20 |                                    **2** (CA-1 + CA-2) |           ~18 |        0 |
| D (catalog-only)               |           ~85 |                                                      0 |           ~85 |        0 |
| E (off-band)                   |             2 |                                                      0 |             0 |        2 |
| **Total**                      |      **~128** |                                                  **8** |      **~118** |    **2** |

(Total ≠ 156 cumulative because resolved/dropped items excluded: F-E-1, F-E-2, F-E-8, F-D-1, F-K-1, A1.)

## Fix-first action list (Gate B → Wave 6)

**Three `audit-blocker`-tagged backlog tasks to spawn + land before Wave 6 starts. Plus 2 pre-launch must-fix Class C bundled into one.**

### 1. `XXXX_BUG_composite-notfound-sub-section-queries.md`

- **Class:** B
- **Severity:** 🟠 HIGH
- **Findings closed:** F-D-2, F-AE-5
- **Scope:** Account / asset / contract detail pages fire sub-section queries (transactions/balances/holders/events/invocations tabs) in parallel with the parent entity query. When parent returns 404, sub-section queries also error, producing 2-4 stacked error blocks (worst: E9 contract = 4 blocks).
- **Approach:** Gate sub-section queries on parent query status. Either:
  - (a) `useQuery({..., enabled: !!parentData})` on every sub-section hook, or
  - (b) Render NotFound at parent level + skip rendering tab content entirely
- **AC:** `/contracts/<garbage>`, `/accounts/<garbage>`, `/assets/<garbage>` render single NotFound block (not 2-4). Loading state on parent doesn't trigger sub-section error.
- **Effort:** ~1-2h.
- **Related:** 0257 (this audit), 0073, 0074, 0075.

### 2. `XXXX_BUG_pool-detail-missing-asset-and-ledger-links.md`

- **Class:** B
- **Severity:** 🟠 HIGH
- **Findings closed:** F-K-2, F-K-3
- **Scope:** Pool detail page renders reserve asset labels (e.g. "USDCOIN", "EUR") as plain text instead of `<Link to="/assets/:id">`. Participants table "Since ledger" column renders ledger sequence as plain number instead of `<Link to="/ledgers/:seq">`. Breaks cross-entity navigation invariant.
- **Approach:** Wrap labels in `<Link>` components. Use existing `assetUrl` / `ledgerUrl` helpers from `web/src/router/routes.ts`.
- **AC:** Hover on reserve label shows pointer cursor + asset URL. Click navigates to asset detail. Same for "Since ledger" → ledger detail.
- **Effort:** ~30 min.
- **Related:** 0257, 0077.

### 3. `XXXX_BUG_search-strkey-canonical.md`

- **Class:** B
- **Severity:** 🟠 HIGH
- **Findings closed:** F-L-1, F-K-4
- **Scope:** Search input accepts strkey for pools (`L...`) but returns 0 results across all 6 search tabs. Backend supports lookup by strkey but search route normalizer skips conversion. Empty-state hint also omits `L...` from supported formats list.
- **Approach:** Single shared "strkey canonicalisation" helper applied at search-input parse boundary. Update empty-state hint to include `L...`.
- **AC:** Paste pool strkey `L...` into search → finds pool. Empty-state hint lists all supported prefixes (`G..` accounts, `C..` contracts, `L..` pools, `M..` muxed if applicable, hash prefixes for transactions/ledgers).
- **Effort:** ~1h.
- **Related:** 0257, 0060.

### 4. `XXXX_PRELAUNCH_footer-legal-and-external-links.md`

- **Class:** C (pre-launch must-fix)
- **Severity:** 🟠 HIGH
- **Findings closed:** CA-1, CA-2
- **Scope:** Footer renders Terms of Service / Privacy Policy / Cookies + Resources (GitHub / Stellar docs / Soroban docs / Stellar dashboard) as dead `<span>` (no `href`). Pre-launch legal/compliance liability for legal links; discoverability gap for Resources.
- **Approach:** Either:
  - (a) Fill in real hrefs (Terms/Privacy/Cookies content URLs from legal team; Resources external URLs)
  - (b) Hide dead links entirely until content ready
- **AC for path (a):** All footer items render as `<a href=...>` with real URLs. External links have `target="_blank" rel="noopener noreferrer"`. Internal Terms/Privacy/Cookies pages exist (or external URLs configured).
- **AC for path (b):** Dead `<span>` removed; footer renders only working items.
- **Effort:** depends on path. Path (a) = needs legal team content (blocked external). Path (b) = ~30 min.
- **Decision required from user:** which path, and if path (a), is content ready?
- **Related:** 0257.

### Off-band (separate immediate, non-blocking)

### 5. `XXXX_CHORE_vite-7.3.3-cve-bump.md` (Vite CVE)

- **Class:** E
- **Action:** `npm i vite@7.3.3 --save-dev` + verify build green + commit.
- **Does not block Gate B.** Single-line bump.

### 6. CORS infra question (no task spawn, just comms)

- **Action:** Ping backend/infra owner (Filip Mazur or stkrolikiewicz) with C-17 question: "Production CORS — does API GW / ALB terminate, or do we need `tower_http::cors::CorsLayer` added to `crates/api/`?"
- **Does not block Gate B.**

## After fix-first lands

1. Each `audit-blocker` task merges to develop via standard PR flow.
2. Rebase audit branch onto develop: `git rebase origin/develop` from `research/0257_frontend-comprehensive-audit`.
3. Update this triage doc with "Resolved in <SHA>" rows.
4. Append worklog: "Gate B fix-first batch landed, audit branch rebased on develop tip <SHA>, baseline reset for Wave 6."
5. **Then** start Wave 6 (Track 2 visual + UX — Playwright full re-pass, Figma fidelity, perf, responsive, a11y, CSS theme).

## Cascade compression — what these fix-firsts reduce in Wave 6

| Fix-first                           | Reduces findings in                                                                                                              |
| ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| F-D-2 + F-AE-5 (composite NotFound) | 2.0 (3 routes × NotFound scenarios = 9 cells), 2.1 (3 Figma fidelity findings), 2.5 (3 a11y findings on stacked error landmarks) |
| F-K-2 + F-K-3 (pool link gaps)      | 2.0 (single route, multiple sub-renders), 2.5 (non-link text styled as link)                                                     |
| F-L-1 + F-K-4 (strkey search)       | 2.0 (single search route, but pre-launch UX priority)                                                                            |
| CA-1 + CA-2 (Footer dead links)     | 2.0 (14 routes × footer = 14 findings), 2.1 (Figma footer divergence finding)                                                    |

**Estimated Wave 6 finding reduction: ~30 duplicate findings avoided.**

## Anti-pattern reminder (per task README)

Do NOT fix-first at Gate B:

- Class A type-safety flag toggles (regenerates Wave 1-4 baseline)
- Class A bundle size optimizations (Wave 6 2.2 measures current)
- Component reimplementation hoists where DOM unchanged (Wave 6 wouldn't see difference)
- Formatter unification batches (single fix, no audit invalidation)
- Lore drift / process compliance (no code change)
- Future Work backlog spawning (Phase 3 sub-phase 3.2 owns)

## Gate B status

**Cleared once 4 fix-first tasks land + audit branch rebased on develop.**

If user picks to defer ALL fix-first to Phase 3 instead (Path Y from prior discussion), Wave 6 starts immediately on current baseline. Trade-off: ~30 duplicate findings in Track 2 output to consolidate during Phase 3 (manageable, just noisier audit-summary.md).

If user picks to fix-first only the pre-launch must-fix (#4 footer legal), Wave 6 starts after that single task lands. Compromise: most Wave 6 cascade compression deferred, but pre-launch liability cleared.

**Recommended:** spawn all 4 (#1-#4) audit-blocker tasks + off-band (#5-#6). Total Gate B effort ~3-4h spread across Filip/Karol/Staś. Wave 6 starts post-merge of all 4 + rebase.
