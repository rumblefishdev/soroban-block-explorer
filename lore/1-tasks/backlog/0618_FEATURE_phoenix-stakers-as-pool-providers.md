---
id: '0618'
title: 'Show Phoenix stakers as the providers of their pool, not the staking contract'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0374', '0571', '0613']
tags: ['effort-medium', 'priority-low', 'liquidity-pools']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Spawned from the 0613 discussion: 95 % of Phoenix share tokens sit in staking contracts, so the participants list names the contract, not the people behind it.'
---

# Show Phoenix stakers as the providers of their pool

## Summary

A Phoenix provider usually stakes the pool's share token right after the
deposit; the token moves to the pool's staking contract, which keeps who
staked how much in its own storage. The participants list reads share-token
holders from `balances`, so it shows the staking contract with 25–50 % of a
pool and hides the stakers. Same shape as 0613 — positions held inside a
contract — answered the same way: a read-time aggregation of data already
indexed.

## Context

Production, 2026-10-05: 728 of 779 Soroban pools carry a share token with
known decimals; the other 51 are the concentrated pools of 0613. Share of
each deployment's share-token supply held by contracts: ~0 % for Aquarius,
Soroswap and the rest, **95.1 % for Phoenix**. The holders are staking
contracts (their interface: `bond`, `stake_amount`, `lp_token`): 19 of them.

## Measured (2026-10-05)

Phoenix emits each field of a stake as its own event: topics
`("bond"|"unbond", "user"|"amount"|"token")`, the value in data, all in one
operation. Reconstructed per contract and compared with the share-token
balance the contract holds:

| Contract version                     | Contracts | Events alone  | Events + share-token transfers |
| ------------------------------------ | --------- | ------------- | ------------------------------ |
| current, small                       | 11        | exact         | exact                          |
| pre-migration (last move 53,583,501) | 4         | 6–9× too high | exact, no staker below zero    |
| post-migration                       | 4         | start missing | start not split per user       |

- **Pre-migration contracts** emitted `unbond` with the user but without the
  amount (295 users vs 15 amounts in `CAIR3UPW…`). The amount is the
  share-token transfer of the same operation (`asset_transfers`, joined on
  `(ledger_sequence, application_order, op_index)`).
- **Migration, ledger ~53,587,500:** each of the four new staking contracts
  received its whole opening balance as one `mint`, equal to the old
  contract's balance (e.g. 1,129,293,845,852 into `CBRGNWGA…`). The split per
  user is in the events `"Stake: Migration: …"` (356 per kind, 4 contracts),
  not decoded yet; without them 43 / 21 / 33 users of three contracts sum
  below zero.
- The old contracts have not moved since ledger 53,583,501 and still hold
  shares of the old pools.
- One Phoenix pool contract (`CD5XNKK3…`) holds 9.5 % of another pool's
  shares; unexplained.

## Implementation Plan

- Decode the migration events; confirm every post-migration contract then
  sums to its balance with no user below zero.
- Read path: per staking contract, stakers = bond/unbond users with amounts
  from the same operation's share-token transfer, plus the migration split.
  The participants list of a Phoenix pool replaces its staking contract's
  row with those stakers (or lists them under it — settled with 0613's
  prototype).
- Spot-check stakers against the staking contract's storage (raw XDR is the
  arbiter).

## Acceptance Criteria

- [ ] Every Phoenix staking contract's stakers sum to its share-token
      balance, none below zero (all 19 contracts).
- [ ] A Phoenix pool's participants show the stakers; spot-checked on chain.
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md`
      (participants); database schema N/A — no schema change.
