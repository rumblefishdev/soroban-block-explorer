---
id: '0600'
title: 'REFACTOR: an address that can be an account or a contract travels in one field, typed by its StrKey prefix'
type: REFACTOR
status: completed
related_adr: []
related_tasks: ['0376', '0487', '0601']
tags: [api, frontend, effort-small, priority-medium]
links: []
history:
  - date: '2026-09-30'
    status: active
    who: karolkow
    note: >
      Created and activated (thread 339 A + owner). Found reviewing #548:
      the API carried two conventions for the same kind of value.
  - date: '2026-09-30'
    status: completed
    who: karolkow
    note: >
      #563, #564, #565 merged; Compute + Web deployed 2026-09-30 13:27 UTC and
      verified. Pool participants `account` → `holder` spawned as 0601
      (thread 369 B).
---

# One address field for an account or a contract

## Summary

A Stellar StrKey says what it is by its first letter (`G` account, `C`
contract, `L` pool). The API should carry such an address in one field and
the SPA should type it in one place. Today one pair of fields splits it and
six SPA sites each re-derive the type, with different answers for `M…`/`B…`.

## Stan teraz

- Done: #563 (`addressType`), #564 (API `caller` / `owner` / `from` /
  `to`), #565 (internal names, thread 368 A); deployed and verified
- Later: pool participants `account` → `holder` — task 0601 (thread 369 B)
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

- [x] One `addressType` in `libs/ui`; no prefix check left in `web/src` pages
      (#563)
- [x] Invocation DTOs carry `caller`; NFT DTOs carry `owner` / `from` / `to`
      (#564)
- [x] Contract and NFT pages render and link as before (checked on the
      deployed API, below); the transaction page's DB fallback for
      invocations was not exercised live (it reads the archive first)
- [x] **API types regenerated** (#564)
- [x] **Docs updated** — backend, frontend and technical overviews (#563, #564)

## Verified after the deploy (2026-09-30, read-only)

- Collection `CBHUX3RS…`, every page (10,056 tokens in 101): `owner` 2,493
  `C…` / 7,563 `G…` / 0 null — equal to `nfts FINAL` in ClickHouse
  (2,493 / 7,563 / 0). Token 64018: `owner` and transfer `to` `CDDHM5YY…`.
- Invocations: `CB23WRDQ…` 500 rows, every `caller` `C…` (full history:
  161,243,645 contract-called rows, 60,510 account-called); `CBZL2IH7…` 100
  rows, every `caller` `G…`.
- No response carried `caller_account`, `caller_contract`, `owner_account`,
  `from_account` or `to_account`; the deployed SPA bundle (49 chunks) names
  none of them.
- `contract_activity` rows with neither caller (~2 bn over history) are
  touched-only rows by design; the Invocations list filters
  `invocation_count > 0`.

## Design Decisions

### Emerged

1. **Transfers SQL keeps `from_account` / `to_account` aliases**: `from` is a
   SQL keyword; only the wire names changed.
2. **`caller` = account, else contract**: resolved in Rust from the two
   stored ids, exactly one of which is set; storage unchanged.
3. **Participants `account` left for 0601** (thread 369 B): renaming it too
   would have needed the same joint deploy; kept out of #564's scope.

## Future Work

- 0601 — pool participants `account` → `holder`.
