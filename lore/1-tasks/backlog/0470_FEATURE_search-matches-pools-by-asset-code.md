---
id: '0470'
title: 'FEATURE: pool search consistency — same rules in the global box and the list filters'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0440']
tags:
  [api, search, liquidity-pools, consistency, priority-medium, effort-medium]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/366']
history:
  - date: '2026-08-10'
    status: backlog
    who: karolkow
    note: >
      Found by the regression sweep. `GET /v1/search?q=KALE` reports
      `Liquidity Pool 0` while the pools page returns 58 pools for the same
      needle and 7 for `xlm/kale`. Two different endpoints —
      `crates/api/src/search/` vs `crates/api/src/liquidity_pools/` — and
      only the second learned asset codes in task 0440.
      Direction set by Karol: the two must behave the same, not "pick the
      cheap option". The performance argument for the current id-only point
      seek was measured against real data and does not hold at this table
      size: the full pools-page predicate over every pool costs 47 ms /
      73 898 rows / 3.33 MiB, and today that search arm does nothing at all
      for a non-hash query.
  - date: '2026-08-10'
    status: backlog
    who: karolkow
    note: >
      Scope widened and 0471 folded in on Karol's call: the two were one
      subject split in two, which cuts against the standing preference for
      larger bundled tasks. Stage 1 (pools, both directions) is IMPLEMENTED on
      `feat/0470_search-pools-by-asset-code`; stage 2 (the remaining lists and
      lifting the shape recogniser) is open. Surveying every list showed the
      same gap twice more: contracts already matches its own `contract_id` in
      `filter[q]`, assets and NFTs do not. The recogniser that decides "is this
      text an identifier" also exists twice — `search::classifier` on the
      server and `directRouteFor` on the frontend, whose own doc comment
      already states the policy this task generalises.
---

# FEATURE: pool search consistency — one rule per entity, both directions

## Summary

Searching an asset code in the header search box reports zero liquidity pools
while the pools page lists dozens for the same text. The two surfaces answer
the same question differently, and the global one is wrong more often.

The requirement is **parity**, not a smaller variant of it: whatever the pools
page matches, global search matches.

## Why this matters more than it looks

Task 0440 taught the pools page to match asset codes as substrings, with `A/B`
pair syntax and a native-XLM rule — shipped 2026-08-07 and closing issue #366.
A reader who types `KALE` into the main search box now gets `Liquidity Pool 0`,
which reads as **that fix not working**. The inconsistency actively undermines
the shipped feature.

## Context — why it is id-only today

Deliberate, and documented in the code: `pool_id` is the full `ORDER BY` key,
so `pool_id = unhex(?)` is a granule-pruned point seek, and `search_pools`
fires only for a hash-shaped query (`crates/api/src/search/queries.rs:299`).
That was a sound default when nothing else matched pools.

**The cost of dropping it, measured on production:**

|                                     |                                  |
| ----------------------------------- | -------------------------------- |
| Unique pools                        | 52 472 (73 880 rows)             |
| Full-table predicate for one needle | **47 ms**, 73 898 rows, 3.33 MiB |
| Pools matched for `KALE`            | 58                               |

`/v1/search` already fans out six queries in parallel, so this lands on the arm
that currently returns nothing for non-hash input. Keep the point seek for
hash-shaped queries — it stays free — and add the code path beside it.

## Implementation

Reuse the 0440 predicate rather than writing a second one; two copies of this
rule will drift, and the native case is exactly where a re-implementation goes
wrong:

```rust
positionCaseInsensitive(if(lp.asset_{side}_type = 0, 'XLM', lp.asset_{side}_code), ?) > 0
```

- **Native XLM is stored with an empty code.** Without the `if(type = 0, …)`
  arm, `XLM` matches thousands of impostor codes and misses every real XLM
  pool — the exact bug 0440 found and fixed.
- **Pair syntax `A/B`** — each needle claims its own leg, order-insensitive.
  `normalize_asset_codes` (`liquidity_pools/handlers.rs:186`) already yields at
  most two needles; lift it rather than re-parsing.
- Extract the shared predicate so `liquidity_pools` and `search` cannot
  disagree again.

## Stage 2 — the same rule on the remaining lists (folded in from 0471)

### Not this: routing list filters through global search

Worth stating, because it is the obvious idea and it is wrong. Global search
and a list filter are different tools:

|             | `/v1/search`                          | list filter                           |
| ----------- | ------------------------------------- | ------------------------------------- |
| Purpose     | identify an entity and navigate to it | narrow a table being browsed          |
| Returns     | `SearchHit` — identifier + label      | full rows (reserves, TVL, holders, …) |
| Volume      | ≤50 per bucket, hard ceiling          | cursor pagination over the whole set  |
| Composition | one query string                      | `filter[x]` AND `filter[y]` AND sort  |
| Exact id    | redirects to the detail page          | must stay on the list                 |

A list calling search would receive identifiers with none of the columns it
renders, and lose pagination. What is genuinely duplicated is the _rules_.

### The measured gap

| List            | free-text filter     | matches                 | accepts its own id?                            |
| --------------- | -------------------- | ----------------------- | ---------------------------------------------- |
| contracts       | `filter[q]`          | `contract_id` substring | **yes**                                        |
| liquidity pools | `filter[asset_code]` | asset codes             | **stage 1**                                    |
| assets          | `filter[code]`       | code, name, symbol      | **no**                                         |
| NFTs            | `filter[name]`       | enrichment name         | **no** (separate `filter[contract_id]` exists) |

Transactions and accounts have no free-text filter — typed id filters only —
so they are out of scope until one is added.

### The policy already exists

`web/src/search/directRouteFor.ts` states it outright:

> "Adding more FE shortcuts here is rarely the right call — keep classifier
> logic on the server unless the entity type has no search bucket."

The decision that shape recognition belongs on the server, in one place, was
already taken. It never reached the list filters.

### Work

- **Recogniser to `common`.** `search::classifier::classify` is private to the
  search module. Lifted, any handler can ask "is this an identifier, and of
  what kind" with the same answer the search box gives. Stage 1 needed only the
  pool half and added `pool_id_from_text` to `common::pool_asset_codes`; that
  function must FOLD INTO the general recogniser rather than become the first
  of four copies — otherwise this task removes one duplication and creates
  another.
- **Per-entity predicates to `common`,** each called by both the list and its
  search bucket — the shape `common::pool_asset_codes` already has.

Order: assets first (an asset id is `CODE-ISSUER` or a contract StrKey, so the
recogniser has real work to do), NFTs second (a contract StrKey, and the typed
filter already exists to reuse), contracts last — only to move its existing
inline rule into the shared module.

## Acceptance criteria

- [ ] Any query that returns pools on the pools page returns THE SAME POOLS in
      global search, up to that bucket's cap — verified on `KALE`, `xlm/kale`
      and `USDC`. Equal COUNTS are not achievable and never were: search caps
      every bucket at `MAX_LIMIT` 50 (default 10), while `KALE` matches 58
      pools. The criterion is set membership within the cap, not parity of N
- [ ] Native XLM behaves identically on both surfaces (0440's rule preserved,
      not re-implemented)
- [ ] Hash-shaped queries keep the point-seek path — no scan introduced for
      the case that is free today
- [ ] The predicate exists in ONE place, shared by both endpoints
- [ ] Search latency measured before and after on a real query mix
- [ ] **Stage 2:** pasting an entity's identifier into its list's free-text
      filter selects that entity on every list that has such a filter
- [ ] **Stage 2:** shape recognition lives in ONE place, server-side;
      `pool_id_from_text` folded into it rather than left as a parallel path
- [ ] **Stage 2:** each entity's match predicate is shared by its list and its
      search bucket — no rule implemented twice
- [ ] **Stage 2:** existing behaviour preserved — a plain code/name still
      matches as before, and no list loses a filter it has today
- [ ] **Docs updated** — search contract under `docs/architecture/**` states
      what matches a pool, and the list-filter contract states that a
      free-text filter also accepts the entity identifier
- [ ] **API types regenerated** — only if the search response shape changes
