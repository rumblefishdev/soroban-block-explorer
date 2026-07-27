---
id: '0441'
title: 'FEATURE: SAC contract detail shows the classic asset it mirrors (reverse of the join we already run)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0339']
tags:
  [
    backend,
    api,
    frontend,
    contracts,
    sac,
    assets,
    priority-medium,
    effort-small,
  ]
links: []
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment: "if SAC, show
      classic asset name (since it is available?) — reverse lookup". Correct —
      the mapping exists and is already used in the other direction on the
      liquidity-pool endpoints. Not covered by 0339, which reshaped the data
      model rather than the contract-detail presentation.
---

# FEATURE: surface the classic asset behind a SAC contract

## Summary

A Stellar Asset Contract is the contract-side facet of a classic asset, but the
contract pages expose only a boolean `is_sac`. Show which asset it mirrors —
code plus issuer, linked to the asset detail page — instead of an unqualified
`SAC` badge.

## Current behaviour

- `crates/api/src/contracts/dto.rs:34` and `:65` expose `is_sac: bool` and
  nothing else about the mirrored asset;
  `crates/api/src/contracts/queries.rs:224` / `:389` select `sc.is_sac`.
- `web/src/pages/contracts/ContractsTable.tsx:39` renders a bare `SAC` chip.

## Why this is cheap

The mapping is already in the database and already queried — in the opposite
direction. `crates/api/src/liquidity_pools/queries.rs:288-341` resolves
`(asset_code, issuer_id)` → `asset_sac.sac_contract_id` → `soroban_contracts`
to attach a SAC contract to a classic pool leg. This task needs the same join
read the other way: `soroban_contracts.id` → `asset_sac` → `(asset_type,
asset_code, issuer_id)`.

Note `asset_sac` requires a `GROUP BY` collapse before use (see the existing
subquery at `:293-295`) — it is not one row per contract by construction.

## Scope

1. Contract detail + contract list queries: left-join the mirrored asset when
   `is_sac`.
2. DTO: replace the bare boolean with the boolean plus optional
   `{ asset_code, issuer, asset_id }`; keep `is_sac` for callers that only
   need the flag.
3. Frontend: badge becomes `SAC · USDC` linking to the asset detail page; the
   detail page gains a "Mirrors asset" row.

## Acceptance criteria

- [ ] Contract detail returns the mirrored classic asset when `is_sac`
- [ ] Reverse join collapses `asset_sac` duplicates (mirror the LP subquery)
- [ ] `is_sac` true with no resolvable asset degrades to the current bare badge
- [ ] Native (XLM) SAC handled — a positive surrogate, not an empty issuer
- [ ] Frontend links the asset; StrKey of the contract stays canonical
- [ ] **Docs updated** — contract endpoint contract under
      `docs/architecture/**` per ADR 0032
- [ ] **API types regenerated** — touches `crates/api/**`; run
      `npx nx run @rumblefish/api-types:generate`

## Notes

Native XLM carries two competing conventions in this codebase (positive
surrogate from `hash64("native")` vs empty string). Use the surrogate form; the
empty-string form falls through filters silently.
