---
id: '0280'
title: 'Audit 0257 closing round 3 — remaining pre-launch batch (incl. surrogate-id anti-pattern)'
type: FEATURE
status: done
related_adr: ['0024', '0030', '0032']
related_tasks: ['0257', '0272', '0274', '0275']
tags:
  [
    'frontend',
    'backend',
    'audit-closing',
    'elastic',
    'priority-high',
    'effort-medium',
    'pre-launch',
    'phase-implementation',
  ]
links:
  - 'Parent audit: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/README.md'
  - 'Master action queue: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/audit-action-queue.md'
  - 'Predecessors: archive/0272 (round 2), 0274 (accounts API), 0275 (contracts list)'
history:
  - date: '2026-06-06'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0257 after round-2 (0276) closed 13/14 SHOULD + the MUST.
      Round 3 = remaining pre-launch batch: the surrogate-id anti-pattern
      cluster (F-RR-36..40, found via live data-walk + endpoint audit) plus
      the original audit cards still open at MUST/SHOULD (build-SHA,
      touch-targets, bundle, error-envelope, type-safety, OperationFlowTree).
      Same elastic card-by-card model as 0272/0276; master action queue is
      source of truth; commits cite F-IDs. NOTE: per explicit user decision
      2026-06-06, this task file is spawned on the audit branch
      `research/0257` (not develop) as a one-off exception — it lives with
      the audit work that produced it.
  - date: '2026-06-06'
    status: backlog
    who: karolkow
    note: >
      Scope A (surrogate-id anti-pattern, F-RR-36..40) RESOLVED out-of-band by
      task 0243 (`feat/0243-accounts-contracts-assets-ch-read-path`) — the CH
      read-path branch hit the same asset-routing problem and fixed it with
      Design A (`route_token` payload), which SUPERSEDES the F-RR-37/38
      "uniform identifier" prescription. A fresh-eyes + adversarial review on
      0243 rejected "uniform identifier" (would destroy the asset/nft display
      headline; cannot remove the 2-component nft special-case). Rewrote
      Scope A + the AC to mark it DONE-via-0243 and flagged F-RR-38 as
      superseded so round-3 does not re-implement the rejected design.
  - date: '2026-06-08'
    status: active
    who: karolkow
    note: >
      Promoted to active. Starting implementation of Scope B cards on
      feat/0280_audit-0257-closing-round-3 branched from research/0257.
  - date: '2026-06-08'
    status: active
    who: karolkow
    note: >
      Round-3 session #1 (9 commits on feat/0280). DONE: 0066 complete+archive
      (drift card 6.1); 0063 PollingIndicator wired into home Latest
      transactions header (Figma tx-only) + complete+archive (spin/onRefresh
      enhancements dropped — spin invisible at fast fetch + smart-polling makes
      onRefresh redundant); card 2.2 CLOSED (PageStub dead-code delete +
      F-X-1 pool-coupling → web/src/pages/pool-shared/, tangle 0<->0); queue
      cards 8.4/2.2/6.3/3.1 refreshed. SKIP (user, total): 1.2 build-SHA, 3.1
      noUncheckedIndexedAccess (soft-fallback != hard-fail + repo-wide flag
      needs buy-in). DEFER post-launch (user): 11.4. STILL OPEN: 8.4 residual
      (~1h), 11.6, 4.1, 3.2, Scope-C process cards. Infra fix: worktree was
      missing @testing-library/* (dual-React, all render tests red) — npm
      install in worktree, not a code change; memory updated. Pending queue
      flips: 1.2 -> SKIP, 11.4 -> DEFER.
  - date: '2026-06-09'
    status: active
    who: karolkow
    note: >
      Round-3 session #2. Card 3.2 worked: branded ID types prototyped then
      DROPPED (no teeth without cascade-threading; F-AQ-4 SKIP); kept + added
      `isAssetId` (C-5). To unblock wiring it (asset id was non-canonical /
      surrogate-routed), MERGED task 0243's branch
      `feat/0243-accounts-contracts-assets-ch-read-path` (ClickHouse read-path
      + `route_token` asset addressing) into feat/0280 per user — so 0280's
      eventual PR sits on top of 0243 (reviewed/merged first), keeping 0280's
      diff to the audit work. Merge done with STRICT separation per user:
      merge commit `311c6929` = routes.ts conflict resolution only (route_token
      supersedes surrogate_id); the semantic `routeForHit.test.ts` rewrite is a
      separate follow-up commit `70eb1644` (not mixed into the merge). Then
      wired `isAssetId` into AssetDetailPage param-guard (`503a9485`, canonical
      -only, mirrors the 5 sibling detail pages). typecheck 4/4 + web 84 +
      ui 57 green. Scope A (F-RR-36..40 / route_token) is now physically on
      this branch via the merge, not just "done elsewhere".
  - date: '2026-06-09'
    status: done
    who: karolkow
    note: >
      Round-3 session #3 + CLOSE. Shipped: card 6.3 item-4 (op-type OpenAPI
      enum / F-Z-2) — `domain::OperationType` registered as a standalone
      OpenAPI component → codegen emits a named `OperationType` union; FE
      `operationTypes.ts` now keys `DISPLAY_LABELS: Record<OperationType,
      string>` exhaustively (drift = compile error). api-types regenerated.
      Live-data polling: home latest-ledgers/transactions feeds moved off the
      12s homePolicy to an adaptive `midpointPollDelay` (target = lastClose +
      1.5×cadence, clamped) tuned to observed ~5.7s ledger cadence and biased
      early (5.5s) to avoid double-catch at the cost of a cheap retry; new
      `polling.test.ts` (+5). Triage: 7.1 F-W6-E2-2 filter-label typo FIXED;
      5.2 / 3.3 / 7.1 → SKIP (5.2 contract-tabs already URL-state + LP-chart
      0199-blocked; 3.3 type-nicety no bug; 7.1 cosmetic bag); 7.5 NFT heading
      → DONE-stale (h1/h2 already correct via TableSectionHeader); F-AQ-8
      results_meta_xdr prototyped then reverted (dead-code, field intentionally
      not served per spec 0046 — marginal). Commits this session: op-type
      (2: feat + docs), polling (4: midpoint + 2 retunes + bias-early), typo
      (1) + queue triage (1 docs) + 7.5 (1 docs). All pre-commit gates green
      (web 89 incl +5 polling, ui 57, cargo check). RESIDUALS (remain tracked
      as cards under the still-active parent 0257 queue, NOT dropped): 6.3
      items 1-3 (CORS C-17, wasm_interface_metadata schema, results-meta —
      backend-blocked), 8.1 Playwright smoke + CI gate, 6.2 future-work
      checklist, 11.6 touch-targets, 4.1 bundle, 10.1 LP-oracle ADR — all
      DEFER post-launch or backend-owned. FE-solo pre-launch work is complete.
---

# Audit 0257 closing round 3 — remaining pre-launch batch

## Summary

Third (and intended final) elastic closure container for audit 0257. Scope =
everything still open at **pre-launch MUST/SHOULD** after round-2 (0276),
headlined by the **surrogate-id anti-pattern** cluster surfaced by the
2026-06-06 live data-walk + API endpoint audit. Master action queue tracks
per-finding status; this task implements card-by-card.

## Status: Done (2026-06-09)

FE-solo pre-launch round-3 work complete. Residual cards (6.3 backend coord,
8.1 Playwright/CI, 11.6/4.1/10.1/6.2) remain DEFER-post-launch or backend-owned
and stay tracked under the still-active parent audit 0257 queue.

## Context

Round-2 (0276) closed F-RR-2/3/6/7/14/17/18/25/33 + F-0272S-2/3/4 +
LOADSKEL-1/2/3; 0274 closed the MUST F-0272S-1 (accounts endpoint); 0275
shipped the contracts list (card 1.3 DONE). F-RR-34/35 fixed on the audit
branch. F-RR-21 (search a11y) is an explicit SKIP. This round picks up the
rest. Full detail + file:line + repro in the master action queue.

## Scope

### A. Surrogate-id anti-pattern (F-RR-36..40) — ✅ RESOLVED by task 0243 (Design A: `route_token`)

DB autoincrement surrogate must never appear in a user-facing URL. Endpoint
audit (queue, 2026-06-06) confirmed the blast radius is **assets only**.

**Shipped on `feat/0243-accounts-contracts-assets-ch-read-path` (karolkow,
2026-06-06) — with a design that SUPERSEDES the original F-RR-37/38
prescription below.** The audit proposed "uniform `identifier` = canonical,
delete the special-case". A fresh-eyes + adversarial review on the 0243 branch
**rejected** that: assets AND nfts both carry a recognizable headline
(`asset_code` / NFT `name`) that is NOT routable, and `label` is already
occupied (asset_type / collection / home_domain), so a uniform-canonical
`identifier` would destroy the headline for two types **and still** could not
remove the irreducibly 2-component nft special-case (`/nfts/:c/:t`). Shipped
design instead: `identifier` stays the display headline; a separate
`route_token: Option<String>` carries the canonical `/assets/:id` token
(contract StrKey | `CODE-ISSUER` | `native`), present only for asset
(`skip_serializing_if`); `routeForHit` routes `route_token ?? identifier`.

Per-finding status (all on 0243 unless noted):

1. **F-RR-37** (backend) — ✅ DONE differently. `asset_hits` CTE projects
   `route_token` (canonical) _alongside_ the display `identifier` (asset
   code), not _instead_ of it. api-types regenerated.
2. **F-RR-36** (FE) — ✅ DONE. `AssetItem.id` is now the canonical String
   token (`i32 → String`), so `AssetsTable` `routes.asset(row.id)` is already
   canonical — no `row.id` surrogate leak.
3. **F-RR-38** (FE) — ⚠️ SUPERSEDED. Original "delete special-case → uniform
   `identifier`" is WRONG (rationale above). Shipped `routeForHit` uses
   `route_token ?? identifier` — the explicit `route_token` payload IS the
   honest minimal encoding. (An "always-populate route_token" variant was
   reviewed + rejected: wire-bloat + a dishonest never-read nft value.) No
   further FE change needed.
4. **F-RR-39** (backend) — ✅ DONE. `parse_asset_id` has a `native` branch;
   `/assets/native` routable. (`XLM` keyword not added — `native` is the
   canonical reserved token.)
5. **F-RR-40** (backend) — ✅ DONE. `surrogate_id` removed from ALL search
   buckets; `route_token` is asset-only (`None` for account/contract/pool).

### B. Original audit cards still open at pre-launch tier

Per-card status after the 2026-06-08 round-3 session:

- **1.2** (MUST) — build SHA / version stamp — 🚫 **SKIP** (user decision
  2026-06-08; queue card flipped to SKIP).
- **11.6** (SHOULD) — touch targets ≥44px @375 — 🟡 **OPEN** (not started).
- **11.4** (SHOULD) — OperationFlowTree vs Figma — ⏸️ **DEFER post-launch**
  (user decision 2026-06-08; still flat, verify blocked on soroban data +
  designer sign-off).
- **4.1** (SHOULD) — bundle + LP chart lazy + vendor split — 🟡 **OPEN**
  (recommendation: defer post-launch; perf-only, woff2 already cut ~1MB).
- **8.4** (SHOULD) — error envelope + reporter + boundary — ✅ **DONE**
  (2026-06-09): boundary 7/7 + `client.ts` `.body` envelope + F-AF-4
  `[object Object]` guard. `extractErrorCode` + console-reporter prototyped
  then dropped (no consumer / console-only = dev-redundant + prod-useless);
  global reporter (F-AE-7) **deferred** to a Sentry/DataDog provider decision.
- **3.1** (SHOULD) — `noUncheckedIndexedAccess` — 🚫 **SKIP** (user decision
  2026-06-08, total skip). Prototyped (flag + 6 fixes, green) then reverted:
  `?? ` fallbacks were soft-failing not hard; repo-wide flag needs team
  buy-in. Queue card flipped to SKIP.
- **3.2** (SHOULD) — branded ID types — 🟢 **PARTIAL/resolved**: branded
  types DROPPED (no teeth without cascade-threading); `isAssetId` validator
  kept + wired into AssetDetailPage. Card is done-as-it-will-be.

### B′. Session-3 additions (live-data polling + cosmetic triage)

- **6.3 item-4** op-type OpenAPI enum (F-Z-2) — ✅ **DONE** (see Scope C 6.3).
- **Live polling cadence** (new, not an original card) — ✅ **DONE**: home
  latest-ledgers/transactions feeds now poll adaptively at the ledger
  midpoint (`midpointPollDelay`, ~5.5s biased-early), replacing the flat 12s
  homePolicy. Global `useNetworkStats` (LIVE badge) stays 12s. +5 unit tests.
- **7.1** visual polish — 🚫 **SKIP** (cosmetic bag); F-W6-E2-2 filter-label
  typo FIXED en route; regressed AK-1/AK-2 owned by DONE cards 11.2/11.3.
- **7.5** NFT heading hierarchy — ✅ **DONE-stale** (h1/h2 already correct via
  `TableSectionHeader`).
- **5.2** URL state for tabs — 🚫 **SKIP** (contract tabs already URL-state;
  LP-chart half is a 0199-blocked placeholder).
- **3.3** switch exhaustiveness / assertNever — 🚫 **SKIP** (type-nicety, no
  bug; `noImplicitReturns` already guards the return-typed switches).

### C. Process / coordination SHOULDs (can split out)

- **6.1** lore drift — 🟢 **PARTIAL**: 0066 triple-drift fixed +
  completed/archived; 0063 same-class drift fixed + closed. Residual: the
  Phase-3 walker script + sweep of any other `status: active` FE tasks.
- **6.2** spawn 23 Future Work — 🟡 OPEN
- **6.3** backend coord (CORS / op-type enum / results_meta_xdr /
  wasm_interface_metadata) — 🟢 **PARTIAL (1 of 4 DONE)**: item-4 **op-type
  OpenAPI enum (F-Z-2) DONE 2026-06-09** — `domain::OperationType` registered
  in OpenAPI components → codegen owns it, FE hand-typed mirror killed
  (`Record<OperationType,string>` exhaustive guard). Items 1-3 (CORS C-17,
  wasm_interface_metadata schema, results_meta_xdr — the last prototyped then
  reverted as dead-code, field intentionally not served per spec 0046) remain
  OPEN + backend-blocked.
- **8.1** test coverage — 🟢 PARTIAL (0226 shipped 132 tests; residual =
  Playwright smoke 11 pages + CI gate). **NOTE:** session also fixed a
  worktree-node_modules gap (missing `@testing-library/*` → dual-React) that
  was reding all render-based tests in this worktree — `npm install` fix, not
  a code change.
- **10.1** LP oracle ADR — 🟡 OPEN (needs team decision).
- **2.2** folder rationalization — ✅ **CLOSED**: the two genuine pieces done
  — PageStub deleted (dead code) + F-X-1 pool coupling extracted to
  `web/src/pages/pool-shared/` (bidirectional tangle 0↔0); detail/-hoist +
  search/ + utils/ + page-root-helpers all SKIP (cosmetic, fresh-eyes
  verified single-consumer lib / mixed cross-cutting / non-issues).

## Out of scope

- **F-RR-21** search a11y — explicit SKIP (post-launch).
- NICE (~20 F-RR/appendix cosmetics) + POST (F-RR-1 order-param OpenAPI,
  F-RR-26 tree-shake) — stay in queue, cherry-pick post-launch.

## Acceptance Criteria

- [x] Scope A (F-RR-36..40) DONE **via task 0243** — zero DB surrogate ids in
      any user-facing URL; `routeForHit` routes `route_token ?? identifier`
      (Design A — NOT the originally-prescribed uniform `identifier`, which was
      rejected on review); native asset routable. F-RR-38 superseded.
- [x] Scope B MUST/SHOULD cards DONE or SKIP-with-rationale — **resolved at
      close**: 1.2 SKIP, 3.1 SKIP, 3.3 SKIP, 5.2 SKIP, 7.1 SKIP, 11.4 DEFER, 8.4
      DONE, 3.2 done-as-it-will-be (branded dropped + isAssetId), 7.5 DONE, op-type
      enum DONE. Remaining 11.6 / 4.1 = DEFER post-launch (perf/a11y, not blockers).
- [x] Each commit cites the F-ID(s) / card closed; queue STATUS flipped —
      done for this session's commits (2.2/F-X-1/PageStub/3.1/0063/0066) and
      queue cards 1.2 → SKIP + 11.4 → DEFER now flipped.
- [x] **API types regenerated** — DONE for the op-type enum change
      (`crates/api/src/openapi/mod.rs` registered `OperationType` →
      `npx nx run @rumblefish/api-types:generate`; openapi.json + generated/
      committed in the same commit, freshness gate green). Scope A's F-RR-37
      regen shipped earlier on 0243.
- [x] **Docs updated** per ADR 0032 — N/A for the shipped changes: the op-type
      enum is an additive OpenAPI **component** with zero wire-shape change
      (response op-type fields stay `string`); polling is pure FE behaviour;
      typo/triage are cosmetic. None alter schema / endpoints / ingestion /
      infra shape.
- [x] Queue reflects final round-3 state — flipped this session: op-type /
      F-Z-2 / 0069-FW DONE, 6.3 → PARTIAL (1/4), 5.2 / 3.3 / 7.1 → SKIP, 7.5 →
      DONE-stale, 10.3 → SKIP (invalid self-link finding). Open residuals remain
      TODO/PARTIAL under the still-active 0257 parent queue.

## Design Decisions

### Emerged (session #3)

1. **op-type enum as a standalone OpenAPI component, not a typed DTO field.**
   The instinct was to type the response op-type fields (`operation_types`,
   `OperationItem.type_name`) to `OperationType`. Rejected: `db_operations`
   and the list mapper are infallible and on the polled hot path; converting
   `String → OperationType` there adds a fallible parse for a cosmetic gain.
   Instead registered `OperationType` in `components(schemas(...))` directly —
   codegen emits the named union with zero wire change, and the FE drift
   protection comes entirely from keying its label map `Record<OperationType,
string>`. Honest minimal change.

2. **Adaptive polling = stateless midpoint, not EMA.** Considered an EMA over
   close-to-close deltas (adapts to cadence drift) but it needs per-fetch
   state and a divide-by-sequence guard against 0-/2-block poisoning. Chose a
   stateless `1.5×cadence − elapsed` rule (re-anchors on each response's
   `closed_at`; no delta computed, so 0/2 blocks cannot poison it). Cadence is
   a hardcoded knob (5.5s) — observed mainnet is ~5.7s with tight jitter, so
   adaptivity guards against a regime change that the data shows does not
   happen.

3. **Bias polling early (5.5s < observed 5.7s).** The two 1:1 failure modes
   are asymmetric: firing late catches 2 blocks in one fetch (breaks 1:1,
   unrecoverable), firing early yields an empty fetch the cheap floor retry
   recovers. Tuned the knob below the mean to lean into the recoverable
   failure. Invariants `floor < gap` and `MAX < 2×gap` keep both the retry
   phase and the main tick single-block.

4. **`results_meta_xdr` (F-AQ-8) closed by NOT exposing it.** Verification
   flipped the card's premise: the field is intentionally withheld (spec
   0046). The real defect was a dead FE `as`-cast digging for an absent
   (misspelled) field. Implemented the removal, then reverted on user call as
   marginal — left the card as the corrected analysis.

## Issues Encountered

- **Queue markers badly stale.** Five findings flagged TODO were already
  resolved in code (5.4 cross-entity links, parts of 7.1, F-W6-E5-1 ledger-nav
  disabled boundary, 7.5 NFT h2 via `TableSectionHeader`) or invalid (10.3
  `linked={false}` is self-link suppression on the page's own id, not a
  fix-by-hide). Every card was code-verified before action; ROI rankings from
  the marker alone were wrong twice. Lesson: trust the code, not the STATUS
  cell.

- **`LIVE_MIN_MS` user-edit mid-session.** User raised the poll floor 1000 →
  4000 directly in `polling.ts`; respected (not reverted) and the test
  expectations were updated to the new clamp `[4000, MAX]` rather than the
  edit undone.

## Future Work

Residuals are **not** spawned as new task files — they already exist as cards
in the parent **0257** audit-action-queue, which stays `active` and is the
single source of truth for this audit. Spawning duplicates would recreate the
lore drift the audit itself flags. Open at close, all DEFER-post-launch or
backend-owned:

- **6.3 items 1-3** — CORS (`CorsLayer` / API-GW question), `wasm_interface_
metadata` discriminated-union schema, `results_meta_xdr` (closed-by-analysis).
  Backend-blocked.
- **8.1** — Playwright smoke for the 11 paginated pages + CI test gate.
- **11.6** touch-targets ≥44px, **4.1** bundle split, **10.1** LP-oracle ADR
  (owned by task 0199), **6.2** future-work checklist.

## Notes

- Elastic, card-by-card. Scope A (F-RR-36..40) already DONE on task 0243 with
  Design A (`route_token`) — do NOT re-implement the F-RR-37/38 "uniform
  identifier" prescription; it was reviewed and rejected (see Scope A).
- Process SHOULDs (Scope C) may split to their own task if this grows.
- Audit trail via commit F-ID citations.
- Branch exception: spawned on `research/0257` per user 2026-06-06 (normal
  convention = develop).
