---
id: '0440'
title: 'FEATURE: LP list asset filter — substring + pair syntax (exact-match only today; explicitly not user regex)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0371']
tags:
  [
    backend,
    api,
    frontend,
    liquidity-pools,
    search,
    priority-medium,
    effort-small,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/366'
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment ("wished someone
      implemented regex for search in pools"). Investigation found the filter is
      weaker than the reporter assumed — exact match, not substring — and that
      the input placeholder overpromises. Scoped to substring + pair syntax;
      user-supplied regex deliberately rejected (see Rejected below).
---

# FEATURE: LP list asset filter — substring + pair syntax

## Summary

The liquidity-pool list filter matches a whole asset code exactly, so `USD`
returns nothing for `USDC` pools, and there is no way to filter by a _pair_
despite the input inviting it. Add substring matching and a `A/B` pair syntax.

## Current behaviour

`crates/api/src/liquidity_pools/queries.rs:975-979`:

```
AND (upper(lp.asset_a_code) = ? OR upper(lp.asset_b_code) = ?)
```

Exact equality on an upper-cased code, one code per request. Consequences:

- `USD` does not match `USDC` — the input has no partial matching at all.
- Only one leg can be constrained; there is no pair filter.
- `web/src/pages/liquidity-pools/PoolsFilterBar.tsx:65` labels the field
  **"Filter by asset pair…"**, which the backend cannot honour. The placeholder
  is the immediate user-visible defect even if the query is left alone.

Distinct from the global search bar (task 0271, completed) — that is a separate
endpoint and does not back this filter.

## Scope

1. Substring match on either leg's code, anchored to a sane minimum length
   (2–3 chars) to keep the scan bounded.
2. Pair syntax: `USDC/XLM` constrains both legs, order-insensitive.
3. Fix the placeholder to describe what the field actually does.

## Rejected: user-supplied regex

The original request was for regex. Not shipping it on a public endpoint: an
arbitrary caller-supplied pattern is an unbounded-backtracking risk against the
read-only ClickHouse profile, on top of the existing read-row quota. Substring
plus pair syntax covers the realistic uses (`USD…`, `…BTC`, `USDC/XLM`) without
handing a query planner to anonymous callers. Record the reasoning in the reply
to the reporter, not just here.

## Implemented 2026-07-31 (feat/0462 branch, awaiting deploy)

Measured against production ClickHouse before and after each decision.

**The min-length rationale in "Scope" was wrong.** It said a guard keeps the
scan bounded; the scan is full either way — `liquidity_pools` is ordered by
`pool_id`, so this filter never pruned on the sort key. Equality and substring
both read ~93.5k rows (~15 ms) through the real `page` CTE, `FINAL` included.
The 2-character guard stays, for usefulness: one character matches most of the
list and reads as a broken filter.

**Native legs were the real bug, and it pre-dated this task.** A native leg
stores `asset_type = 0` with an EMPTY `asset_code`, so `XLM` matched none of
the 16,578 pools that hold XLM — while matching ~740 pools whose leg is a
credit asset someone named "XLM". The filter answered with impostors and hid
the real thing. Both legs are now read through
`if(asset_type = 0, 'XLM', asset_code)`; `XLM` went from a handful of
lookalikes to 21,590 pools, at identical cost.

**Pair sides are EXACT, single fragments are substrings.** Verified on rows,
not counts: substring sides answered `XLM/USDC` with 197 pools including
`DXLM/USDC`, `native/yUSDC` and `USDC/LibreXLM`; exact sides answer with 63,
all genuinely that pair (native-leg pools included). A pair names two assets —
same lookalike-noise argument as the native fix. The reported complaint was
about single fragments (`USD` must find `USDC`), and that stays a substring.

Measured cost through the real query, `FINAL` included:

| Filter                | rows_read |
| --------------------- | --------- |
| none (baseline page)  | 93,552    |
| old exact-match `USD` | 93,530    |
| new substring `USD`   | 93,552    |
| new pair `XLM/USDC`   | 96,507    |

Rejected input returns `400 invalid_filter` — `u`, `USDC/`, `/xlm`, `a/b/c`.
SQL generation was extracted from `fetch_pool_list` into `push_asset_filter`
so the emitted predicate and its bind order are unit-testable without a
database.

Not verified locally (no local ClickHouse with data): the HTTP path itself —
the 400 envelope and the rendered list. Both are post-deploy checks.

## How the field compares (first-hand, 2026-07-31)

Each behaviour below was typed into the live product, not read from docs.

**GeckoTerminal** (largest DEX analytics, covers Raydium) — one free-text box.
`USD` matches as a substring across symbols AND names (`USD1Swap`, `USDFI`,
"USD Coin", "Tether USD"); `SOL/USDC` returns exactly SOL/USDC pools across
four DEXes, so the `/` convention we adopted is theirs too. Results are
grouped by entity type, and every row carries identity: a verified tick, a
numeric security score, the token address and the pool address. (Whether
their pair is a strict AND was not confirmed — the modal wedged during the
nonsense-pair probe.)

**Aquarius** (the main Stellar DeFi hub) — "Search by token name or token
address". `USD` returns "There's nothing here" — the exact-match limitation
this task removes. `XLM/USDC` returns results, but so does
`XLM/ZZZZNONSENSE`, so their separator splits and matches EITHER side; it is
not a pair constraint. Every pool row is labelled with full names and issuer
home domains: "Stellar Lumens (stellar.org) · USD Coin (circle.com)".

**stellar.expert** — the liquidity-pool list has NO search field at all, only
sorting. Its answer to lookalikes is curated labelling in the row itself:
`LDEX [SCAM] GCTA…EH7N`, `AMM [caution: private pooling system]`,
`USDC [Centre] GA5Z…KZVN`, plus home domains (aqua.network, ultracapital.xyz).

**Uniswap** — support article returned 403; nothing claimed.

### What this says about us

On matching we now beat both Stellar-native products: Aquarius has the exact
bug we fixed and a pair separator that does not constrain, stellar.expert has
no filter. Two gaps remain, and both are things every competitor covers:

1. **No identity signal.** `XLM` and `DXLM` look identical in our list. GT
   uses verified ticks + a security score, Aquarius the issuer home domain,
   stellar.expert curated `[SCAM]` labels. **We already have the data** —
   `issuer_home_domain` ships on the assets endpoint (task 0450, live). Adding
   it beside the code in the pool list is frontend-only and turns the exact
   pair rule into something the reader can see. Recommended next step.
2. **No search by asset id / issuer address.** Both GT and Aquarius accept an
   address; a code alone cannot disambiguate issuers by construction.

### Open question: a partially typed pair

`SOL/US` returns NOTHING today, because pair sides match whole codes. Measured
alternatives, all on production:

| Pair-side rule        | `XLM/USDC` | `XLM/US` | Lets in                                                                |
| --------------------- | ---------- | -------- | ---------------------------------------------------------------------- |
| exact (shipped)       | 63         | **0**    | nothing                                                                |
| substring             | 197        | many     | `DXLM`, `yUSDC`, `LibreXLM` (prefix impostors)                         |
| prefix (`startsWith`) | 313        | 313      | `XLMFISHY`, `XLMPAYPAL`, `XLMGOOGLE`, `USDCSTELLAR` (suffix impostors) |

No string rule wins: prefix simply trades one impostor family for another.
The honest options are (a) keep exact and make the empty state say so — "no
pools with exactly SOL and US; type one fragment to search partially" — or
(b) accept substring sides and lean on the identity labels from gap 1. Do NOT
implement "exact, then silently retry as substring": a filter that quietly
answers a different question than the one typed is the failure mode this
codebase keeps re-learning.

## Acceptance criteria

- [x] Substring match on `asset_a_code` / `asset_b_code`, min-length guarded
- [x] `A/B` pair syntax, order-insensitive
- [x] Placeholder text matches actual behaviour
- [x] Query cost measured (`read_rows`) on the busiest codes; no regression vs
      the current exact-match plan
- [x] Regex explicitly not accepted; malformed input rejected, not passed through
- [x] Native legs searchable as `XLM` (found while verifying; pre-existing)
- [x] **Docs updated** — backend-overview (endpoint contract), frontend-overview
      (§6.13 filter description) and the documented list SQL
- [x] **API types regenerated** — `openapi.json` + `generated/*` in the same change
