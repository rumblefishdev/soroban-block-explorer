---
id: '0676'
title: 'FEATURE: contract page shows the tokens and NFTs the contract holds'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0675', '0647', '0373']
tags: [priority-low, effort-medium, layer-api, layer-frontend, nft]
links: []
history:
  - date: 2026-11-19
    status: backlog
    who: karolkow
    note: 'Created from task 0675: the account page now lists holdings, the contract page does not.'
---

# FEATURE: contract page shows the tokens and NFTs the contract holds

## Summary

A contract can hold tokens and NFTs, like an account. The account page shows
them in its Assets card (task 0675). The contract page shows only its summary
and the Interface, Code, Invocations and Events tabs, so a contract's holdings
are visible nowhere.

## Stan teraz

- Done: nothing; backlog.
- Next: grilling on the view and the scope (NFTs first or both), with a peer
  comparison, before any code.

## Context

Measured on production on 2026-11-19 (read-only, `FINAL` reads):

- 564 contracts hold 3,049 NFTs, 17% of all NFTs. The largest contract
  holder has 75. An NFT page links its `C…` owner to the contract page, where
  the NFT is missing — the same gap as issue 741, for contracts.
- 58,153 contracts hold 63,057 token balances (`balances`, `amount > 0`,
  open). Per contract: median 1, p99 3, max 69.
- `GET /v1/nfts?filter[owner]=` accepts only a `G…` account (task 0675).
- `GET /v1/accounts/:id` serves only `G…` accounts.

## Premise

Not reviewed yet. Open points to check first:

- Contract-held SAC balances: task 0647 changes where they are stored. Check
  that the rows read for a contract are correct before showing token balances.
  NFTs do not depend on it.
- Which contracts hold NFTs: the top five are type "other" contracts,
  probably marketplaces or escrows (not checked).

## Implementation Plan

### Step 1: NFTs

`filter[owner]` accepts a `C…` contract too; the contract page shows the same
NFTs view as the account page.

### Step 2: Tokens

The contract's balances on the contract page, after the 0647 check.

## Acceptance Criteria

- [ ] The contract page of an NFT-holding contract lists its NFTs, linked to
      their NFT pages.
- [ ] The contract page of a token-holding contract lists its balances.
- [ ] A contract with no holdings shows no extra section and sends no
      holdings request.
- [ ] **Docs updated** — `docs/architecture/**` for the filter and the
      frontend data contract.
- [ ] **API types regenerated** — `pnpm nx run @rumblefish/api-types:generate`.
