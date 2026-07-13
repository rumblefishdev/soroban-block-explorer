---
id: '0382'
title: 'Automated verify-range vs Horizon/stellar.expert harness (never-silently-miss contract)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-medium, layer-testing, validation]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker §11 (MISSING pattern: automated verify-range). G-role-crossref = the contract.'
---

# Automated verify-range vs Horizon harness

## Summary

Build an automated harness that verifies our indexed activity for a ledger range
against an external source of truth (Horizon / stellar.expert), so coverage gaps
are caught mechanically instead of by manual spot-check.

## Context

Spawned from 0359 §11 (MISSING pattern). The whole 0359 audit rested on
"never silently miss" — but there is no automated contract enforcing it. The
0359 `notes/G-role-crossref.md` is the informal spec of what SHOULD appear.

## Implementation

- For a ledger range, diff our per-asset / per-account / per-op activity against
  Horizon (and stellar.expert where it exposes more, e.g. per-asset native tx).
- Report missing / extra rows per dimension (asset, account, op).
- Wire as a CI-runnable check (or an ops script) so backfill + live drift is
  caught. This is the validation gate for 0379's backfill acceptance too.

## Acceptance Criteria

- [ ] harness diffs indexed activity vs Horizon for a ledger range
- [ ] reports missing/extra per dimension
- [ ] runnable in CI or as an ops validation script
