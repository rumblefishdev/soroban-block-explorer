---
id: '0615'
title: 'BUG: pool legs that are Soroban tokens are never priced (price key lacks the contract address)'
type: BUG
status: backlog
related_adr: ['0053', '0058']
related_tasks: ['0374']
tags: [priority-medium, effort-small, layer-api, liquidity-pools]
links: []
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Found in the architecture review re-check of 2026-10-04; 0374 already closed, so its own task.'
---

# Pool legs that are Soroban tokens are never priced

## Summary

`price_leg` (`crates/api/src/liquidity_pools/queries/usd_analytics.rs:98`)
maps only native (0) and classic credit (1|2); a Soroban-token leg (family 3)
gets the empty key, so its pool's TVL, volume and fees read null even when the
prices views carry a price for that token.

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
  cross-checked against CoinGecko on 2026-10-05:

  | token               | decimals | views close | CoinGecko | ratio  |
  | ------------------- | -------- | ----------- | --------- | ------ |
  | SolvBTC `CBIJBDNZ…` | 8        | 8,537.05    | 86,055    | 0.099  |
  | XAUM `CC2RBGYN…`    | 9        | 41.92       | 4,152.45  | 0.0101 |
  | XRP `CB7OOP3V…`     | 6        | 14.78       | 1.52      | 9.7    |

  Our own reserves agree with CoinGecko: the XLM side of `CD2O2B6P…` implies
  SolvBTC at $85,980. So the views scale every contract token as if it had 7
  decimals. Owned by the prices service; reported there. Shipping this task
  before that fix would print TVL 10–100× wrong for non-7-decimal tokens.

## Implementation Plan

- Extend the price identity with the contract address; family 3 maps to
  `('contract', contract_address)`.
- Every query that joins the prices views by the identity tuple follows:
  `fetch_last_closes` / `fetch_pool_usd_analytics` (`usd_analytics.rs`), the
  chart series (`get_pool_chart.rs:179, 187, 315, 459`).
- The docs comment of `price_leg` ("LP legs are classic-only") is stale since
  Soroban pools joined the list; fix it.

## Acceptance Criteria

- [ ] The 10 measured pools serve TVL / volume where every leg is priced;
      pools with an unpriced leg still read null (no partial sums).
- [ ] The prices view's contract prices match an external source within a few %
      for every priced Soroban leg before release (the 2026-10-05 table, re-run).
- [ ] Classic pools unchanged (compare list + detail responses before/after).
- [ ] CH-gated test pins a Soroban-token leg priced by contract address.
- [ ] Docs updated — `docs/architecture/**` frontend/API contract for pool USD
      fields, or `N/A — reason`.
