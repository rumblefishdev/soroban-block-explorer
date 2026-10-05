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
- [ ] Classic pools unchanged (compare list + detail responses before/after).
- [ ] CH-gated test pins a Soroban-token leg priced by contract address.
- [ ] Docs updated — `docs/architecture/**` frontend/API contract for pool USD
      fields, or `N/A — reason`.
