---
id: '0472'
title: 'FEATURE: contract pages link the asset / collection they represent (fungible + NFT)'
type: FEATURE
status: backlog
related_adr: ['0051']
related_tasks: ['0441']
tags: [frontend, contracts, assets, nfts, priority-low, effort-small]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/368'
history:
  - date: '2026-08-10'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0441 review: the SAC chip now links its mirrored asset,
      which leaves the OTHER contract classes as the odd ones out. Measured
      on prod: every one of the 4,340 Fungible contracts has an assets row
      keyed by its own contract surrogate, and the /assets/{C…} deep-link
      already resolves it — the link is frontend-only. NFT collections are
      reachable via the existing /nfts filter[contract_id]. Zero type-0
      non-SAC contracts exist, and classic assets have no contract page, so
      SAC + these two classes close the matrix completely.
---

# FEATURE: contract pages link the asset / collection they represent

## Summary

Task 0441 made a SAC contract link the classic asset it mirrors. The same
"this contract has a face elsewhere in the explorer" relation exists for the
other two contract classes and is still unlinked:

- a **Fungible** (SEP-41) contract IS an asset — `assets` carries an
  `asset_type = 3` row keyed by the contract's own surrogate
  (4,340 of 4,340 on prod, 2026-08-10), and `/assets/{C…}` already resolves
  the contract StrKey to that asset;
- an **NFT** contract is a collection — the NFTs list already filters by
  `filter[contract_id]`.

Both are frontend-only links; no API change, no new query.

## Non-goals

- Type-0 non-SAC contracts: zero exist on prod (every type-0 is a SAC).
- Classic assets: no contract page exists to link from; the SAC case is 0441.

## Scope

1. Contract detail (Fungible): an "Asset" summary row linking to
   `routes.asset(contract_id)` — same row shape as 0441's "Mirrors asset".
2. Contract detail (NFT): a "Collection" link to
   `/nfts?filter[contract_id]={C…}`.
3. Contracts list: decide with /ux-expert whether the Type chip carries the
   link (mirroring the 0441 `SAC · CODE` chip) or the list stays plain.

## Acceptance criteria

- [ ] Fungible contract detail links its asset page; vitest case
- [ ] NFT contract detail links its filtered collection view; vitest case
- [ ] /ux-expert pass on the chip-vs-row question for the list; decision
      recorded here
- [ ] No API surface change (frontend-only; no api-types regen)
- [ ] Docs: frontend-overview §6.10 updated

## Notes

Asset detail already links back to the contract (`AssetSummary`, deployed
contracts only), so after this task the contract ↔ asset relation is
navigable in both directions for every class that has one.
