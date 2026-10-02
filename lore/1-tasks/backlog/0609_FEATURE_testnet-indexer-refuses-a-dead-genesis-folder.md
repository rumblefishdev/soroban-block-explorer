---
id: '0609'
title: 'Testnet indexer refuses to start an empty database from a folder that stopped growing'
type: FEATURE
status: backlog
related_adr: ['0052']
related_tasks: ['0553']
tags: ['testnet', 'indexer', 'effort-small', 'priority-low']
links: []
history:
  - date: 2026-10-02
    status: backlog
    who: karolkow
    note: 'Spawned from 0553 (decision W137 B): the reset runbook guards the risk for now.'
---

# Testnet indexer refuses to start an empty database from a folder that stopped growing

## Summary

Since 0553 an empty testnet database starts reading the public data lake at
ledger 2. During a testnet reset that is safe only if the indexer is paused
before the database is dropped: otherwise it refills the database from the
OLD genesis folder, and the new chain later lands on top of it — two chains
in one database with nothing to detect it. Today only the reset runbook
(`docs/runbooks/testnet-reset.md`, warning at the top) prevents this.

## Context

The lake keeps one folder per testnet genesis
(`v1.1/stellar/ledgers/testnet/<date>/`); an old folder stops growing after a
reset. 0553's plan named a reset alarm (phase 4); the stall alarm covers the
detection, not the refusal.

## Implementation Plan

- Before starting an empty database at ledger 2, check the configured folder
  is live: its newest ledger closed recently (read the newest file's close
  time, or compare with testnet RPC's latest ledger). Refuse with an error
  naming the folder otherwise.
- First measure what is reliable: object `LastModified` is rewritten weekly
  by SDF (0553 notes), so it cannot be the signal.

## Acceptance Criteria

- [ ] An empty database configured with a folder that stopped growing is
      refused at the first wake, with an error naming the folder.
- [ ] A live folder starts at ledger 2 as before.
