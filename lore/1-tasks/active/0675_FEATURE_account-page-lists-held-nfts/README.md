---
id: '0675'
title: 'FEATURE: account page lists the NFTs the account holds'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0376', '0493']
tags: [priority-medium, effort-medium, layer-api, layer-frontend, nft]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/741']
history:
  - date: 2026-11-18
    status: backlog
    who: karolkow
    note: 'Created from issue 741: a user could not find an NFT on the account page.'
  - date: 2026-11-18
    status: active
    who: karolkow
    note: 'Activated: option A chosen — an NFTs card under Balances, shown only when the account holds one, fed by filter[owner] on GET /v1/nfts.'
---

# FEATURE: account page lists the NFTs the account holds

## Summary

The account page shows no NFTs. A user who holds one finds it only as a
"+1 NFT #N" row in the account's transaction history. Add a section that lists
the NFTs the account holds now, each linked to its NFT page.

## Stan teraz

- Done: `filter[owner]` on `GET /v1/nfts` with a CH-gated test (red without
  the filter, red again with the filter matched on unmerged rows); NFTs card
  on the account page; docs; API types.
- Next: look at the card on real data (local API on production CH), then
  the PR — held while the open logic-PR queue is at 6 or more.
- In force: option A (2026-11-18) — a separate NFTs card under Assets,
  shown only when the account holds at least one indexed NFT; reuses
  `NftsTable` without the owner column; no account sub-resource.
- Cost measured 2026-11-18 on production (`system.query_log`): the filtered
  page takes ~190 ms and ~385 MiB for an account with no NFTs, ~410–510 ms
  for the 2,350-NFT holder; today's unfiltered list page takes ~250 ms and
  ~80 MiB. The extra memory comes from the `nft_enrichment` / mint joins
  once a non-key filter is applied, not from the owner lookup (~2 MiB).

## Context

Issue 741: account `GDMJSG3KXP6TNWMGBQG44NS6FM7Z5NCUCA7SRSW3FBNIKNZPUYDXEQ6F`
holds token `3` of collection
`CDC5ZX53PWDPVS5AGZPHJ6WHFPB66AZFFEHK4NCFH2AR7ZZQUKOGJFGB`, and the reporter
could not see it under the account's assets.

Why it is missing (read on `develop` at 09c8aab9d):

- `fetch_balances` (`crates/api/src/accounts/queries.rs`) joins `balances` to
  `assets` with `INNER JOIN`. An NFT collection has no `assets` row, so an NFT
  never reaches the Balances section.
- `AccountDetailPage.tsx` renders Summary, Balances, Signers, Sponsorship and
  Transactions — no NFT section.
- `GET /v1/nfts` filters by collection, contract id and name only
  (`crates/api/src/nfts/dto.rs` `ListParams`); there is no owner filter.

The data is already there: `nfts.current_owner_id` names the holder, and
contract holders resolve since task 0376 (K2-5).

## Premise

Reviewed 2026-11-18 by a fresh reviewer given only the goal and constraints:
**holds, with changes** (owner filter on the existing list, matched after the
dedup; empty state honest about completeness). Detail in the note below.

Measurements, the premise review and the peer comparison:
[notes/R-measurements-premise-peers.md](notes/R-measurements-premise-peers.md).

## Implementation Plan

### Step 1: API

Owner filter on the NFT list, or an account sub-resource — to be decided.
Reads `nfts` deduplicated per `(contract_id, token_id)`, keyset-paginated.

### Step 2: Frontend

A section (or tab) on the account page, built on the shared `DataListCard`
like its siblings; each row links to `/nfts/:contract/:tokenId`. Empty state
when the account holds none.

## Acceptance Criteria

- [ ] The account page of the account in issue 741 lists token `3` of
      `CDC5ZX53…GJFGB`, linked to its NFT page.
- [ ] An account with more NFTs than one page (e.g. the 2,350-NFT holder)
      paginates.
- [x] An account with no NFTs shows no NFTs card (option A: the card appears
      only when the first page has rows; a failed fetch still shows its error).
- [x] A CH-gated test: an NFT with unmerged rows for two owners lists only
      under its latest owner.
- [x] **Docs updated** — `docs/architecture/**` for the new endpoint or filter
      and the frontend data contract.
- [x] **API types regenerated** — `pnpm nx run @rumblefish/api-types:generate`.

## Notes

- Out of scope: NFTs in `nfts_pending` (task 0392), names and media of
  unenriched NFTs (token `3` above has neither).
