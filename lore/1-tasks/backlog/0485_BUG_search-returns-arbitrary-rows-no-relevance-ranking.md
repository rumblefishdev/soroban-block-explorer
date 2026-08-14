---
id: '0485'
title: 'BUG: search returns arbitrary rows — no relevance ranking in three of four buckets'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0472', '0470', '0318']
tags: [backend, search, clickhouse, priority-high, effort-medium]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/368'
history:
  - date: '2026-08-13'
    status: backlog
    who: karolkow
    note: >
      Renumbered 0482 -> 0485 on 2026-08-13: 0482 was already taken by
      `0482_BUG_op-selection-url-state-ownership` on branch
      fix/0482_op-selection-url-state. Split out of 0472. The asset-bucket fix was implemented and measured
      there, then REVERTED off that branch (commit aea53f01) once the same
      defect turned up in two more buckets and the NFT one proved to need a
      redesign, not a sort. Everything below is measured on production.
---

# BUG: search answers with whichever rows the scan reached first

## The defect

Three of the four search buckets end in `LIMIT {per_group_limit}` with **no
`ORDER BY`**. ClickHouse returns whichever rows the scan reached first and
stops. There is no relevance ordering, and two identical calls may disagree.

| bucket                          | state                                                                |
| ------------------------------- | -------------------------------------------------------------------- |
| accounts                        | `ORDER BY account_id` — fine                                         |
| contracts, StrKey-prefix mode   | `ORDER BY contract_id` — fine                                        |
| **assets** (`queries.rs` ~657)  | `LIMIT`, no `ORDER BY`                                               |
| **contracts, text mode** (~553) | `LIMIT`, no `ORDER BY`                                               |
| **NFTs** (~777)                 | `LIMIT`, no `ORDER BY`                                               |
| **pools, code mode**            | `ORDER BY newest DESC` — deterministic, but freshness, not relevance |

The pools row is new. Until task 0470 shipped (#409), pools matched an exact
`pool_id` only — at most one hit, so there was nothing to rank and the bucket
does not appear in the original three-of-four count. It is now a fourth
text-matching bucket, and it does have an `ORDER BY`, so it is not part of the
"two identical calls may disagree" defect. It is still not RELEVANCE: `XLM`
matches 14 971 pools and the bucket returns 20 of them by which pool traded
most recently, so an exact-code leg has no advantage over a substring one.
Whatever tier ranking this task lands should extend to it.

The retired `0471` reference was dropped from `related_tasks` — that task was
folded into 0470 and its file retired rather than left as a second task on one
subject.

## What users get today (measured on prod)

| query              | result                                                                           |
| ------------------ | -------------------------------------------------------------------------------- |
| `USDC`             | ten `IUSDC` rows — **the real USDC is absent**                                   |
| `AQUA`             | `JAQUARS`, `LAQUA`, `LAQUA`, `LITEAQUARIUS`, then `AQUA` (5th)                   |
| `XLM`              | substring lookalikes only — **XLM absent**                                       |
| `Talk` (contracts) | "Take the Red Pill, Talk to Legal" 1st; the contract named exactly `Talk` is 4th |
| `spiko` (NFTs)     | **zero hits**, though the collection has 3,621 tokens — see below                |

## Bucket 1 — assets: solved, needs re-applying

The reverted commit did this and it worked (`q=usdc` → the Circle USDC first,
`q=xlm` → native first, `q=AqUa` → AQUA first):

- rank by tier: exact code > prefix > substring anywhere — the order follows
  from what MATCHED, not from an invented weighting;
- tie-break on holder count from `balance_aggregates` (441 assets carry the
  code `USDC`; Circle's has 613,691 holders vs 3,098 for the next);
- trailing PK columns so the order is total — holder counts are NULL for most
  rows and determinism is half the fix.

Measured trade-offs, all on prod:

| variant                                            | cost              |
| -------------------------------------------------- | ----------------- |
| scan alone (case-insensitive)                      | 28 ms             |
| + `balance_aggregates` via `GROUP BY` subquery     | 93 ms             |
| + same join, bare (the table is 1:1 on `asset_id`) | **71 ms**         |
| case-SENSITIVE PK-anchored probe (rejected)        | 13 ms / 51k rows  |
| case-insensitive folded (`lower()` on the key)     | 41 ms / 497k rows |

`lower()` on the sort-key column forfeits primary-key pruning. Predictable
matching still wins: a search where `usdc` misses `USDC` is broken however
fast. Endpoint latency is ~330 ms with ~47 ms of run-to-run jitter, so the
added ~43 ms is below the noise floor.

## Bucket 2 — contracts: rank, but do not invent a popularity signal

- only **3,893 of 140,000** contracts have a name, so this bucket covers 2.8%
  of the corpus;
- **327 name groups are duplicated**; the worst is `Pool Share Token` × **481**;
- invocation count is NOT usable as a tie-break: `soroban_invocations_appearances`
  is 1.45 billion rows / 18.8 GiB and the naive join hit
  `Code 241 … would use 3.73 GiB` — the exact trap this module's comments warn
  about. A bounded `IN`-list lookup costs 12 ms per contract but adds a third
  round-trip.

So: tier ranking plus a deterministic tail, no popularity term. With 481
identically-named contracts no ordering helps the user pick — that is the
display problem below, not a ranking one.

## Bucket 3 — NFTs: this one is not a sorting bug

`search_nfts` matches `nft_enrichment.name`, the **per-token** name. Measured:

|                                    |               |
| ---------------------------------- | ------------- |
| enrichment rows                    | 44,200        |
| with a token name                  | 10,897 (~25%) |
| collections in the system          | **66**        |
| `spiko` matches by token name      | **0**         |
| `spiko` matches by collection name | **3,621**     |

The largest collections carry empty token names, so `DeFindex-Vault-Meru`
(14,656 tokens) and the Spiko funds are **invisible to search entirely**.
And nobody searches for `Meridian 2025 66 #66749` — people search for the
collection.

The bucket should match `collection_name` and return COLLECTIONS (66 of them),
routing to the collection view. That view only became addressable in 0472
(`/nfts?contract={C…}`); there is still no dedicated collection page. Ranking
then falls out trivially: exact name > prefix, tie-break by collection size.

## Acceptance criteria

- [ ] Assets bucket ranked (re-apply the reverted work); `q=USDC` returns USDC
      first, `q=XLM` returns XLM, `q=AqUa` behaves like `q=aqua`
- [ ] Contracts text bucket ranked; exact name wins; deterministic tail
- [ ] NFT bucket decision recorded: collections vs tokens. If collections —
      match `collection_name`, route to the collection view
- [ ] Every bucket's order is total (repeat calls agree)
- [ ] API params and response SHAPE unchanged; RESULT ORDER changes — call it
      out in the PR, this is not a no-op refactor
- [ ] Cost measured per bucket against the ~330 ms endpoint baseline
- [ ] Docs: backend-overview search section

## Not in scope

Making hits distinguishable (441 rows all displaying `USDC`) — split to
[[0484]]. Ranking picks the best FIRST hit; that task is about telling the
others apart. It turned out to be frontend-only: the issuer already ships
inside `route_token`. Options 2-3 there (TOML domain, holder count) would
need this task's backend work and can ride along with it.
