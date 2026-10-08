---
id: '0615'
title: 'BUG: pool legs that are Soroban tokens are never priced (price key lacks the contract address)'
type: BUG
status: completed
related_adr: ['0053', '0058']
related_tasks: ['0374']
tags: [priority-medium, effort-small, layer-api, liquidity-pools]
links: []
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Found in the architecture review re-check of 2026-10-04; 0374 already closed, so its own task.'
  - date: 2026-10-08
    status: active
    who: karolkow
    note: 'Started: the prices views are rescaled and 0620 gave the legs decimals, so only the price key is missing.'
  - date: 2026-10-08
    status: completed
    who: karolkow
    note: 'Merged #656. Local API on production data: 18 of 792 Soroban pools null → TVL, classic pools unchanged (2,990/2,992 identical, 2 by cents). Ships with the next release.'
---

# Pool legs that are Soroban tokens are never priced

## Summary

`price_leg` (`crates/api/src/liquidity_pools/queries/usd_analytics.rs:98`)
maps only native (0) and classic credit (1|2); a Soroban-token leg (family 3)
gets the empty key, so its pool's TVL, volume and fees read null even when the
prices views carry a price for that token.

## Carried from 0620 (2026-10-08)

The four Aquarius pools of 0617 (`CCYMZTOJ…` USST/USDC, `CCKQASCN…` XLM/XRP,
`CCCDPF74…` and `CBMOEJUO…` with HITZ) now serve decimals and reserves for
both legs (0620, production since 2026-10-08), yet `tvl` is null: the
Soroban leg gets the empty price key. Prices exist for XRP (`CB7OOP3V…`,
fresh) and HITZ (`CBAPZAZN…`, last bucket 2026-10-02); none for USST. Done
when these pools show TVL wherever both legs price.

**The views are rescaled (measured 2026-10-08)**, so the blocker below is
gone: contract closes now match market scale — SolvBTC `CBIJBDNZ…` 82,299,
XAUM `CC2RBGYN…` 4,145, XRP `CB7OOP3V…` 1.33 (was 8,537 / 41.92 / 14.78 on
2026-10-05). Six contract tokens price in the last day. `CCKQASCN…` (XLM/XRP)
would read about $2,227 (6,188.97 XLM × 0.1984 + 752.12 XRP × 1.3282); the
API serves null.

## Start after (checked 2026-10-05)

`feat/0374-volume-priceable` edits `usd_analytics.rs` and its tests; start
once it has merged into `develop` (or is dropped).

## Context

The prices views key a Soroban token by `contract_address`, with
`asset_code` and `issuer_address` empty (`prices.current_price_usd`,
`asset_kind = 'contract'`). Our join key is `(asset_kind, asset_code,
issuer_address)`, so simply mapping family 3 to `'contract'` would collapse
every Soroban token onto one key `('contract','','')`. The key must carry
`contract_address`.

Measured on production 2026-10-04: 40 pool legs are Soroban tokens; 3 of them
are priced in the views; 10 pools carry such a leg. The views price 6 Soroban
tokens in total.

## Measured (2026-10-05): impact, and the prices are mis-scaled

- **Impact.** 261 of 779 Soroban pools read TVL null and 124 volume null
  (production API, every pool). Among Aquarius pools, those with a null TVL
  hold $27.8M of the protocol's $57.9M TVL by its own per-pool figures
  (48 %); the largest are Soroban-token pools, e.g. `CD2O2B6P…` XLM/SolvBTC
  ($9.1M).
- **The views' contract prices are off by 10^(7 − decimals).** Decimals read
  from each contract's own `decimals()` (read-only simulation), prices
  cross-checked against an external market-price source on 2026-10-05:

  | token               | decimals | views close | external | ratio  |
  | ------------------- | -------- | ----------- | -------- | ------ |
  | SolvBTC `CBIJBDNZ…` | 8        | 8,537.05    | 86,055   | 0.099  |
  | XAUM `CC2RBGYN…`    | 9        | 41.92       | 4,152.45 | 0.0101 |
  | XRP `CB7OOP3V…`     | 6        | 14.78       | 1.52     | 9.7    |

  Our own reserves agree with the external figure: the XLM side of
  `CD2O2B6P…` implies SolvBTC at $85,980. The views scale every contract token
  as if it had 7 decimals; they keep no decimals for contract tokens
  (`prices.assets` has no such column) and read none from our tables. Shipping
  this task before the views are rescaled would print TVL 10–100× wrong for
  non-7-decimal tokens.

## Implementation Plan

- Extend the price identity with the contract address; family 3 maps to
  `('contract', contract_address)`.
- Every query that joins the prices views by the identity tuple follows:
  `fetch_last_closes` / `fetch_pool_usd_analytics` (`usd_analytics.rs`), the
  chart series (`get_pool_chart.rs:179, 187, 315, 459`).
- The docs comment of `price_leg` ("LP legs are classic-only") is stale since
  Soroban pools joined the list; fix it.

## Acceptance Criteria

- [x] The 10 measured pools serve TVL / volume where every leg is priced;
      pools with an unpriced leg still read null (no partial sums). Local API
      on production data, 2026-10-08: 18 of 792 Soroban pools go null → TVL,
      0 change value, 774 identical (e.g. `CD2O2B6P…` XLM/SolvBTC $8,367,651,
      `CAXYSVTP…` USDC/XAUM $264,880, `CCKQASCN…` XLM/XRP $2,222).
- [x] Contract prices checked before release: pool-implied prices in the two
      deep balanced pools agree with the views within 0.1 % (SolvBTC $82,230
      vs $82,299; XAUM $4,140 vs $4,145). XRP's only pool is thin ($63 volume
      a day) and implies $1.64 against the views' $1.33.
- [x] Classic pools unchanged: 2,990 of 2,992 identical; the 2 differ by
      cents (reserves moved between the two reads). Classic price rows carry
      an empty `contract_address` (0 of 30,782 in two days).
- [x] CH-gated test pins a Soroban-token leg priced by contract address
      (`soroban_token_close_by_contract`; red when the match ignores it).
- [x] Docs updated — N/A: no `docs/architecture/**` page describes the price
      key; the key is documented at `usd_analytics.rs` and updated there.
