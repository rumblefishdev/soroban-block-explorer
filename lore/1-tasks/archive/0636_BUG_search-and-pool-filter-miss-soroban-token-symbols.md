---
id: '0636'
title: 'BUG: search and the pool asset filter never match a Soroban token by symbol or name'
type: BUG
status: completed
related_adr: []
related_tasks: ['0615', '0620', '0470', '0371', '0546']
tags: [priority-high, effort-small, layer-api, search, liquidity-pools]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-08
    status: backlog
    who: karolkow
    note: 'Found in the post-0620 audit of Soroban-token gaps; decided W326 A — its own task, after 0615.'
  - date: 2026-10-08
    status: active
    who: karolkow
    note: 'Started after 0615 merged.'
  - date: 2026-10-08
    status: completed
    who: karolkow
    note: 'Merged #658 ([structure only]) and #659. Local API on production data: search and the pool filter find Soroban tokens by symbol/name (SolvBTC first; XLM/SolvBTC 0 → 3 pools); classic results unchanged; every needle incl. single letters 200; non-ASCII names in both cases. Follow-up 0637 (one ticker rule) in backlog. Ships with the next release.'
---

# Search and the pool asset filter miss Soroban token symbols

## Summary

Search's asset bucket and the pool list `filter[asset_code]` match only
`assets.asset_code`, which is empty for every Soroban token, so a Soroban
token cannot be found or filtered by its symbol or name. The assets list
already matches `symbol` and `name`; apply the same match in both places.

## Context

- `crates/api/src/search/queries.rs:708` (asset bucket) and
  `crates/api/src/common/pool_asset_codes.rs:42-46` (`asset_codes_predicate`,
  used by `filter[asset_code]` and the search pool bucket) read
  `shown_code_sql` only. `crates/api/src/assets/queries.rs:754-756` also
  matches `soroban_contract_metadata.name` / `.symbol`.
- Production, 2026-10-08: `asset_code` is empty for all 4,495 Soroban tokens;
  3,361 of the 4,158 with a symbol do not contain it in their name. Searching
  "SolvBTC" returns 16 classic look-alikes and not the token `CBIJBDNZ…`
  (named "Solv BTC"); 83 Soroban pools have a Soroban-token leg, none
  filterable by it — e.g. XLM/SolvBTC `CD2O2B6P…` ($9.1M).

## Implementation

- Match a Soroban token's `symbol` and `name` (from
  `soroban_contract_metadata`) beside `asset_code`, exactly as the assets list
  does, in both sites.
- Keep classic matching unchanged.

## Implementation Notes

- Pool filter (`common/pool_asset_codes.rs`): a leg matches by its shown code
  as before, or by being one of the needle's Soroban tokens, read once per
  request from `soroban_contract_metadata` (`soroban_token_ids`) and passed as
  the server parameter `{tokens_i:Array(Int64)}`. A needle matching more than
  1,000 tokens (a single letter) reads them inside the query instead.
- Search asset bucket: matches symbol and name as well, ranks an exact/prefix
  symbol like a code, and labels a hit through `asset_identity::leg_label`
  (code, Soroban symbol, contract) — it used to fall back to `XLM` for any hit
  without a code.
- Search pool bucket: the "longer than a 12-character code" gate became "the
  needle is a StrKey", so a Soroban token's longer name (11 of the 48 pooled
  tokens) still finds its pools, and the two surfaces keep answering alike.
- Search's buckets moved to `search/queries/{pools,assets}.rs` first (#658,
  `[structure only]`; `queries.rs` was 885 lines).

## Issues Encountered

Measured on production data (local API vs `develop`, 2026-10-08), three
shapes rejected before the one shipped:

- token ids read inside the lambda's `IN` subquery: +2.5M rows, +0.25 s per
  pool-list call (re-read per block);
- ids inlined into the SQL: `A/E` (~7,000 ids) exceeds `max_query_size` → 500;
- one scalar array with `has()`: linear per leg, 9–33 s and 500s.
- A server parameter travels in the URI, which the HTTP client caps at 64 KiB
  (~2,800 ids) — hence the 1,000-id cutover to the in-query read.

## Acceptance Criteria

- [x] Searching "SolvBTC" returns `CBIJBDNZ…` first; "XAUM" returns
      `CC2RBGYN…`. "XRP" finds `CB7OOP3V…` but ranks it below the top 10:
      ranking within a tier is by holders, and it has 25 against 3,049–27,719
      for the classic XRP assets.
- [x] `filter[asset_code]=XLM/SolvBTC` lists `CD2O2B6P…`, `CDL5H2BZ…`,
      `CCD3P3RN…` (was none); XAUM 4 → 7 pools; search pool bucket for
      SolvBTC 1 → 8.
- [x] Classic search and filter results unchanged (USDC, AQUA, KALE, XLM,
      XLM/USDC, `usdc:G…` identical before/after); every needle incl. `A`,
      `A/E`, `S/E` answers 200.
- [x] CH-gated test pins a Soroban token matched by symbol and by name
      (`soroban_leg_matched_by_symbol`, red without the token match).
- [x] **Docs updated** — `backend-overview.md` (pools list + search),
      `frontend-overview.md` (pool filter).
