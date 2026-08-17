---
id: '0483'
title: 'FEATURE: contract detail names the asset / collection it is (not just its class) — needs an API field'
type: FEATURE
status: backlog
related_adr: ['0051']
related_tasks: ['0472', '0441']
tags: [api, frontend, contracts, assets, nfts, priority-medium, effort-small]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/368'
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Scope extended after the 0472 deploy: the same header chip is also the
      only chip in the app rendered at a different size and in a different
      register from every other surface. Measured on production, not inferred
      — see `## The chip is also inconsistent`. Bundled here rather than
      spawned as its own task because both halves change the same component's
      presentation; the size half is frontend-only and need not wait for the
      API field.
  - date: '2026-08-13'
    status: backlog
    who: karolkow
    note: >
      Third layer of the same asymmetry issue #368 started. 0441 linked the
      SAC's asset; 0472 gave every contract class a linked header chip — but
      only the SAC chip can NAME what it points at, because `sac_asset` is the
      one identity the contract endpoint returns. Closing the gap needs an API
      field, so it did not belong in a frontend-only task.
---

# FEATURE: the header should name the asset / collection, not only the class

## The pattern, three layers deep

Someone asked (#368) for a SAC contract page to show the classic asset it
mirrors. Each round of work has closed one layer and revealed the next:

| layer                        | fixed in  | what was left                             |
| ---------------------------- | --------- | ----------------------------------------- |
| SAC links its asset          | 0441      | Fungible and NFT contracts linked nothing |
| every class links what it IS | 0472      | only SAC can NAME it                      |
| every class NAMES what it is | this task | —                                         |

## What it looks like now

```
SAC        Stellar Asset Contract · POYE   → /assets/POYE-GCBP…
Fungible   Fungible                        → /assets/CBR6…BVKL
NFT        NFT                             → /nfts?contract=CBHU…A6GR
```

The SAC chip says which asset. The other two say only what kind of thing this
is; the name is one click away.

## Why 0472 stopped there

`ContractDetailResponse` carries eleven fields and none of them is a name:

```
contract_id, contract_type, contract_type_name, deployed_at_ledger, deployer,
is_sac, sac_asset, stats, upgradeable, wasm_hash, wasm_uploaded_at_ledger
```

`sac_asset` was added by 0441, which is why the SAC case works for free. A
Fungible's SEP-41 symbol lives behind `/assets/{contract_id}`; an NFT
collection's name lives in the NFT data. Options, none of them frontend-only:

| option                                                     | cost                                                     |
| ---------------------------------------------------------- | -------------------------------------------------------- |
| add a display-name field to the contract endpoint          | API change + api-types regen; one extra join server-side |
| have the page fetch the second endpoint                    | an extra request per contract page, for one word         |
| leave the class label, keep the link (**shipped in 0472**) | free                                                     |

## Scope

1. Decide the shape. A single nullable `display_name` (whatever the class
   makes available) keeps the wire thin and the frontend dumb; separate
   per-class fields mirror the data sources but push the branching outward.
2. Serve it: Fungible from the token metadata already joined for `/assets`,
   NFT from the collection name (`nft_enrichment.collection_name` — the same
   column task 0482 wants for search).
3. Frontend: `contractFace` fills the label from it — the switch and the tests
   already exist, only the label source changes.
4. Regenerate `libs/api-types` (CI gate `API types freshness`).

## The chip is also inconsistent

Reported after the 2026-08-17 deploy by comparing two sibling detail pages —
`/contracts/CB23…OUOV` and the asset it links to, `/assets/KALE-GBDV…KALE`.
Measured with computed styles on production:

|                 | asset detail | contract detail                 |
| --------------- | ------------ | ------------------------------- |
| font-size       | 12px         | 14px                            |
| font-weight     | 700          | 500                             |
| padding         | 2px 8px      | 4px 12px                        |
| rendered height | 21px         | 28px                            |
| label           | `SAC`        | `Stellar Asset Contract · KALE` |

Same font family (Satoshi) — the "different typeface" impression is the weight
swap, on a chip a third taller.

**The contract page is the outlier, on both axes.** `size="md"` occurs in
exactly one file in the whole frontend — `ContractDetailPage.tsx:132` (the face
chip) and `:139` (the 0327 upgradeability badge). Every other chip in the app —
asset header, account header, all tables, transactions, ledgers — is `sm`. The
long register is equally alone: `Stellar Asset Contract` renders on the contract
detail only, while five surfaces say `SAC` (asset header, assets table, account
balances, contracts table, contracts list filter).

The counter-case is worth stating rather than assuming away: a page header may
legitimately carry a larger chip than a table row. If that is the intent, the
fix is the reverse one — asset and account headers move up, and the app grows a
documented header-chip size. What must not survive is one page differing from
every other by accident.

Note the register question is not independent of this task's main scope: item 3
of `## Scope` rewrites what the label says, so decide the wording once, for both.

## Watch out

527 type-3 assets on prod have neither code nor symbol, and 75% of NFT
enrichment rows have no per-token name. The field must be nullable and the
chip must fall back to the bare class label — never invent a name (0472
already learned this: feeding the title's "Asset" fallback to a letter avatar
rendered a confident "A" that read like a real ticker).

## Acceptance criteria

- [ ] Contract detail names the asset for Fungible and the collection for NFT
- [ ] Nullable end to end; unnamed contracts keep the plain class chip
- [ ] `libs/api-types` regenerated in the same commit as the API change
- [ ] vitest cases for named and unnamed, per class
- [ ] Docs: backend-overview (contract endpoint) + frontend-overview §6.10
- [ ] Contract detail chips match the rest of the app (one size decision,
      applied everywhere, not per page)
- [ ] One register for the SAC label across all six surfaces
