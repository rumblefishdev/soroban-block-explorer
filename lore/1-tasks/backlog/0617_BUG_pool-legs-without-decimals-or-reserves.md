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

## Acceptance Criteria

- [ ] Each of the six pools shows both reserves, or the page states why not,
      with the cause fixed at its source.
