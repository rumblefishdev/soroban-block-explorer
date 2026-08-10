---
id: '0472'
title: 'FEATURE: contract pages link + name what they represent (fungible/NFT links, SAC polish from /ux-expert)'
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
      Scope extended after the /ux-expert pass over the shipped 0441 UI:
      three accepted findings added (detail header names the asset, summary
      row label "Asset", SAC chip replaces the redundant Token chip + the
      list filter label "Token" → "SAC"). The chip-vs-row question for the
      list is DECIDED (rows only, Type chips stay unlinked) — AC updated.
      Measured basis for the dedup: contract_type × is_sac cross-tab on
      prod shows Token ⟺ is_sac exactly (3,946/3,946; zero non-SAC type-0),
      so the double chip carries zero extra information.
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

Cross-links (original scope):

1. Contract detail (Fungible): an "Asset" summary row linking to
   `routes.asset(contract_id)` — same row shape as 0441's mirrored-asset row.
2. Contract detail (NFT): a "Collection" link to
   `/nfts?filter[contract_id]={C…}`.
3. Contracts list: **decided** (/ux-expert, 2026-08-10) — Type chips stay
   unlinked. A linked chip points at a DIFFERENT entity (the 0441
   `SAC · CODE` case); "Fungible"/"NFT" are category labels, and a link from
   a category label to the row's own asset would read as a filter, not
   navigation. Links live on the detail rows only.

SAC polish (accepted /ux-expert findings on the shipped 0441 UI):

4. Detail header: the `Stellar Asset Contract` chip becomes
   `Stellar Asset Contract · CODE`, linked like the list chip — the page's
   landing moment should name the asset, not just flag SAC-ness
   (frontend-overview: "SAC identification must be visually clear").
5. Summary row label: "Mirrors asset" → **"Asset"** — one plain word instead
   of invented jargon (update frontend-overview wording too).
6. Chip dedup + filter relabel (both halves accepted):
   - a SAC row shows ONLY the linked `SAC · CODE` chip — the `Token` type
     chip is dropped for `is_sac` rows (prod cross-tab: Token ⟺ SAC exactly,
     3,946/3,946, so the pair is 100% redundant);
   - the list filter label `Token` → `SAC` (UI label only; the API
     `filter[type]=token` param and `contract_type` values are unchanged).
     Add `aria-label`/tooltip with the issuer on the linked chip while there —
     the bare code is ambiguous (many issuers of "USDC" on prod).

## Acceptance criteria

- [ ] Fungible contract detail links its asset page; vitest case
- [ ] NFT contract detail links its filtered collection view; vitest case
- [x] /ux-expert pass on the chip-vs-row question for the list; decision
      recorded — rows only, Type chips stay unlinked (2026-08-10, see Scope 3)
- [ ] Detail header chip names + links the mirrored asset (`… · CODE`)
- [ ] Summary row relabelled "Asset"; frontend-overview updated
- [ ] SAC rows show a single chip (`SAC · CODE`, no `Token` chip); filter
      label reads `SAC`; API params untouched
- [ ] Linked SAC chip carries an issuer tooltip / aria-label
- [ ] No API surface change (frontend-only; no api-types regen)
- [ ] Docs: frontend-overview §6.10 updated

## Notes

Asset detail already links back to the contract (`AssetSummary`, deployed
contracts only), so after this task the contract ↔ asset relation is
navigable in both directions for every class that has one.
