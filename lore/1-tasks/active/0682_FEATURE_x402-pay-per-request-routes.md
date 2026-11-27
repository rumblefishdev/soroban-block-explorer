---
id: '0682'
title: 'Sell three data routes per request through x402 (testnet)'
type: FEATURE
status: active
related_adr: ['0048']
related_tasks: ['0277', '0677']
tags: [effort-medium, priority-medium, layer-backend, layer-infra, x402]
links:
  - https://github.com/rumblefishdev/stellar-x402/blob/develop/lore/1-tasks/active/0041_FEATURE_example-integrations.md
  - https://github.com/rumblefishdev/stellar-x402/tree/develop/packages/sdk/rust
history:
  - date: 2026-11-27
    status: active
    who: adamkoot
    note: >
      Created and started. One of the two example integrations of stellar-x402's task 0041: a
      caller with no key and no browser pays USDC for one request. Stage 1 is the testnet
      deployment, for testnet USDC; mainnet data for pubnet USDC is a later task.
---

# Sell three data routes per request through x402 (testnet)

## Summary

A script or an agent pays USDC for one request and gets the answer:
`transactions/{hash}`, `contracts/{contract_id}/events` and `contracts/{contract_id}/interface`
get paid twins under `/x402/v1/`, settled by the stellar-x402 facilitator and listed in its
catalog. The API issues no keys today and its free tier needs a browser (Turnstile), so this is
its first way in for a caller without one.

## Context

- stellar-x402 task 0041 (design and plan in its `notes/`): the RFP's two example integrations
  are this API and the Prices API. The seller side is the Rust crate `stellar-x402`
  (`packages/sdk/rust` there), pinned by `rev`.
- The three routes are what the explorer has and Horizon or Stellar RPC do not give: decoded
  operations, full event history, a contract's interface.
- The payment's network is the data's network: testnet data for testnet USDC, on the testnet
  deployment. Production sells nothing until the facilitator serves pubnet.
- A paid route must not charge for an answer with nothing in it, and a facilitator outage must
  not touch the rest of the API.

## Implementation Plan

- `crates/api/src/x402.rs`: settings from the environment, the payment gate (asked on the first
  paid request, never at cold start), the three routes over the keyed handlers.
- The routes skip the auth gate and stay behind the edge-secret lock.
- Gateway: `GET /x402/{proxy+}` with a request limit of its own; the Lambda's settings in
  `envs/testnet.json`.

## Acceptance Criteria

- [ ] Without `X402_PAY_TO`, or with settings that name another network than the one indexed,
      the paid paths do not exist
- [ ] An unpaid request to a paid route answers 402 with the terms and the catalog listing,
      with no key and no session, only through Cloudflare; every other route keeps its gate
- [ ] An answer with nothing in it is an error that is not charged: 503 `heavy_unavailable`
      for a transaction without its archive part, 404 `no_data` for an empty events page or a
      null interface
- [ ] Testnet's gateway gives the prefix its own request limit; production maps nothing
