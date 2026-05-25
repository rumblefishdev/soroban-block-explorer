# Gate A — Triage Decisions

**Date:** 2026-05-25
**Stage:** end of Wave 3 (Tier 1 + Tier 2 sub-phases complete)
**Findings inventoried:** ~89 actionable across 16 files (`00-archaeology` through `quick-wins-DM-DN-CA`)
**Cumulative severity:** 2 🔴 / 22 🟠 / 29 🟡 / 27 🟢

## Scope of this gate

Per task README "Triage gates" section:

- **Class A (baseline-breaker)** — decide per finding: fix-first | document baseline + accept | defer
- **Class B (routing/contract) that affects Wave 4 sub-phases (1.5 / 1.6 / 1.9 / 1.9b / 1.12 / 1.15)** — fix-first or accept measurement against current broken state
- **Class B that only affects Track 2** — defer to Gate B
- **Class C (visual/layout)** — defer to Gate B
- **Class D (catalog-only)** — defer to Phase 3 bulk-spawn
- **Class E (off-band: security / secret / license)** — immediate independent fix; does not block gate

## Wave 4 sub-phases at risk

Wave 4 runs:
1. **1.5 state coverage matrix** 14×9 = 126 cells (Playwright marathon)
2. **1.6 console + error handling** across all routes
3. **1.9 + 1.9b** component reuse + coupling (grep-heavy)
4. **1.12 state separation** + EXTRA `useTableUrlState` justification
5. **1.15 Stellar domain consistency** (strkey, XDR, Soroban-era, network passphrase)

A finding becomes Gate A fix-first only if it changes what these sub-phases measure.

---

## Class A — baseline-breakers (5 findings)

| ID | Finding | Cascade if NOT fixed | Decision | Rationale |
|---|---|---|---|---|
| **F-AQ-1** 🟠 | `noUncheckedIndexedAccess` off in `tsconfig.base.json` | Toggling mid-audit regenerates tsc errors → 1.11/1.11b baseline invalid | **DEFER (Phase 3 refactor task)** | Audit MEASURES current "flag off" baseline. Toggling = refactor scope, not Gate A. Spawn dedicated task post-audit: enable flag, fix new errors, separate PR. |
| **F-AQ-4** 🟠 | Zero branded ID types (`AccountId` / `AssetId` / `PoolId` / etc.) | None for Wave 4 (1.15 inspects display patterns, not type-level) | **DEFER (Gate B refactor task)** | Type-system refactor, not measurement question. Gate B can decide if it bundles with 1.10/10b senior craft findings. |
| **F-AI-1 + F-AI-2** 🟠 | Main bundle 594KB / LP chunk 313KB | None for Wave 4 (perf is 2.2 = Wave 6) | **DEFER (Gate B)** | Wave 4 doesn't measure bundle. Gate B re-snapshots for Wave 6 baseline. |
| **DM-1** 🟠 | `Footer.tsx:78-102` hardcoded "All systems operational" | Confirms 2.3 V live indicator finding (Wave 6) but doesn't break Wave 4 | **DOCUMENT BASELINE + DEFER (Gate B)** | Audit baseline = "footer indicator is hardcoded, no health probe". 2.3 V will cite this. Fix-first = single-PR refactor (introduce `/health` probe + indicator state), but doesn't unlock Wave 4 anything. |
| **F-AF-1** 🟡 | `client.ts:11-29` error interceptor flattens typed `ErrorEnvelope` | 1.6 console error handling sees flattened `Error` not typed `code` discriminant — partial measurement loss | **ACCEPT BASELINE + NOTE IN 1.6** | Document in Wave 4 1.6 output: "error-state taxonomy measured at consumer level, not interceptor level". Fix-first not justified; refactor in Phase 3 with typed `extractErrorCode()` helper. |

## Class B — routing/contract that affects Wave 4 (3 fix-first candidates)

| ID | Finding | Wave 4 impact | Decision |
|---|---|---|---|
| **F-E-1** 🔴 | Pagination cursor never written to URL on `/transactions` + `/ledgers` Next-click | **1.5 cells D2/D9 on every list page** become unreliable: "success state" matrix can't validate URL-restorable pagination; D9 "stale polling indicator" tied to cursor refresh path | **FIX-FIRST** ✅ Single highest-value Gate A item. Spawn `audit-blocker` task. Without fix, 1.5 records "URL state broken across all list pages" 14 times. |
| **F-K-1 + A1** 🟠 | `/transactions/:hash` renders 14-line stub (`PageStub`), no validation | **1.5 entire E3 row (9 cells) becomes N/A**; 1.6 E3 console blank; 1.7 every tx-link landing E3 = stub-by-design; 2.0/2.1/2.4 E3 = N/A on Track 2 | **RESOLVED 2026-05-25:** 0070+0071 in-flight by FilipDz (local-only, unpushed). Path B (document baseline + delta-audit post-merge). Skip E3 row across all matrix sub-phases with `N/A — gated on FilipDz merge` markers. See action item #3 below for full plan. |
| **F-E-2** 🟠 | Lowercase `?op=invoke_host_function` from URL → MUI warning + API 400 + 0 rows; `normalizeOperationType` doesn't normalize URL input | **1.6 console** registers API 400 on every Next-click during 1.5 marathon → 100+ false-positive console errors during Playwright pass | **FIX-FIRST** ✅ Single-file fix (`normalizeOperationType` at URL parse boundary). Without it, 1.6 baseline polluted by reproducible API 400 noise that masks real console findings. |

## Class B — defer to Gate B (Track 2 impact only)

Listed for the record; no Gate A action.

| ID | Finding | Why Gate B |
|---|---|---|
| F-K-2 + F-K-3 🟠 | Pool detail reserve labels + participants "Since ledger" no `<a href>` | 1.7 already done (Wave 3); affects 2.0 Track 2 only |
| F-L-1 + F-K-4 🟠 | Pool strkey (`L...`) search → 0 results across 6 tabs; strkey↔hex unified canonicalisation | 1.14 already done; affects 2.0 search Track 2 |
| F-E-3, F-E-8 🟡 | URL state edge cases | 2.0 Track 2 measures real-world UX impact |
| F-I-3 🟡 | Polling cache logic gap | 2.0 / 2.2 Track 2 |
| C-5 🟠 | Missing `isAssetId` / `isNftId` validator | Routing-level refactor; Gate B with type-safety refactors |

## Class C — defer to Gate B (visual/layout)

All deferred. Track 2 measures visual fidelity vs Figma post-fix. Gate B triages.

- **CA-1 + CA-2** 🟠 Footer Terms/Privacy/Cookies + Resources dead `<span>` (no `href`) — likely pre-launch must-fix at Gate B
- **J-3** Tx pages: `TopNav.formatNumber` duplicated locally
- **J-5** Timestamp depth inconsistency
- **J-7** Address/hash truncation re-implementations across pages
- **F-L-2** Search no-results layout

## Class D — defer to Phase 3 bulk-spawn (~22 findings)

Catalog-only. Phase 3 sub-phase 3.2 spawns each as `lore/1-tasks/backlog/XXXX_*.md` with `related_tasks: ['0257']`. Examples:

- **A2 + Q-4** 0066 task triple-drift (frontmatter / body / history) → lore-hygiene fix
- **A3** 25/28 Future Work items from FE archive without spawned task → bulk spawn
- **A5** 0199 (LP analytics) + 0215 (LP-blocked FE) still blocked, oracle ADR pending
- **DN-1** Build version / SHA not displayed in UI
- **F-CO-6** `@mui/utils` triple-version in lockfile
- **AR-7** Branch protection rules (needs GitHub-side human verify)
- **Q + AR series** Lore process compliance batch
- **All J-data-formatting sub-Yellow** Trailing-zero, em-dash vs hyphen, etc.
- **All H-security LOW** (1 finding only)
- **All I-polling sub-Yellow + LOW** (4)
- **All K/E/L sub-Yellow + LOW** (~6)
- **AO-build hygiene sub-Yellow + LOW** (2)

Exact list = every Wave 1-3 finding tagged `[Class D]`.

## Class E — off-band immediate (2 actions)

| ID | Finding | Action | Blocks Gate A? |
|---|---|---|---|
| **F-CO-1** 🟠 | Vite 7.3.1 has 3 high-severity dev-server CVEs (path traversal, fs-deny bypass, arbitrary file read via WebSocket); fix at 7.3.3 | **Spawn dedicated `XXXX_CHORE_vite-7.3.3-cve-bump.md` backlog task → promote → merge → audit branch rebase**. Single-line dependency bump. | **No.** Off-band. Wave 4 can start without waiting. |
| **C-17** 🟠 | No `tower_http::cors::CorsLayer` in `crates/api/src/` — prod browser CORS depends on infra (API GW / ALB) terminating | **Open ticket with infra owner: confirm CORS termination at infra layer OR add `CorsLayer` to API**. Audit cannot resolve unilaterally. | **No.** Out of FE audit scope. Document, route to backend/infra. |

---

## Summary table

| Class | Total | Fix-first @ A | Defer to B | Defer to Phase 3 | Off-band immediate |
|---|---:|---:|---:|---:|---:|
| A | 5 | 0 | 3 | 2 | 0 |
| B (Wave 4 impact) | 3 | **3** | 0 | 0 | 0 |
| B (Track 2 only) | 6 | 0 | 6 | 0 | 0 |
| C | 5 | 0 | 5 | 0 | 0 |
| D | ~22 | 0 | 0 | ~22 | 0 |
| E | 2 | 0 | 0 | 0 | 2 |
| **Total** | **~43** | **3** | **14** | **~24** | **2** |

(Remaining ~46 of ~89 actionable findings are LOW severity catalog/nit items — all defer to Phase 3 by default.)

## Fix-first action list (Gate A → Wave 4)

Three `audit-blocker`-tagged backlog tasks to spawn + land before Wave 4 starts:

1. **`XXXX_BUG_url-cursor-not-written.md`** — F-E-1
   - **Class:** B
   - **Severity:** 🔴 CRITICAL
   - **Scope:** `web/src/pages/.../useTableUrlState` (and consumers on `/transactions`, `/ledgers`, every list page) — confirm cursor write path on Next-click, fix to persist `?cursor=...` in URL.
   - **AC:** refresh on page N preserves position; deep link `/transactions?cursor=ABC` lands on cursor page; both list pages tested.
   - **Related:** 0257 (this audit)

2. **`XXXX_BUG_url-op-filter-case-normalise.md`** — F-E-2
   - **Class:** B
   - **Severity:** 🟠 HIGH
   - **Scope:** `web/src/pages/transactions/operationTypes.ts` — `normalizeOperationType` to canonicalise URL input before API call.
   - **AC:** `?op=invoke_host_function` (lowercase) round-trips to canonical case; no MUI warning; API returns 200 with rows.
   - **Related:** 0257, 0251 (H2 — same root cause area)

3. **NO audit-blocker spawn for E3** — F-K-1 + A1 — **RESOLVED 2026-05-25:**
   merged via develop (commit `a2c1b205`, PR #215). E3 row now measurable.
   Delta-audit scope queued (see worklog Phase 4 re-audit queue).
   Original context for the record (was Path B before merge):
   - **Context:** 0070 + 0071 (TxDetail normal + advanced) are in-flight by
     **FilipDz** on local branches not yet pushed to origin (verified: zero
     remote refs matching `0070|0071|tx-detail|transaction-detail`).
   - **Decision:** **Path B (document baseline) + delta-audit plan.**
     Spawning fix-first audit-blocker would duplicate Filip's WIP and risk
     conflict on merge. Auditing his WIP wastes effort (he'll rework
     pre-merge).
   - **Wave 4 + Track 2 baseline:** E3 row skipped across all matrix
     sub-phases. Pre-populate output files with `N/A — gated on 0070+0071
     (FilipDz in-flight, local-only)` markers in:
     - 1.5 `findings/D-state-coverage-matrix.csv` — 9 N/A cells for E3 row
     - 1.6 `findings/M-AE-console-error-handling.md` — E3 section "N/A pending merge"
     - 2.0 `findings/playwright-pass/E3-transactions-detail.md` — header note "deferred"
     - 2.1 `findings/B-figma-fidelity.md` — E3 section "N/A pending merge"
     - 2.4 `findings/R-responsive-matrix.csv` — 3 N/A cells for E3 row
   - **Effective matrix:** 1.5 = 13×9 = 117 measurable cells; 2.4 = 13×3 = 39 measurable cells.
   - **Delta-audit post-merge** (separate ~1.5h session, single Playwright run):
     - E3: 9 cells 1.5 + 1.6 console + 1.7 cross-entity links FROM E3 (ops table → assets/contracts/accounts) + 2.0 visual + 2.1 Figma + 2.4 responsive 3 cells.
     - Total delta scope ~1.5h vs ~6h if E3 audited mid-Wave 4 and re-audited post-Filip-rework.
   - **Action item for user:** ask Filip ETA on 0070+0071 push/merge. If <1 week, queue delta-audit. If longer, document permanently in audit-summary.md as "E3 row not covered, gated on Phase Y task completion".
   - **No backlog task spawned** for E3 — Filip already owns the work via existing 0070+0071 in backlog.

Plus 1 off-band:

4. **`XXXX_CHORE_vite-7.3.3-cve-bump.md`** — F-CO-1
   - **Class:** E
   - **Action:** `npm i vite@7.3.3 --save-dev`; verify build still green; commit.
   - **Does not block Wave 4.**

Plus 1 off-band routing (no spawn, just communication):

5. **CORS infra question to backend/infra owner** — C-17
   - Email / Slack: "Production CORS — does API GW / ALB terminate, or do we need `tower_http::cors::CorsLayer` added to API?"

## After fix-first lands

1. Each `audit-blocker` task merges to develop via standard PR flow.
2. Rebase audit branch onto develop: `git rebase origin/develop` from `research/0257_frontend-comprehensive-audit`.
3. Update this triage doc with "Resolved" rows (date + commit SHA).
4. Append worklog: "Gate A fix-first batch landed, audit branch rebased on develop tip <SHA>, baseline reset for Wave 4."
5. **Then** start Wave 4 (Tier 3 sequential — 1.5 state matrix marathon).

## Anti-pattern reminder (per task README)

Do NOT fix-first at Gate A:
- Lore drift (0066) — batch in 1.18 / Phase 3
- Style / comment nits — Phase 3 batch PR
- Doc-only updates — Phase 3
- Renamings / refactors without behavior change — Phase 3
- Future Work backlog spawning — literally Phase 3 sub-phase 3.2
- Type-safety flag toggles — invalidates current baseline, Phase 3 dedicated task
- Bundle size optimization — Wave 6 measures post-fix; defer to Gate B

## Post-merge update 2026-05-25 — develop @ 6b7fb558 (FilipDz tx-detail PR #215)

**Action item #3 (E3 baseline decision):** **RESOLVED 2026-05-25** —
merged via develop (commit `a2c1b205`). E3 row now measurable. Delta-audit
scope queued (see worklog Phase 4 re-audit queue).

**Net Gate A fix-first scope unchanged:** 2 audit-blocker tasks
(F-E-1 cursor URL write + F-E-2 lowercase op normalise) + 1 off-band
(Vite 7.3.3 CVE bump) + 1 communication (CORS infra question).

**Wave 4 plan adjustment:** E3 row now in-scope across 1.5 / 1.6 / 1.7 /
2.0 / 2.1 / 2.4 (was N/A — Path B markers no longer needed). Effective
matrix restored to 1.5 = 14×9 = 126 cells; 2.4 = 14×3 = 42 cells.

**Severity escalations from post-merge findings:**
- J-4 (STROOPS_PER_XLM gap) bumps 🟡 → 🟠 HIGH (duplicate realised in tx-detail)
- J-7 (truncation re-impls) bumps 🟡 → 🟠 HIGH (count went from 2 → 6)
- F-J-16 NEW 🟠 HIGH (duplicate `formatFee` function)
- F-K-7, F-K-8 NEW 🟡 MEDIUM (E3 outbound link verification — defer to delta)
- F-AI-10 NEW 🟡 MEDIUM (E3 chunk composition)

None of the new severity escalations promote to fix-first at Gate A;
all are Class B/C deferrable to Gate B.
