---
id: '0376'
title: 'NFT completeness: multi-owner, contract-owner, pending visibility, collection union'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-large, layer-indexer, nft]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker. Bundles K1-6, K2-5, K2-6, K3-7.'
  - date: '2026-09-29'
    status: active
    who: karolkow
    note: >
      Activated for K2-5 only (decision 303 A): contract-owned NFTs resolve to
      their C-address instead of null, in one small PR. K1-6, K2-6 and K3-7
      stay open.
---

# NFT ownership completeness

## Summary

Close the NFT ownership gaps: the single current-owner slot loses history,
contract-held NFTs show a NULL owner (22% of NFTs / 51% of transfer rows),
pending NFTs are invisible (71K), and collection activity is not unioned on the
contract page.

## Re-measured 2026-09-08 (production)

The contract-owner gap, counted from the current tables rather than the 0359
sample: **339 of 1 089 distinct NFT owners (31%) cannot be resolved through
`accounts`, and all 339 resolve in `soroban_contracts`.** So the owner is known
for every one of them — `nfts/queries.rs` simply resolves owners against
`accounts` alone, and a contract owner therefore renders as null. A read-side
omission, not missing data.

Found while sweeping for contradictions during task 0540; the shared-vocabulary
side of it is [[0542]].

## K2-5 — contract owners (2026-09-29)

**Re-measured (read-only):** 1,117 distinct current owners — 762 accounts,
355 contracts (32%), 0 in neither; contracts hold 2,851 of 13,364 owned NFTs
(21%). The list showed a dash for them and the detail page said **"Burned"**.

**Change** (branch `feat/0376-contract-nft-owners`, `f5129eac6`, local): the
list, detail and transfers queries also resolve the owner id in
`soroban_contracts` and return `owner_contract` / `from_contract` /
`to_contract` beside the account fields — the `caller_account` /
`caller_contract` convention. SPA: `OwnerIdentifier` links a contract owner to
its contract page. Verified with `bin/local` on production data: list of 100
rows — 96 accounts, 4 contracts, 0 with both, 0 with neither; token 40370 of
`CBHU…A6GR` shows mint → `CDLM…VAHL` (contract) → transfer to `GAA4…JXOG`, both
contract sides linked to `/contracts/…`; token 59104 shows owner `CC3Z…UIOP`
instead of "Burned".

**Revised after review (2026-09-29, thread 338 A):** the pair of fields was
replaced by the pool participants' shape — `owner_account`, `from_account`,
`to_account` carry a `G…` account or a `C…` contract, resolved through both
tables; the SPA links by `isContractId` (`38f556d2c`). Why: NFT storage has one
owner column in one surrogate space (like a share-token holder), and a StrKey
names its own kind; the `caller_account` / `caller_contract` pair mirrors two
stored columns. Re-verified on production data: list 96 G / 4 C / 0 null,
detail owner `CC3Z…UIOP`, transfers contract on both sides.
Open from the review: no CH-gated Rust test covers a contract owner (the local
ClickHouse has none to read); the API keeps both conventions, and the SPA
checks a StrKey's kind inline in five places.

## Context

Spawned from 0359. The NFT owner is a single-slot current value; contract owners
(C-address) are dropped like other non-G participants (overlaps 0373).

## Implementation

- **K1-6** — multi-owner / owner-history (mitigated today by `/transfers`).
- **K2-5** — resolve contract-owner NULL (C-address owners; ties to 0373 non-G).
- **K2-6** — make pending NFTs visible (71K; see memory: nfts_pending load-bearing).
- **K3-7** — union NFT collection activity onto the contract page.

## Acceptance Criteria

- [ ] owner history retained (not single-slot) — K1-6
- [x] contract-owner resolved (no NULL) — K2-5 (PR #548, deployed 2026-09-30:
      token 59104 owner `CC3Z…UIOP`; collection `CDUT…` 15 C / 5 G / 0 null)
- [ ] pending NFTs visible — K2-6
- [ ] collection activity unioned on contract page — K3-7
