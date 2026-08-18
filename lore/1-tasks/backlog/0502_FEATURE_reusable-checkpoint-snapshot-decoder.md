---
id: '0502'
title: 'FEATURE: reusable checkpoint-snapshot decoder — read pubnet state, not just our stream'
type: FEATURE
status: backlog
related_adr: ['0055']
related_tasks: ['0463', '0503', '0321', '0501', '0492']
tags:
  [
    backend,
    backfill,
    xdr-parser,
    data-integrity,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0463's seed work. The seed builds a one-off decode of the
      history-archive bucket list; this task promotes it to a reusable
      capability once that decode has actually run and its costs are known.
      Deliberately NOT started before the seed — the shape should come from
      working code, not from speculation.
---

# FEATURE: reusable checkpoint-snapshot decoder

## Why this exists

Our index is a **stream of changes** since ledger floor 50,457,424. The
official SDF history archive publishes a **full state snapshot** of pubnet as
raw XDR in the checkpoint bucket list — measured at **4.54 GB gzipped across
21 files** (2026-08-17):

```
https://history.stellar.org/prd/core-live/core_live_001/.well-known/stellar-history.json
```

The difference matters more than it sounds. A stream can only tell you about
things that moved; **78.85 % of chain history predates our floor**, so an
entry that never changed since then has no row in our database at all — it
cannot be sampled, counted, or even detected by any query we can write. The
snapshot is the only source that answers **"what do we NOT have?"**.

Verified: this project has never used it. Every lore mention of "history
archive" is Galexie/captive-core catch-up configuration; no code touches
`currentBuckets` or the bucket files.

## Scope

Extract the one-off decode built for the 0463 seed into a component that can
be pointed at the archive and asked for a typed stream of ledger entries:

- fetch + verify the bucket list from the archive manifest (the archive is a
  hash-chained transport anchored to SCP — a **verified** source under the
  standing rule, unlike an unverifiable API response);
- stream-decode buckets without materialising 4.54 GB in memory;
- yield typed entries — `AccountEntry`, `TrustLineEntry`, `ContractData`,
  `LiquidityPoolEntry` — with each entry's own `lastModifiedLedgerSeq`;
- expose the checkpoint ledger, so callers can reason about staleness rather
  than guess.

**Staleness is part of the contract, not a caveat:** the snapshot is correct
at its checkpoint ledger and stale the moment it lands. Anything written from
it versions on the entry's own ledger so live parser writes win regardless of
load order — never on a window boundary (the task 0492 defect).

## What it unlocks

- **Task 0503** — exhaustive completeness audit against network state.
- **Task 0501** — trustline authorization flags ride the same entries.
- Contract balances, pool state, and any future entity that is a ledger entry.
- A repeatable drift check: "our state versus the network's", runnable on a
  schedule instead of discovered by accident.

## Acceptance criteria

- [ ] A caller can request a typed entry stream for a given entry type without
      knowing anything about bucket layout
- [ ] Memory stays bounded — measured, not assumed, on the full 4.54 GB
- [ ] The checkpoint ledger and per-entry `lastModifiedLedgerSeq` are exposed
- [ ] Wall-clock and peak memory of a full pass are recorded in the task
- [ ] The 0463 seed is refactored onto it rather than keeping a second copy
- [ ] **Docs updated** — `docs/backfills.md` gains the snapshot as a source
- [ ] **API types** — N/A
