---
id: '0435'
title: 'BUG/RESEARCH: we do not model Soroban state archival — 54 contracts with live token traffic have no deployer, no wasm, and contradict asset_sac'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0421', '0256', '0432', '0316']
tags:
  [
    priority-medium,
    effort-medium,
    layer-indexer,
    layer-xdr-parsing,
    data-integrity,
    soroban,
  ]
links:
  - https://developers.stellar.org/docs/learn/fundamentals/contract-development/storage/state-archival
history:
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      Surfaced while validating deployer attribution (0256). 54 contracts carry
      no `deployer_id` and no `wasm_hash` yet emit live token events. My first
      explanation — "contract-authorized deployment, so NULL is correct" — was
      taken from a comment in our own code and does not survive scrutiny: it
      explains the missing deployer but not the missing wasm, not the
      `is_sac` contradiction, and not why stellar.expert cannot find some of
      them on the ledger at all.
      The likelier explanation, and the reason this is filed: **Soroban archives
      contract state when its TTL expires, and we model none of it.**
---

# We are blind to Soroban state archival

## The observation

`soroban_contracts` holds **54 rows** (of 131,314 = 0.04%) where:

- `deployer_id` is NULL **and** `wasm_hash` is NULL
- `wasm_uploaded_at_ledger = 0` — the stub-row sentinel
- `is_sac = false` on **all 54** — yet **4 of them appear in `asset_sac`**, so
  that flag is a default masquerading as a fact (the 0421 whole-row-default
  class)
- they are referenced by **427 contract events, 104 invocations, 2 operations**
- their events are real token traffic: **408 `transfer`, 18 `burn`, 1 `mint`**
- earliest event is ledger 50,468,736 (2024-02, ~11k ledgers after Soroban
  go-live), latest activity reaches ledger 63,355,224

Ingest completeness is **not** the cause: `ledgers` holds 13,139,550 distinct
sequences over a range of exactly 13,139,550 — **zero gaps**.

External lookup is inconsistent, which is itself the clue:

- some return `"Contract was not found on the ledger"` from stellar.expert
- others return a record with **no `creator`, no `wasm`, no `created`** and
  `events: 0`, despite our having hundreds of events for them

## The hypothesis worth testing first

Per the official docs, Soroban **archives** `Persistent` and `Instance` ledger
entries when their TTL reaches zero:

> "When a `Persistent` or `Instance` entry TTL is 0, it is 'archived' and can't
> be accessed until it is 'restored'."

Archived entries leave current ledger state but remain restorable. That would
explain every observation at once: the historical events persist in the ledger
archive (we ingest those), while the contract instance is absent from current
state (so lookups fail and we never see a deploy entry to attribute).

**We model none of this.** There is no notion of TTL, archival, eviction or
restoration anywhere in the schema.

## Why this is worth more than 54 rows

Hubble — SDF's public dataset — ships `ttl`, `evicted_keys`,
`evicted_keys_snapshot` and `restored_key` as first-class tables (see 0432).
The protocol's own maintainers treat archival as core state. If contracts (and
their data) can silently leave current state, then:

- "contract not found" pages may be wrong — the contract may be archived, not
  nonexistent
- token balances held in archived entries are invisible
- any completeness audit that assumes present-in-events ⇒ present-in-state is
  measuring the wrong invariant

## Investigation

- [ ] Confirm or kill the archival hypothesis: take 5 of the 54, query Hubble's
      `evicted_keys` / `ttl` for their ledger keys. This is the cheapest
      decisive test and needs no code.
- [ ] If confirmed: decide what we surface. Options range from a `state:
    archived` flag on the contract page to full TTL tracking. Note the
      `LedgerEntry::to_key()` helper in `stellar-xdr` (0431) is what maps an
      entry to the key Hubble indexes by.
- [ ] Independently: fix `is_sac` on stub rows. A `Bool` column cannot express
      "unknown", so a stub asserting `false` contradicts `asset_sac` for 4
      contracts today. Same defect class as 0421 — belongs with that fix, not
      with archival.
- [ ] Check whether `ledger_entry_changes` already carries eviction/restoration
      change types we discard. `stellar-xdr` exposes them; we may be dropping
      the signal rather than never receiving it.

## Acceptance Criteria

- [ ] A verdict on the archival hypothesis with evidence from Hubble or raw XDR
      — not from our own code's comments.
- [ ] The 54 contracts are explained: archived, contract-authored, or a parser
      defect. One answer, evidenced.
- [ ] `is_sac` no longer asserts `false` on stub rows that `asset_sac`
      contradicts.
- [ ] A written decision on whether archival becomes a modelled concept or an
      explicitly accepted blind spot.
- [ ] Docs updated — `docs/architecture/**` if archival becomes modelled.
- [ ] API types regenerated — required only if a contract-state field is added.

## Method note

The first explanation I reached for came from a comment in our own parser and
was wrong. Use the protocol docs, Hubble, or raw XDR — see
[[feedback-verify-external-not-our-code]].
