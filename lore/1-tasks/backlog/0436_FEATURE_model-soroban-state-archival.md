---
id: '0436'
title: 'FEATURE: model Soroban state archival — TTL, eviction, restoration are protocol state we do not represent'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0435', '0432', '0429', '0415']
tags:
  [
    priority-medium,
    effort-large,
    layer-indexer,
    layer-api,
    layer-frontend,
    soroban,
    data-model,
  ]
links:
  - https://developers.stellar.org/docs/learn/fundamentals/contract-development/storage/state-archival
history:
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0435, which confirmed the gap rather than assumed it: five
      contracts that provably existed (they emitted 408 transfers, 18 burns, 1
      mint) return **zero entries** from `getLedgerEntries` on Soroban RPC,
      against a positive control that returns one. They existed and are gone
      from current state — archival.
      0435 answers "why are these 54 rows odd". This task answers "what should
      the explorer do about a protocol concept it does not represent at all".
      Kept separate because 0435 is a bug write-up and this touches schema, API
      and UI. Checked 0429 and 0415 first — neither fits: 0429 is about a
      shrinking pre-parser residue (opposite direction), 0415 is about who
      authored a fact, not about facts we never ingest.
---

# Model Soroban state archival

## What the protocol does that we ignore

Soroban charges rent for ledger entries. When a `Persistent` or `Instance`
entry's TTL reaches zero it is **archived** — removed from current state,
inaccessible until restored:

> "When a `Persistent` or `Instance` entry TTL is 0, it is 'archived' and can't
> be accessed until it is 'restored'."

Entries can also be **evicted**, and later **restored** via `RestoreFootprintOp`
or an `InvokeHostFunction`. None of TTL, archival, eviction or restoration
exists anywhere in our schema.

## Why it matters beyond the 54 rows

Confirmed consequence today: contracts with real token traffic sit in our tables
as stubs with no deployer and no wasm, because we never saw a deploy entry — it
had already left current state.

Likely consequences not yet measured:

- **"Contract not found" may be a lie.** Archived ≠ nonexistent. A user looking
  up a contract that expired deserves "archived on ledger N", not a 404.
- **Balances in archived entries are invisible.** A holder whose balance entry
  expired still had it; we show nothing.
- **Completeness audits measure the wrong invariant.** Anything asserting
  "present in events ⇒ present in state" is wrong by construction — that is
  precisely the population 0435 found.
- **TTL is a real user-facing fact.** Explorers show contract expiry; ours
  cannot.

That SDF treats this as core state is visible in Hubble's schema: `ttl`,
`evicted_keys`, `evicted_keys_snapshot`, `restored_key` are first-class tables
(see 0432).

## Open questions — decide before building

1. **Do we ingest it, or query it on demand?** The signal is in ledger-entry
   changes we already receive; `stellar-xdr` exposes eviction/restoration change
   types. Check whether we are **discarding** it rather than never receiving it
   (0435 lists this as an unresolved item). Ingesting is cheap if the data is
   already flowing past us.
2. **What is the minimum useful representation?** Ranges from one nullable
   `archived_at_ledger` on `soroban_contracts` to full TTL tracking per entry.
   Start from the question a user asks — "is this contract alive?" — not from
   the protocol's full model.
3. **Does archival apply to entries we index beyond contracts?** Contract data
   and balances are `Persistent` entries too. Scope this deliberately.
4. **How does restoration interact with `ReplacingMergeTree`?** A restored
   contract reappears in state. The version column must make the restore win —
   the same trap as 0421, where a defaulting write outversioned the truth.

## Implementation sketch (not a decision)

- [ ] Determine whether eviction/restoration change types reach our parser today
      and are dropped. This gates everything else and is a read-only check.
- [ ] Decide the representation (question 2) and record it.
- [ ] Ingest, with the RMT versioning question (4) answered explicitly.
- [ ] Backfill the historically archived population — the 54 contracts are the
      known floor, not the total.
- [ ] Surface it: contract detail page distinguishes active / archived, and the
      "not found" path stops lying.

## Acceptance Criteria

- [ ] A written decision on scope (which entry kinds, what representation).
- [ ] An archived contract is distinguishable from a nonexistent one, in the API
      and in the UI.
- [ ] A restored contract returns to active state without a manual repair pass.
- [ ] The 54 contracts from 0435 are correctly labelled, not stubs.
- [ ] Verified against an external source — `getLedgerEntries` for current
      state, Hubble's `evicted_keys` / `restored_key` for the mechanism and
      history. Not against our own tables.
- [ ] Docs updated — `docs/architecture/**` per ADR 0032; this changes the shape
      of what we model.
- [ ] API types regenerated — required, a contract-state field is API surface.

## Note on sequencing

Question 1 is a half-hour read-only check and could change the size of this task
by an order of magnitude. Do it before estimating anything else.
