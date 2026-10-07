---
id: '0621'
title: 'Classify fungible tokens by exact SEP-41 signatures, not function names'
type: REFACTOR
status: backlog
related_adr: ['0061']
related_tasks: ['0620', '0283', '0320', '0325', '0473']
tags: ['effort-medium', 'priority-medium', 'soroban', 'classification']
links: []
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Spawned from 0617 research (W249); after 0620.'
---

# Classify fungible tokens by exact SEP-41 signatures

## Summary

`classification.rs:101-120` types a program Fungible when any one of
`decimals`, `allowance`, `total_supply` appears by name (`total_supply` is not
even in SEP-41). The interface spec we already store carries full signatures;
classify on those, and later confirm with a local `decimals()` call (0620).

## Context

Measured 2026-10-05 on `wasm_interface_metadata` (all programs):

- Of 330 programs behind token contracts: 220 match all 10 SEP-41 functions
  exactly, 94 more match the core five (`balance`, `transfer`, `decimals`,
  `name`, `symbol`); 16 programs (44 contracts) fail the core — 8 without
  `transfer`, 3 NFT-shaped (`balance → u32`), 2 returning name/symbol as
  `Symbol`/`Bytes`.
- 113 programs (281 contracts) typed Fungible today match no core signature.
- No program typed Other or NFT today matches the core: zero new tokens.

`Result<T, E>` counts as `T`; `transfer` to `Address` or `MuxedAddress`.

## Implementation

- Signature rule in `classification.rs`, one place; rebuild
  `contract_type` with `contract-type-rebuild`.
- Decide what a former "token" without `transfer` becomes, and what happens
  to its `assets` row.
- WASM upgrade re-classification stays with 0325.

## Acceptance Criteria

- [ ] Type decided by exact core SEP-41 signatures; the 44 + 281 contracts
      reviewed and their rows moved deliberately.
- [ ] A program is a token when its signatures match AND a local
      `decimals()` call (0620) succeeds on the contract; the name-only rule
      is removed.
- [ ] **Docs updated** — `xdr-parsing-overview.md` or `N/A — reason`.
