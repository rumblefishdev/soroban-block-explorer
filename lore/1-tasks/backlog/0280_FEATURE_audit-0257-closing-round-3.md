---
id: '0280'
title: 'Audit 0257 closing round 3 — remaining pre-launch batch (incl. surrogate-id anti-pattern)'
type: FEATURE
status: backlog
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
---

# Audit 0257 closing round 3 — remaining pre-launch batch

## Summary

Third (and intended final) elastic closure container for audit 0257. Scope =
everything still open at **pre-launch MUST/SHOULD** after round-2 (0276),
headlined by the **surrogate-id anti-pattern** cluster surfaced by the
2026-06-06 live data-walk + API endpoint audit. Master action queue tracks
per-finding status; this task implements card-by-card.

## Status: Backlog

Activate when ready. Mixed FE + backend (search CTE + asset id parsing).

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

- **1.2** (MUST) — build SHA / version stamp in UI (= DN-1/AO-11).
- **11.6** (SHOULD) — touch targets ≥44px @375 (105/106 fail) (F-W6-RESPONSIVE-4).
- **11.4** (SHOULD) — OperationFlowTree collapse/expand vs Figma (F-DP-4) — verify (was data-blocked; backend now serves soroban/multi-op? recheck).
- **4.1** (SHOULD) — bundle >500KB + LP chart lazy + vendor split (F-AI-1/2/8).
- **8.4** (SHOULD) — error envelope + reporter + SectionErrorBoundary coverage.
- **3.1** (SHOULD) — enable `noUncheckedIndexedAccess` + fix hazards.
- **3.2** (SHOULD) — branded ID types via validators.

### C. Process / coordination SHOULDs (can split out)

- **6.1** lore drift 0066 · **6.2** spawn 23 Future Work · **6.3** backend
  coord (CORS/op-type enum/results_meta_xdr) · **8.1** test coverage
  baseline (critical components) · **10.1** LP oracle ADR · **2.2** folder
  rationalization.

## Out of scope

- **F-RR-21** search a11y — explicit SKIP (post-launch).
- NICE (~20 F-RR/appendix cosmetics) + POST (F-RR-1 order-param OpenAPI,
  F-RR-26 tree-shake) — stay in queue, cherry-pick post-launch.

## Acceptance Criteria

- [x] Scope A (F-RR-36..40) DONE **via task 0243** — zero DB surrogate ids in
      any user-facing URL; `routeForHit` routes `route_token ?? identifier`
      (Design A — NOT the originally-prescribed uniform `identifier`, which was
      rejected on review); native asset routable. F-RR-38 superseded.
- [ ] Scope B MUST/SHOULD cards DONE or SKIP-with-rationale.
- [ ] Each commit cites the F-ID(s) / card closed; queue STATUS flipped.
- [ ] **API types regenerated** — REQUIRED for F-RR-37 (search DTO shape change) + any `crates/api/**` change; `N/A` otherwise per card.
- [ ] **Docs updated** per ADR 0032 — search response shape (F-RR-37) +
      asset addressing convention may warrant a `docs/architecture` note.
- [ ] Queue reflects final round-3 state.

## Notes

- Elastic, card-by-card. Scope A (F-RR-36..40) already DONE on task 0243 with
  Design A (`route_token`) — do NOT re-implement the F-RR-37/38 "uniform
  identifier" prescription; it was reviewed and rejected (see Scope A).
- Process SHOULDs (Scope C) may split to their own task if this grows.
- Audit trail via commit F-ID citations.
- Branch exception: spawned on `research/0257` per user 2026-06-06 (normal
  convention = develop).
