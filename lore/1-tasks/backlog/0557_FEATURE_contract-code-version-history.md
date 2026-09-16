---
id: '0557'
title: 'FEATURE: show how many times a contract changed its code, and when'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0320', '0325', '0327', '0374']
tags:
  [
    'soroban',
    'contracts',
    'executable_update',
    'api',
    'frontend',
    'effort-small',
    'priority-medium',
  ]
links: []
history:
  - date: 2026-09-16
    status: backlog
    who: karolkow
    note: >
      Filed while comparing other explorers' handling of a pool whose code was
      replaced: stellar.expert shows a code-version count on the contract page,
      we show only whether the code CAN change (0327).
---

# FEATURE: contract code version history

## Summary

The contract page says whether a contract's code can be replaced (0327's
Upgradeable/Immutable badge) but not that it was, how often, or when. A
holder of a token cannot see that the logic governing their balance changed.
The data already exists: every replacement is an ingested `executable_update`
system event carrying the old and the new code hash.

## Context

- Upgrades are common: 486 of 4,450 bespoke token contracts had their code
  replaced at least once, 1,678 replacements in total (production, 2026-09-16).
- The replacement code has full authority over the contract's own storage. On
  pool `CAZ6W4WH…` the replacement moved every token out in the same
  transaction (task 0325).
- stellar.expert's contract page shows "Versions: N" for the same contract.
- Coverage is bounded by our ingest floor: events below it are not held (0327
  measured that event history alone misses most _upgradeable_ contracts —
  capability is not history). The history must say where it starts.

## Implementation Plan

1. **Define "version".** Measured on `CAZ6W4WH…`: 11 upgrade events (deduped
   by `(transaction_id, event_index)`), 7 distinct new hashes; stellar.expert
   says 9. Settle the definition (distinct code hashes the contract ran, in
   order, including the original) and reconcile against raw chain data
   before rendering a number.
2. **API:** on the contract endpoint, the ordered list of code changes — ledger,
   time, old hash, new hash — deduped on read (RMT), plus the ledger the
   history is complete from. No schema change: read `soroban_events`
   (`signature = 'executable_update'`, consensus + system only, the same guard
   as `build_wasm_upgrade_rows`).
3. **Frontend:** a version count next to the Upgradeable badge and an expandable
   list of changes; below the ingest floor, an explicit "history before
   <date> not indexed", never an implied "never changed".

## Acceptance Criteria

- [ ] Version definition written down and reconciled on 3 contracts against
      raw chain data (one of them `CAZ6W4WH…`)
- [ ] Contract endpoint returns the ordered code changes and the completeness
      bound; duplicates never double-count
- [ ] Contract page renders count + list; partial history is labelled as partial
- [ ] **Docs updated** — `docs/architecture/**` API + frontend data contract
- [ ] **API types regenerated** — the contract endpoint changes shape
