---
id: '0600'
title: 'REFACTOR: an address that can be an account or a contract travels in one field, typed by its StrKey prefix'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0376', '0487']
tags: [api, frontend, effort-small, priority-medium]
links: []
history:
  - date: '2026-09-30'
    status: active
    who: karolkow
    note: >
      Created and activated (thread 339 A + owner). Found reviewing #548:
      the API carried two conventions for the same kind of value.
---

# One address field for an account or a contract

## Summary

A Stellar StrKey says what it is by its first letter (`G` account, `C`
contract, `L` pool). The API should carry such an address in one field and
the SPA should type it in one place. Today one pair of fields splits it and
six SPA sites each re-derive the type, with different answers for `M…`/`B…`.

## Stan teraz

- Done: decision 339 A (+ owner), karolkow, 2026-09-30
- Next: PR 1 — one `addressType` in `libs/ui`; PR 2 — API field renames
- In force: API contract changes; types regenerated; no storage change

## Context (measured 2026-09-30)

- Two fields: `caller_account` + `caller_contract` — contract invocations
  (`contracts/dto.rs`) and the transaction's invocation list
  (`transactions/dto.rs`), added by 0487.
- One field: NFT `owner_account` / `from_account` / `to_account` (since #548,
  thread 338 A), pool participants `account`, the balances issuer line, and
  StrKeys inside operation JSON, the execution trace and humanized sentences.
- SPA prefix checks: `AccountBalances`, `PoolParticipants`, `OwnerIdentifier`
  (G/C only), `HighlightedJson`, `HumanizedSentence` (C/L, else account),
  `ExecutionTrace` (C/L/G, else no link). `libs/ui` already has
  `isAccountId` / `isContractId`.

## Plan

1. **PR 1 (SPA):** `addressType(value)` in `libs/ui/src/identifiers`:
   `C` contract, `L` pool, `G` account, anything else `null` (no link). The
   six sites use it. Behaviour: `M…` / `B…` stop getting a dead account link
   in the five sites that defaulted to account.
2. **PR 2 (API contract):** `caller_account` + `caller_contract` → `caller`;
   NFT `owner_account` → `owner`, `from_account` → `from`, `to_account` →
   `to`. Types regenerated, SPA follows, docs updated.

## Acceptance Criteria

- [ ] One `addressType` in `libs/ui`; no prefix check left in `web/src` pages
- [ ] Invocation DTOs carry `caller`; NFT DTOs carry `owner` / `from` / `to`
- [ ] Contract and transaction pages and NFT pages render and link as before
      (checked on production data)
- [ ] **API types regenerated**
- [ ] **Docs updated** — `docs/architecture/**` API and frontend contracts
