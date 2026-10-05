---
id: '0619'
title: 'A Soroban pool whose token is an account, not a contract'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0617', '0571']
tags: ['effort-small', 'priority-low', 'liquidity-pools']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Spawned from 0617: pool CCH6A2JC… has an account as its token_a.'
---

# A Soroban pool whose token is an account, not a contract

## Summary

Pool `CCH6A2JCUFDNPPV2XHG5SPIOGQREV7QJZ62CIZJXQQR7E3NI3YZXBJIF` names an
account as one of its tokens. The indexer stores the leg as if it were a
contract, so the pool page shows a leg with no identity.

## Context

On chain, the pool's `CONFIG` has `token_a` = the USDC issuer account
(`GA5ZSEJY…`) and `token_b` = the native XLM SAC (`CAS3J7GY…`). Reserves are
0/0 and unchanged since creation (ledger 50,875,676); an account cannot act
as a token, so the pool can never trade.

How the leg is produced: `parse_pool_config` accepts any address
(`xdr-parser/src/pool_config_factory.rs:160-163`, via `scval::address`), and
`contract_token_asset_id` (`db-clickhouse/src/persist/stage/soroban_pools.rs:102-107`)
hashes the text as a contract. The leg id equals the account's surrogate, so
no `assets` row can match. It is 1 of 1,571 legs across 779 Soroban pools;
no other pool shares the cause. Not the 0571 identity gap (that one is a real
asset without its row).

## Implementation

- `contract_token_asset_id`: check the token with `ids::contract_payload`
  and refuse a non-contract token loudly, the way a bad pool address is
  refused. Shared by all three pool families.
- Decide at implementation: drop such a pool from the registry, or keep it
  with an explicit "token is an account" marker.
- Removing the existing row is a production write — hand the command over.

## Acceptance Criteria

- [ ] A pool whose token is not a contract never gets a leg that resolves to
      no asset; the indexer logs the refusal.
- [ ] `CCH6A2JC…` no longer shows an unidentified leg on production.
- [ ] **Docs updated** — `docs/architecture/**` or `N/A — reason`.
