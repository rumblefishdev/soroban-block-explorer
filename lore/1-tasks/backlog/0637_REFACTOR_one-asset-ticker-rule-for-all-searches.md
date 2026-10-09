---
id: '0637'
title: 'REFACTOR: one "what an asset is called / what finds it" rule for the assets list, search and the pool filter'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0636', '0470', '0485', '0440']
tags: [priority-low, effort-small, layer-api, search]
links: []
history:
  - date: 2026-10-08
    status: backlog
    who: karolkow
    note: 'Decided W337 B after 0636: one read-time concept, data stays split; not scheduled now.'
---

# One ticker / match rule for every asset search

## Summary

Three surfaces each carry their own copy of "an asset matches a needle when
its shown code, its Soroban symbol or its Soroban name contains it": the
assets list (`assets/queries.rs`), the search asset bucket
(`search/queries/assets.rs`) and the pool filter
(`common/pool_asset_codes.rs`). Fold them into one read-time concept — the
asset's **ticker** (classic code, `XLM` for native, a Soroban token's symbol)
and **what finds it** (ticker or name) — used by all three. Data stays split.

## Context

- Code, symbol and name are different facts: a classic code is part of the
  asset's protocol identity (code + issuer); a Soroban symbol is whatever the
  contract's `symbol()` returns, not unique (7 contracts call themselves
  `USDC`, 2,276 `SMOL`); a name is free text. Writing the symbol into
  `assets.asset_code` would mix identity with a self-declaration, and the
  symbol can change without the asset row changing — so the split storage
  stays (rejected option C).
- Rust already has the display ladder (`asset_identity::leg_label`: code,
  symbol, truncated contract). SQL has only `shown_code_sql` (code / `XLM`).
- Today the copies are pinned by drift tests
  (`native_is_matched_by_type_not_by_stored_code` in each module); a change to
  the rule still has to be made three times.

## Implementation

- One SQL-side definition of ticker and match, beside `shown_code_sql`, read
  by the three surfaces; the pool filter keeps its token-id parameter
  mechanism (0636) for performance.
- Behaviour unchanged: before/after compare of the three surfaces on
  production data.

## Acceptance Criteria

- [ ] One definition of the match rule; the three surfaces call it.
- [ ] Results of all three surfaces identical before/after for a fixed set of
      needles (codes, symbols, names, pairs, single letters, native, StrKey).
- [ ] **Docs updated** — `backend-overview.md` search/filter sections, or
      `N/A — reason`.
