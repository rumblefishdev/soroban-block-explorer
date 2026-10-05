---
id: '0617'
title: 'Soroban pools with a leg that has no decimals or no reserve'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0374', '0571']
tags: ['effort-small', 'priority-medium', 'liquidity-pools']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Spawned while closing #405: 6 pools show a leg without a reserve.'
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: >
      Investigated the three causes. Decimals: not a parser skip — the tokens
      keep metadata under their own storage keys; census of all 339 tokens
      without decimals below. Phoenix pool deferred to 0325; the account leg
      split to 0619.
---

# Soroban pools with a leg that has no decimals or no reserve

## Summary

Six Soroban pools show a leg with no reserve, so no TVL. Three causes;
find each one's origin and fix it there.

## Context

Production API, every Soroban pool, 2026-10-05:

- **Token decimals unknown, 4 Aquarius pools:** `CCKQASCN…` (XLM/XRP
  `CB7OOP3V…`), `CCCDPF74…` and `CBMOEJUO…` (`CBAPZAZN…`), `CCYMZTOJ…`
  (`CBZ4DCE7…`/USDC, $718k on the USDC side). The contracts answer
  `decimals()` on chain (XRP: 6), so the identity resolver misses a value
  the chain has — likely the non-standard `METADATA` instance shape the
  indexer logs as skipped.
- **Phoenix `CAZ6W4WH…` (PHO/USDC):** both legs classic with 7 decimals,
  both reserves null — no pool state read.
- **`CCH6A2JC…` (unnamed deployment):** one leg with no identity at all, the
  other reserve 0 — the write-time identity gap of 0571, or a different one.

## Findings — 2026-10-05

**Token decimals.** The earlier lead (a non-standard `METADATA` the parser
skips) was wrong: none of the three tokens has a `METADATA` key. XRP
(`CB7OOP3V…`) and USST (`CBZ4DCE7…`) keep `Vec[Meta] => {decimals, name,
symbol}`; HITZ (`CBAPZAZN…`) keeps separate `Vec[Decimals]`, `Vec[Name]`,
`Vec[Symbol]`. SEP-41 fixes the `decimals()` function, not where the value
lives, and the parser reads only `METADATA` (`xdr-parser/src/token_metadata.rs`).

Census, every non-SAC contract whose WASM exports `decimals` and `balance`
(4,207), against `soroban_contract_metadata`: 339 have no decimals. Each one's
instance read from RPC, and `decimals()` simulated on each:

| Group                   | Contracts | Notes                                                                                                                 |
| ----------------------- | --------- | --------------------------------------------------------------------------------------------------------------------- |
| Answers `decimals()`    | 287       | 0, 2, 6, 7, 8, 9, 13, 18; 15 of them with empty instance storage (value in code)                                      |
| — storage key guessable | 120       | 18 key layouts in total; guess equals `decimals()` 120/120                                                            |
| — no guessable key      | 167       | value in code or computed                                                                                             |
| No answer               | 52        | 47 empty storage (never initialised); 5 vault/CDP contracts trapping; 25 never invoked, 22 invoked once, max 34 calls |

Every pool leg without decimals is among the 287. No contract used as a token
lacks a value the chain has; the 52 are not tokens in use.

**Phoenix `CAZ6W4WH…`.** The contract was upgraded to a staking contract
(`CONFIG` now holds `lp_token`, `min_bond`…; both token balances 0). The API
withholds its reserves on purpose (`soroban_reserves::NOT_A_POOL`); the stale
state row is from ledger 54,514,504. The other 19 Phoenix pools serve
reserves. Left to 0325, which stores the "no longer a pool" verdict.

**`CCH6A2JC…`.** `token_a` is an account (the USDC issuer), not a contract;
reserves 0/0 are true on chain. Split to 0619.

## Acceptance Criteria

- [ ] Each of the six pools shows both reserves, or the page states why not,
      with the cause fixed at its source.
