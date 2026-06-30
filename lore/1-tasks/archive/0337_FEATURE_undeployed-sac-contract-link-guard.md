---
id: '0337'
title: 'FEATURE: deployed-state-aware contract link for un-deployed SACs (asset detail + list)'
type: FEATURE
status: superseded
related_adr: []
related_tasks: ['0323', '0336', '0339']
tags: [frontend, sac, assets, api, ux, priority-low, effort-small]
links: []
history:
  - date: '2026-06-29'
    status: backlog
    who: claude
    note: >
      Spawned from the SAC/asset modeling analysis session as the frontend
      sibling of 0336. Front half of "show the un-deployed-SAC handle, marked,
      non-linked"; the back half (re-derive the C… strkey) is 0323's deferred
      option-c.
  - date: '2026-06-30'
    status: superseded
    who: claude
    by: ['0339']
    note: >
      Superseded by 0339 (SAC = facet of classic_credit). The deployed-state link guard
      here was a band-aid for the un-deployed-SAC misleading-link symptom; 0339 root-fixes
      it by not modeling un-deployed SACs as a separate contract-bearing entity. Archived
      as superseded before implementation (root-fix is the chosen path).
---

# FEATURE: deployed-state-aware contract link for un-deployed SACs

## Summary

The asset detail page and the assets list render an asset's `contract_id` as a
**link to `/contracts/{id}`**. For an **un-deployed SAC** there is no contract
page, so the link 404s (after 0323 Phase 2 removes the ghost row) / shows a
hollow contract page (today). When the SAC is un-deployed, render the
`contract_id` as copyable text **without** the link, plus an "un-deployed SAC"
marker. Detail is frontend-only; the list also needs a deploy signal added to
its DTO.

## Context

- **Detail link:** `web/src/pages/assets/AssetSummary.tsx:71-93` renders
  `<IdentifierWithCopy value={asset.contract_id} type="contract" />`, guarded
  only by `{asset.contract_id && …}`. `type="contract"` → links to
  `/contracts/{id}` (`libs/ui/src/identifiers/routes.ts:6`).
- **Signal is already on the detail response:**
  `AssetDetailResponse.deployed_at_ledger: Option<i64>`
  (`crates/api/src/assets/dto.rs`), `null` = un-deployed. So the detail guard is
  **frontend-only, no API change**.
- **List link:** `web/src/pages/assets/AssetsTable.tsx:69-72` renders the same
  `IdentifierWithCopy type="contract"`. But `AssetItem` (the list DTO) has **no**
  `deployed_at_ledger` — only `AssetDetailResponse` does. So the list **cannot**
  tell deployed from un-deployed → guarding it requires adding the field to
  `AssetItem` + the list query sourcing it + **api-types regen**.
- **Contract endpoint 404:** `/v1/contracts/{id}` returns 404 on a missing row
  (`crates/api/src/contracts/handlers.rs:261`).
- **Today vs post-0323:** today an un-deployed SAC still has a ghost
  `soroban_contracts` row → link → hollow page (HTTP 200). After 0323 Phase 2
  the row is gone → 404. The guard fixes both.

### Interaction with 0323 (strkey handling) — read before implementing

- **0323 option-a (default):** `contract_id` goes `null` for un-deployed → the
  existing `{asset.contract_id && …}` guard **auto-hides** the row (no link, but
  no handle shown either). This task's guard then has nothing to render — it only
  matters if the handle is kept.
- **0323 option-c (re-derive the `C…` strkey):** `contract_id` is populated → the
  link would 404 → **this guard is exactly what makes option-c correct** (show
  the handle, non-linked, badged). So: option-c WITHOUT this guard = the dead
  link; this guard is the front half of keeping the handle visible.

→ Clarify the 0323 strkey decision first; this task is only meaningful under
option-c (or while the ghost row still exists today).

## Implementation Plan

### Step 1 — detail guard (frontend-only)

In `AssetSummary.tsx`, when `deployed_at_ledger == null` and `contract_id` is
present, render the `contract_id` as copyable text **without** the link, plus a
small "un-deployed SAC" / "no contract instance" chip. Otherwise the existing
`IdentifierWithCopy`. Check whether `IdentifierWithCopy` already has a no-link
variant; if not, reuse the copy primitive without the link wrapper. **Key off
`deployed_at_ledger == null`** (the universal "this link would 404"), not
`asset_type_name` — that uniformly covers native/edge too.

### Step 2 — list parity (API + frontend)

Add `deployed_at_ledger` (or a `deployed: bool`) to `AssetItem`; have the assets
list query source it (join `soroban_contracts`); regen api-types. Apply the same
guard in `AssetsTable.tsx`. Weigh the list-query join cost (a `soroban_contracts`
lookup over the whole list) — if heavy, split Step 2 out or denormalize.

### Step 3 — tests

An un-deployed SAC (`deployed_at_ledger == null`, `contract_id` present) renders
non-linked + badge in both detail and list; a deployed SAC / Soroban token still
links to its contract page.

## Acceptance Criteria

- [ ] Asset detail: an un-deployed SAC's `contract_id` is shown, **non-linked**,
      with an "un-deployed" marker (no dead/hollow link).
- [ ] Deployed SAC / Soroban token: `contract_id` still links to the contract page.
- [ ] List parity: same guard in the assets list — requires `deployed_at_ledger`
      on `AssetItem` (API + list query + api-types regen).
- [ ] Guard keys off `deployed_at_ledger == null`, not `asset_type`.
- [ ] **Docs updated** — `N/A` (presentation only, no architecture-shape change) —
      verify before close.
- [ ] **API types regenerated** — REQUIRED for Step 2 (`AssetItem` gains a field):
      run `npx nx run @rumblefish/api-types:generate` and commit the diff.
      `N/A` only if Step 2 is split into its own task.

## Notes

- **Depends on 0323's strkey decision** (option-a → handle auto-hidden, this task
  moot for un-deployed; option-c → this task makes it correct). Sequence/clarify
  with 0323 before starting.
- **Sibling of 0336** (classic↔SAC duplication) — same SAC-presentation theme,
  different layer. With 0336's read-collapse, the displayed asset row is the
  `contract_id`-bearing one, so this guard lands in one consistent place.
- Today the link is a hollow page (HTTP 200), not a hard 404; the 404 appears
  after 0323 Phase 2. The guard improves both states.
