---
id: '0596'
title: 'Remove the dead NFT event_order counter from the parser'
type: REFACTOR
status: completed
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
  - date: '2026-09-30'
    status: active
    who: karolkow
    note: >
      Activated (thread 596) after 0538 closed.
  - date: '2026-09-30'
    status: completed
    who: karolkow
    note: >
      #560 merged: field, counter and cap removed; xdr-parser 455 tests
      (3 counter/cap tests out, 2 in). Ships with the next Compute deploy;
      no database change.
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

- [x] No `event_order` left in `xdr-parser` (only historical comments in
      `api` and `init.sql`)
- [x] The cap test replaced by one showing no event is skipped
      (`keeps_every_change_past_the_old_smallint_cap`, 32,772 events)
- [x] **Docs updated** — N/A: `docs/architecture/xdr-parsing/**` never named
      the counter or the cap; `ExtractedNftEvent` docs fixed in the code

## Design Decisions

### Emerged

1. **One PR, not a structure PR + a behaviour PR** (thread 353): the counter
   existed only for the cap, so it could not go without the behaviour change.
2. **The CH-gated `db-clickhouse` tests were not run** (no local ClickHouse);
   their only change is the removed fixture field, proven by compilation.
