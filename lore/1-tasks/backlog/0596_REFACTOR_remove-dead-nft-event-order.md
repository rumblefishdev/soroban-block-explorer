---
id: '0596'
title: 'Remove the dead NFT event_order counter from the parser'
type: REFACTOR
status: backlog
related_adr: ['0059']
related_tasks: ['0424']
tags: ['xdr-parser', 'nft', 'effort-small', 'priority-low']
links:
  - crates/xdr-parser/src/state/nfts.rs
  - crates/xdr-parser/src/types.rs
history:
  - date: '2026-09-29'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0424 future work.
---

# Remove the dead NFT event_order counter from the parser

## Summary

`extract_nft_ownership_events` still computes `ExtractedNftEvent.event_order`,
a per-`(contract, token, ledger)` counter, and skips every event past 32,767
for one triple because the retired `nft_ownership` stored it as SMALLINT.
Since task 0424 nothing reads it: ownership changes carry the chain position.

## Context

Found closing 0424. The skip is now a behaviour with no reason: a contract
emitting more than 32,767 changes for one token in one ledger loses the rest.

## Implementation

- Drop the `event_order` field and the counter; drop the skip and its warn.
- Update the parser tests that assert the counter or the cap.

## Acceptance Criteria

- [ ] No `event_order` left in `xdr-parser`
- [ ] The cap test replaced by one showing no event is skipped
- [ ] **Docs updated** — `docs/architecture/xdr-parsing/**` (or N/A with reason)
