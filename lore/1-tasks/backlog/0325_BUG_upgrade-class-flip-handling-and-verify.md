---
id: '0325'
title: 'BUG: handle (rare) class flip on WASM upgrade — reclassify + NFT quarantine promote/drop, and verify it is real not a parse artifact'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0320', '0283']
tags:
  [
    soroban,
    classification,
    clickhouse,
    executable_update,
    phase-future,
    effort-small,
    priority-low,
  ]
links: []
history:
  - date: 2026-06-24
    status: backlog
    who: karolkow
    note: >
      Spawned from 0320 research. 0320 fixes stale wasm_hash (update the field);
      it deliberately does NOT handle the rare case where an upgrade CHANGES the
      contract's class. Measured: 2 of 4,691 upgrades changed class, both on one
      contract. Deferred here.
---

# BUG: class flip on WASM upgrade — handle + verify it is real

## Summary

Task **0320** corrects the stale `soroban_contracts.wasm_hash` after a WASM
upgrade ("update the field"). It deliberately does **not** handle the case where
an upgrade _changes the contract's class_ (Other ↔ Fungible ↔ Nft), because the
mainnet measurement showed this is vanishingly rare and **net-zero** across all
1,362 upgraded contracts. This task picks up that deferred edge case.

## Context — exactly where it happened (measured 2026-06-24, prod CH)

Across **4,691** `executable_update` upgrades, **only 2 transitions changed
class**, both on a single contract:

- **Contract:** `CDCN2D4OF5IHPAHUIF6RPVH654KW6LKTYKYK3IQULBBWURD7L4CDNSRO` (37 upgrades)
- **Ledger 59265674 — Other → Fungible.** New wasm `13e408b8…` exposes
  `total_supply, decimals, allowance, balance, approve, transfer, name, symbol,
mint_reward, max_supply, …` (an OpenZeppelin fungible surface).
- **Ledger 59337663 — Fungible → Other.** New wasm `f35c6fb9…` (OLD `2bd0eeb7…`)
  dropped the fungible discriminators.
- **Net deploy → current: Other → Other** (no change). 0 NFT flips ever, 0 net
  flips for any of the 1,362 contracts.

**Open question — real or a parse artifact?** Initial evidence says **real**: the
`13e408b8…` interface genuinely lists `total_supply`/`decimals`/`allowance`, so the
Fungible verdict is correct, not a mis-parse. But this task should confirm — verify
the `wasm_interface_metadata` for these intermediate hashes matches the actual
on-chain WASM exports (e.g. `stellar contract fetch` + decode), to rule out a
contract-interface extraction bug that would fabricate spurious flips.

## Implementation

- Verify the `13e408b8…` / `f35c6fb9…` interfaces against the real on-chain WASM
  (extraction correctness). If wrong → fix the interface extraction (the real bug).
- If real: in the 0320 live RMW path, when `prior_wasm_verdicts[new_hash]` differs
  from the existing `contract_type`, re-run the NFT quarantine promote/drop
  (`reclassify_contracts_from_wasm` companion in `stage.rs`): Nft → promote
  `nfts_pending`/`nft_ownership_pending`; Fungible → drop pending.
- Add the flip case to the 0320 audit-harness invariant (contract_type tracks the
  current wasm's verdict, not just the hash).

## Acceptance Criteria

- [ ] Confirmed whether the CDCN2D4O flip is real or an interface-extraction bug
- [ ] If real: upgrade that flips class re-runs quarantine promote/drop correctly
- [ ] Invariant covers contract_type, not only wasm_hash
