---
id: '0339'
title: 'REFACTOR: SAC is a facet of classic_credit, not a separate `asset_type` — collapse the classic↔SAC entity split'
type: REFACTOR
status: completed
related_adr: ['0051']
related_tasks: ['0336', '0337', '0323', '0154', '0219']
tags:
  [
    clickhouse,
    sac,
    assets,
    api,
    frontend,
    contract-classification,
    layer-data,
    priority-medium,
    effort-large,
  ]
links:
  - 'https://developers.stellar.org/docs/tokens/stellar-asset-contract'
  - 'https://developers.stellar.org/docs/tokens/anatomy-of-an-asset'
history:
  - date: '2026-06-30'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from a SAC/asset modeling analysis session. Root-fix for the
      classic↔SAC duplication: SAC is the same economic asset as its classic
      credit, not a separate entity. Supersedes the band-aids 0336 (read-collapse)
      and 0337 (link guard); absorbs the un-deployed-SAC-as-asset concern.
  - date: '2026-06-30'
    status: active
    who: stkrolikiewicz
    note: >
      Promoted to active to start the design (ADR-first). Taking the SAC-as-facet refactor
      to the workbench: 3 gating decisions (migration strategy; CH keying for
      one-row-per-(code,issuer) with native/soroban carve-outs; canonical-id wire change)
      → audit of the affected surfaces (schema / write-path / API / frontend) → ADR →
      phased implementation on a branch off develop.
  - date: '2026-06-30'
    status: active
    who: stkrolikiewicz
    note: >
      Design resolved → ADR 0051. Decisions: drop `asset_type=sac (2)`; SAC-ness as
      self-contained property columns (`sac_contract_id` / `sac_strkey` / `sac_deployed`)
      on the classic/native row; KEEP the CH ORDER BY (no rebuild — stop using value 2, the
      handle moves to property columns); converge the write path (SAC deploy/override sets
      the SAC columns on the classic/native row, not a separate type=2 row); canonical id
      `C…→CODE-ISSUER` with `fetch_by_contract_id` extended for deep-link back-compat.
      Subsumes 0336/0337 + option-c. Next: phased impl — Phase 1 (code + PR: schema ADD
      COLUMN, enum/write-path/API/frontend, api-types), Phase 2 (prod ADD COLUMN + ~31k
      type=2→type=1/0 data-pass, writer-first).
  - date: '2026-07-01'
    status: active
    who: stkrolikiewicz
    note: >
      Phase 1 landed — PR #298 merged (enum drops Sac=2; write path + API + frontend +
      docs + api-types). Key EMERGED correction (adversarial review): the SAC facet is NOT
      columns on `assets` (a versionless RMT re-written whole every ledger → a mutable
      non-key column is clobbered by the next trustline re-emit, the same trap that moved
      total_supply→asset_aggregates); it moved to a new indexer-owned `asset_sac`
      AggregatingMergeTree(max) side table, joined at read (ADR 0051 amended). A second
      adversarial review caught a critical bug shipped in #298 — `asset_aggregates_mv`
      narrowed to `asset_type = 1`, but that MV reads `account_balances_current` where
      asset_type is the XDR discriminator (2 = credit_alphanum12, a long-code CLASSIC
      asset, NOT sac) → dropped long-code classic supply/holders; reverted in follow-up
      PR #299 (+ the CH-26.3-validated Phase 2 runbook). 0336/0337 archived (superseded).
      Remaining: **Phase 2** prod data-pass (seed `asset_sac` + insert missing type=1/0
      identities + DELETE type=2), writer-first — runbook: `0339_phase2-migration-runbook.md`.
      Status stays `active` until Phase 2 runs; ACs ticked then.
  - date: '2026-07-02'
    status: completed
    who: stkrolikiewicz
    note: >
      Phase 2 prod data-pass EXECUTED 2026-07-01 (asset_sac seeded, 208 identity
      rows inserted, type=2 DELETEd; prod assets now in {0,1,3}; gates + Step 4
      green; 2 runbook bugs found+fixed → commit ab632264). Final cleanup + SAC-UX
      landed via PR #303 (base develop): dropped the transitional `!= 2` carve-out
      in the api transactions handler; aligned the enrichment-runner candidate
      predicate `IN (1,2)` → `= 1` (both drain modes + status count + module doc +
      README + ignored CH smoke test); narrowed `filter[sac]=true` to deployed-only
      (`sac.sac_deployed`) so reserved SACs stop showing under "SAC"; FE re-modelled
      SAC as a property axis orthogonal to type (type badge always + a separate
      "SAC" tag when deployed; "Has SAC" filter toggle); api-types regen (only the
      filter[sac] description text). All 9 ACs met; ADR 0051 Delivery Checklist
      ticked. Backup `assets_pre0339` kept (soak). 0336/0337 already archived.
---

# REFACTOR: SAC is a facet of classic_credit, not a separate asset_type

## Summary

A classic asset and its Stellar Asset Contract (SAC) are the **same economic
asset** — Stellar docs call them "the same asset", with the SAC being "an API for
interacting with the asset". Our `assets` table splits them into two entities
(`asset_type=1 classic_credit` vs `asset_type=2 sac`), which is a conflation of
_identity_ with _addressing/interface_. Re-model: **SAC-ness becomes a property**
(`contract_id` + a deployed flag) of a `classic_credit` asset, not a separate
type/row. Only `soroban` (bespoke, no classic backing) stays a distinct
contract-backed type. This root-fixes the symptoms that 0336 and 0337 band-aid.

## Context

### The two types are one economic asset

- `classic_credit` (type=1): `code:issuer`, no `contract_id`.
- `sac` (type=2): `code:issuer` **+ `contract_id`** (the SAC address).

Same code, issuer, **balances, supply, holders** — `asset_aggregates` already
keys them as ONE (`(asset_code, issuer_id)`, `asset_type IN (1,2)`). The only
difference between the two rows is whether we recorded the `contract_id` handle.
The system already treats them as one asset economically, but stores/lists them
as two.

### Stellar's model (authoritative — see links)

- Classic asset = the asset (`code:issuer`), exists independently on the network.
- SAC = a reserved contract address, deployed-or-not; "an API for interacting
  with the asset". An **un-deployed SAC is "merely a reserved address, neither an
  asset nor an active contract"**.
- Standard explorers (stellar.expert) show the classic asset as the asset and the
  SAC as a contract-with-deployment-status — **not** a separate asset entity.

### Why it's like this (evolutionary artifact, not a deliberate choice)

The system was "Soroban-first" (0154), so contracts/SACs were modeled first-class
and `classic_credit` was **added later** (0219 — "indexer never writes
classic-credit assets rows"). So `sac` predates `classic_credit`; the split is a
build-order artifact.

### Symptoms the split causes (currently band-aided)

- **Duplication** on `/v1/assets` + non-deterministic by-code-issuer resolver → 0336.
- **Misleading `contract_id`** that links to a non-existent contract page for
  un-deployed SACs → 0337.

### Prod scale (2026-06-30)

`asset_type=2 (sac)` ≈ **31,113** rows (~22.7k overlap un-deployed-SAC ghosts);
`classic_credit` ≈ 316,193. The type=2 set is what folds into classic_credit.

## Target model

- `asset_type ∈ {native, classic_credit, soroban}` — **drop `sac` (=2)**.
- A classic asset carries optional `contract_id` (the SAC address, derivable) +
  a deployed flag (`deployed_at_ledger` / `is_sac_deployed`). SAC = a **facet**.
- `soroban` (type=3) unchanged — the only genuinely distinct contract-backed asset
  (no classic backing).
- "SAC" UI filter → a **property filter** (`classic_credit WHERE contract_id IS
NOT NULL` / `sac_deployed`), preserved without a separate type.
- SAC-event activity (under `C…`) resolves to the (single) classic asset — by
  `code:issuer` (the event carries it) and/or `contract_id` on the one row.

## Implementation Plan (high-level — design via ADR first)

1. **ADR** — model-shape change; cross-link 0034 (rename) + 0037 (schema snapshot).
2. **Enum/schema** — drop `Sac` from `TokenAssetType`; update `asset_type_name`;
   re-key CH `assets` so one row per economic asset (carve-outs for native /
   soroban type=3 keyed by `contract_id`); add the deployed flag.
3. **Write path** — a SAC sighting (deploy or event) **updates** the classic
   asset's `contract_id`/deployed flag instead of emitting a separate `sac` row.
   Un-deployed SAC seen via event → `classic_credit` row from the event's
   `code:issuer`, `contract_id` set, deployed=false. (`detect_classic_credit_assets`
   - the SAC/override path.)
4. **Migration** — fold existing ~31k type=2 rows into their `classic_credit`
   counterparts (merge `contract_id` onto the type=1 row / relabel + dedup). No
   asset lost — esp. SAC-event-only assets that have no prior trustline row.
5. **API DTO** — `asset_type`/`asset_type_name` lose `sac`; surface `contract_id`
   - deployed flag on `classic_credit`. **Regen api-types.**
6. **Frontend** — "SAC" filter → property filter (subsumes 0336 read-collapse on
   the list); contract_id rendered deployment-aware, non-linked when un-deployed
   (subsumes 0337). Canonical id: `code-issuer` primary, `C…` secondary handle;
   resolver accepts both (already partly does).

## Acceptance Criteria

- [x] `asset_type` has no distinct `sac` value; a classic asset with a SAC is
      `classic_credit` + `sac_contract_id` (+ `sac_deployed` flag).
- [x] One `assets` row per economic asset (no classic↔SAC duplication) — verified
      on a prod cohort; by-code-issuer resolver deterministic.
- [x] "SAC" UI filter works as a property filter (deployed-only "Has SAC" toggle, #303).
- [x] Deployed SAC still links to its contract page; un-deployed SAC's
      `sac_contract_id` rendered non-linked + marked (subsumes 0337).
- [x] `soroban` (type=3) unaffected.
- [x] Activity under a SAC's `C…` resolves to the single classic asset
      (`fetch_by_contract_id` matches the `asset_sac` facet).
- [x] Migration folds existing type=2 rows into `classic_credit` / native; no asset
      lost (Phase 2, 2026-07-01: 208 identity rows inserted, type=2 DELETEd).
- [x] **Docs updated** — ADR 0051 + database-schema / backend / frontend / xdr-parsing
      overviews. Required (shape change).
- [x] **API types regenerated** — done (#298 DTO change; #303 filter description).

## Supersedes / relations

- **Supersedes 0336** (read-collapse) and **0337** (link guard) — both root-fixed here.
- **Absorbs** the un-deployed-SAC-as-asset concern (they become `classic_credit`,
  not a separate row) — aligns with the standard Stellar model.
- **Independent of 0323 Phase 2** — the `soroban_contracts` ghost deletion stays
  correct regardless (un-deployed SAC is not a deployed contract).
- Rationale anchored in Stellar docs (links) + the 0154/0219 build-order history.

## Notes

- **Big blast radius** (enum / DTO / frontend filter / api-types / migration /
  ADR) → `effort-large`; do the ADR + migration design before touching code.
- **Decide first:** migration strategy (relabel type=2→type=1 + merge `contract_id`
  vs rebuild), the CH keying change (one row per `(code,issuer)` with native /
  soroban carve-outs), and the canonical-id wire change (`C…` deep-links).
- When this lands, archive 0336 + 0337 as `superseded by: [0339]`.

## Completion Notes (2026-07-02, PR #303)

Phase 1 (#298 / #299) + the Phase 2 prod data-pass (executed 2026-07-01) already
landed. This final session did the mechanical cleanup + SAC-UX polish and closed
the task.

### Design Decisions — Emerged

1. **`filter[sac]` deployed-only** (user-approved 2026-07-01): reserved
   (un-deployed) SACs matched the old `sac_contract_id != 0` and read as confusing
   ("SAC / not deployed", e.g. zkUNP). Narrowed to `sac.sac_deployed`.
2. **Two-axis FE badges**: the SAC badge previously _replaced_ the type badge; now
   the type badge is always shown and SAC is an additive property tag, shown only
   when `sac_deployed`. Un-deployed SACs get no tag (the reserved-address note stays
   on the detail page only). The list filter row splits SAC into a "Has SAC" toggle.
3. **Also fixed the ignored runner smoke test**: the runbook enumerated 3 predicate
   spots, but the `#[ignore]` `select_sep1_chunk_*` test seeded a synthetic `type=2`
   row and asserted its inclusion — a lie post-relabel. Reseeded to `type=1`
   (SAC-as-facet) and tightened the assertion.
4. **Fixed a stale doc**: `frontend-overview.md` §7 listed type badges with wrong
   colors (Classic=blue, SAC=violet); corrected to match `assetType.ts` and the
   two-axis model.

**Modified tests:** `assetType.test.ts` (`sac` no longer a type; added `SAC_TAG` +
filter-list assertions); `AssetDetailPage.test.tsx` (dropped the `['sac','SAC']`
case; added deployed / reserved SAC-tag cases); `AssetsListPage.test.tsx`
(`?type=sac` → `?sac=true`; added a `filter[sac]` mapping test); runner smoke-test
seeds.

**Intentionally deferred:** `assets_pre0339` backup NOT dropped (soak ongoing);
`lore/README.md` index not regenerated in this direct-to-develop commit (the MCP
`generate-index` targets the feature-branch worktree — regen on the next
develop-side lore session).
