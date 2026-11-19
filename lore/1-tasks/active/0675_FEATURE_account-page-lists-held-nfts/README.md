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

- Done (PR #743): `filter[owner]` on `GET /v1/nfts` served by the join-free
  `fetch_held_by`; `nft_count` in `GET /v1/accounts/:id`; NFTs as a second tab
  of the Assets card; docs; API types. Reviews: /code-review (both axes),
  /ponytail-review, /devils-advocate, /ux-expert — findings applied.
- Next: look at the card on real data (local API on production CH) once the
  certificate links exist; then merge.
- In force: NFTs are a tab of the Assets card — `Tokens N | NFTs N`, only when
  `nft_count` > 0, `?holdings=nfts` (2026-11-18, replaces the separate card).
  Read shape B — no JOIN for the owner read; `filter[owner]` refuses other
  filters. Server join settings stay as they are (measured: a thread cap would
  slow the transactions list, ~10 cores per query).
- In force: the switch is `Tokens N` / `NFTs N` pills under the card title, in
  place of the token counter (2026-11-19, chosen from five prototypes, branch
  `proto/0675-holdings-switch`).
- In force: the NFT list stays its own cursor request, not part of
  `GET /v1/accounts/:id` (2026-11-19). Inside the account response it would
  have no size bound (651 KiB for the largest holder today, 283 B per NFT),
  and one failed NFT read would fail the whole account page.
- In force: an NFT whose contract has no `soroban_contracts` row fails the NFTs
  view (500), loudly, instead of being dropped (2026-11-19). 0 of 17,532 NFTs
  on production; the writer ends `soroban_contracts` before `nfts`, so such a
  row means an indexer defect.
- In force: the NFTs view has no header row, like the Tokens list beside it,
  and declares its own two columns (NFT, Contract ID) instead of a `hideOwner`
  flag on the `/nfts` table (2026-11-19); `ExplorerTable` takes `showHeader`.
- Cost on production (`system.query_log`, 3 owners × 3 runs): the shared list
  shape took 155–385 MiB / 190–400 ms; `fetch_held_by` takes 4–10 MiB /
  24–35 ms plus two name lookups of ~2.6 MiB; the count takes 8 ms / 7 MiB.

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

- [x] The account page of the account in issue 741 lists token `3` of
      `CDC5ZX53…GJFGB`, linked to its NFT page (local API on production CH,
      2026-11-19: `nft_count` 1, the list returns `Stroopies 3`).
- [x] An account with more NFTs than one page (e.g. the 2,350-NFT holder)
      paginates (2026-11-19: 118 pages of 20, 2,350 rows, 2,350 unique,
      equal to `nft_count`).
- [x] An account with no NFTs shows the Assets card without an NFTs tab and
      sends no NFT request.
- [x] A CH-gated test: an NFT with unmerged rows for two owners lists only
      under its latest owner.
- [x] **Docs updated** — `docs/architecture/**` for the new endpoint or filter
      and the frontend data contract.
- [x] **API types regenerated** — `pnpm nx run @rumblefish/api-types:generate`.

## Notes

- Out of scope: NFTs in `nfts_pending` (task 0392), names and media of
  unenriched NFTs (token `3` above has neither).
