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

- Done: gap measured on production; premise reviewed; peers compared.
- Next: one PR — `filter[owner]` on `GET /v1/nfts` + an NFTs card on the
  account page.
- In force: option A (2026-11-18) — a separate NFTs card under Balances,
  shown only when the account holds at least one indexed NFT; reuses
  `NftsTable` without the owner column; no account sub-resource.

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

### Measured 2026-11-18 (production, read-only `chq`)

| What                                                                                   | Count                               |
| -------------------------------------------------------------------------------------- | ----------------------------------- |
| NFTs in `nfts`                                                                         | 17,501                              |
| NFTs with no current owner — all burned (last ownership event is type 2 for every one) | 1,438 (8%)                          |
| Distinct holders                                                                       | 1,420 — 856 accounts, 564 contracts |
| NFTs held by accounts                                                                  | 13,014 (74%)                        |
| NFTs per holding account, p50 / p90 / p99 / max                                        | 3 / 40 / 75 / 2,350                 |
| Holding accounts with exactly 1 NFT / with more than 25                                | 315 / 110                           |
| Most collections held by one account                                                   | 9                                   |
| Rows in `nfts_pending` (not visible anywhere)                                          | 279                                 |

Counts read `nfts` deduplicated with `argMax(current_owner_id,
current_owner_ledger)` per `(contract_id, token_id)` (the table is a
ReplacingMergeTree and carries unmerged duplicates).

So the list must paginate (max 2,350), and the same section fits a contract
page too (564 contract holders).

## Premise

Reviewed 2026-11-18 by a fresh reviewer given only the goal and constraints.
Verdict: **holds, with changes.**

- The problem is real: nothing lists NFTs by owner, so a link or hint cannot
  fix it. Any fix needs an owner filter in the API.
- NFTs as rows of `balances` would be the wrong model: a per-token asset row
  distorts holder counts, a per-collection count loses the token, and the
  extractor skips non-`i128` balances (`crates/xdr-parser/src/state.rs`).
- Reviewer's recommendation (not decided): `filter[owner]` on the existing
  `GET /v1/nfts` — one predicate in the `page` CTE of
  `crates/api/src/nfts/queries.rs` — instead of an account sub-resource that
  would fork the query; reuse `NftsTable` without the owner column.
- `current_owner_id` agrees with the last `nft_ownership_changes` owner for
  all 17,501 NFTs (0 mismatches). Both come from the same events; nothing
  checks them against contract storage.
- The owner filter must apply after the dedup: one key has unmerged rows
  with different owners, so filtering before `argMax`/`FINAL` would list an
  NFT under its previous owner. Pin it with a CH-gated test.
- Completeness: NFTs of contracts the classifier misses and `nfts_pending`
  rows stay invisible (task 0392), so the empty state says "no indexed
  NFTs", not "holds no NFTs".
- False-positive risk: a contract with both `owner_of` and `decimals`
  classifies as NFT (`crates/xdr-parser/src/classification.rs`); no sign of
  it in today's data.

## How others do it

Checked 2026-11-18 from docs and open code; live pages not opened.

| Explorer                     | Account page shows NFTs as                                                                                         | Source                                                                                   |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------- |
| Blockscout                   | "Tokens" tab with an NFTs sub-tab (list and collection views, each paginated); "Token transfers" is a separate tab | `blockscout/frontend` `src/slices/address/pages/details/tokens/address-tokens-layout.ts` |
| Solscan                      | Portfolio tab with Tokens, Positions, NFTs, Domains sub-tabs; "NFT Activities" separate                            | kb.solscan.io/exploring-account-portfolio-page                                           |
| Etherscan                    | "NFT Transfers" tab (history); holdings page with Assets, NFT and Liquidity Pools sections                         | docs.etherscan.io, Etherscan help center                                                 |
| stellar.expert, StellarChain | unverified (JS-rendered pages)                                                                                     | —                                                                                        |

Common pattern: holdings in their own section, apart from both fungible
balances and transfer history. The Stellar data API of Alchemy offers NFTs by
owner G-address, optionally filtered by contract.

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
- [ ] An account with no NFTs shows "no indexed NFTs", not an error.
- [ ] A CH-gated test: an NFT with unmerged rows for two owners lists only
      under its latest owner.
- [ ] **Docs updated** — `docs/architecture/**` for the new endpoint or filter
      and the frontend data contract.
- [ ] **API types regenerated** — `pnpm nx run @rumblefish/api-types:generate`.

## Notes

- Out of scope: NFTs in `nfts_pending` (task 0392), names and media of
  unenriched NFTs (token `3` above has neither).
