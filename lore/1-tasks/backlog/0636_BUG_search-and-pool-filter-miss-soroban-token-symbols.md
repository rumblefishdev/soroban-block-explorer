---
id: '0636'
title: 'BUG: search and the pool asset filter never match a Soroban token by symbol or name'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0615', '0620', '0470', '0371', '0546']
tags: [priority-high, effort-small, layer-api, search, liquidity-pools]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-08
    status: backlog
    who: karolkow
    note: 'Found in the post-0620 audit of Soroban-token gaps; decided W326 A — its own task, after 0615.'
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

## Acceptance Criteria

- [ ] Searching "SolvBTC" / "XRP" returns the Soroban tokens `CBIJBDNZ…` /
      `CB7OOP3V…`.
- [ ] `filter[asset_code]=SolvBTC` lists `CD2O2B6P…`.
- [ ] Classic search and filter results unchanged (before/after compare).
- [ ] CH-gated test pins a Soroban token matched by symbol.
- [ ] **Docs updated** — `docs/architecture/**` search/pool filter contract,
      or `N/A — reason`.
