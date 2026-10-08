---
id: '0633'
title: 'Token metadata for contracts whose functions read persistent contract data'
type: FEATURE
status: backlog
related_adr: ['0061']
related_tasks: ['0620']
tags: ['effort-medium', 'priority-low', 'soroban', 'tokens']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-07
    status: backlog
    who: karolkow
    note: >
      Spawned from 0620 (decision deferred by the owner). 17 token contracts
      on 10 programs read decimals/name/symbol from persistent contract data,
      which the indexer reads and discards.
---

# Token metadata for contracts whose functions read persistent contract data

## Summary

Task 0620 runs a token's `decimals()` / `name()` / `symbol()` locally over the
program bytes (`wasm_programs.code`) and the contract instance
(`contract_instances`). 17 token contracts keep those values outside the
instance, in their own persistent contract-data entries, which we do not
store — so the local run cannot answer for them. Decide where those entries
come from.

## Context

Measured 2026-10-07: `simulateTransaction` of the three functions on one
contract per token program (330 programs, 4,211 contracts whose interface
exports `decimals` and `balance`), footprint read from the result.

| Program                                        | Contracts | Persistent entries read                                        | Activity              | Metadata stored today     |
| ---------------------------------------------- | --------- | -------------------------------------------------------------- | --------------------- | ------------------------- |
| `016c4bae`                                     | 3         | ~30 vault config entries (`BorrowCap`, `AccumulatedInterest`…) | ~2,300 txs each, live | none                      |
| `547a076f`                                     | 3         | `Name`, `Symbol`, `Decimals`                                   | 24–33 txs             | none                      |
| `5476efd1`                                     | 4         | `Name`, `Symbol`, `Decimals`                                   | 1–8 txs               | none                      |
| `0ecdea7b`, `9d25d52c`, `d08e655a`, `5fce4f88` | 4         | `token_data` / `TokenData`                                     | 2–4 txs               | none                      |
| `8f1a1bd1`, `d65b3bb0`                         | 2         | `Admin`, `MaxSupply`, `PendingAdmin`                           | 2–7 txs               | 1 of 2                    |
| `df06cfa8`                                     | 1         | as `016c4bae`                                                  | 5 txs                 | wrong: name = contract id |

Why they are missing: the indexer sees every persistent contract-data change
but keeps only shapes it recognises — `Balance(Address)` → `balances`, pool
state, CAP-85 executable tags. Every other persistent entry is decoded and
dropped. Storing all of them means every holder's balance as well (millions
of entries), so "store everything" is not the option; "store non-balance
entries" is, at an unmeasured size.

## Options (from the 0620 discussion)

- **A.** One-off command fills the 17 from RPC; new contracts of these
  programs wait for a re-run.
- **B.** The indexer asks RPC on a missing entry — couples ingestion to an
  external, rate-limited service.
- **C.** Store non-balance persistent contract data (new table + one-off
  fill). Measure its size first, e.g. from a checkpoint bucket list.
- **D.** The indexer records a contract whose run missed an entry; the
  enrichment worker (already calling RPC for NFT `token_uri`) completes it.

## Acceptance Criteria

- [ ] Size of non-balance persistent contract data measured (needed to
      weigh C).
- [ ] Option chosen with the owner.
- [ ] The 17 contracts have `decimals`, `name`, `symbol` equal to RPC
      simulation; `CCHKJVFJDF3NCMQPKGRUMMEOZDLHERF2ZYAT24VNX35OSDZKEBLIHQFY`
      no longer stores its contract id as its name.
- [ ] A newly deployed contract of one of these programs gets its metadata
      without a manual step (unless A is chosen deliberately).
