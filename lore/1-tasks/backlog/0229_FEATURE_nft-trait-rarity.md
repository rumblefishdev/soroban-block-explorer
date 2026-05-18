---
id: '0229'
title: 'NFT trait rarity ("X% have this") for detail page'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0076']
tags: [priority-low, effort-medium, layer-api]
milestone: 2
links: []
history:
  - date: 2026-05-18
    status: backlog
    who: claude
    note: 'Spawned from 0076 future work — Figma trait card has a rarity line with no data source.'
---

# NFT trait rarity ("X% have this") for detail page

## Summary

The Figma NFT detail design (`262:17522`, "Traits") renders each trait card with
a third line — e.g. `12% have this` — showing how common that trait value is
within the collection. Task 0076 shipped the trait cards **without** this line:
the `GET /v1/nfts/:id` metadata is the raw off-chain JSON and carries no
rarity/supply data, and per-trait rarity needs collection-wide aggregation.

## Context

Spawned from task 0076 (frontend NFTs pages). The frontend `NftMetadata`
component is ready to render a rarity line — it just has nothing to render.
Rarity = `count(NFTs in collection with this trait_type=value) / collection size`.

## Implementation

- API: extend the NFT detail response (or a sibling endpoint) with per-attribute
  rarity, e.g. `attributes[].rarity_pct`.
- DB: aggregate trait counts per `(collection, trait_type, value)`. Decide
  precompute vs on-request; NFT metadata is JSONB-ish so consider a materialised
  trait-count table populated by enrichment.
- Frontend: add the `X% have this` line to `NftMetadata`'s `TraitCard`
  (`web/src/pages/nft-detail/NftMetadata.tsx`) once the field exists.
- Regenerate `libs/api-types` if the API schema changes.

## Acceptance Criteria

- [ ] NFT detail trait cards show a rarity percentage line
- [ ] Rarity computed against the NFT's own collection
- [ ] Graceful when a collection is too small / rarity unknown
