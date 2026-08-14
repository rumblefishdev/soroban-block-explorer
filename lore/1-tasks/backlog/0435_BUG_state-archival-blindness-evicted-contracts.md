---
id: '0435'
title: 'BUG/RESEARCH: we do not model Soroban state archival — 54 contracts with live token traffic have no deployer, no wasm, and contradict asset_sac'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0421', '0256', '0432', '0316', '0436']
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

## CONFIRMED 2026-07-22 — tested against the chain, not against Hubble

Hubble needs a Google Cloud account, so I used a cheaper and more direct
oracle: **`getLedgerEntries` on Soroban RPC asks the chain for CURRENT state**,
which is exactly the question. Ledger keys built with the official
`stellar xdr encode --type LedgerKey`, not by hand.

| subject                                                         | result                   |
| --------------------------------------------------------------- | ------------------------ |
| positive control — the active factory `CCG5EWFY…`               | **1 entry — found**      |
| `CDNBVUNN…`, `CDQLDS2M…`, `CC3OMWLR…`, `CCAYZINB…`, `CANL52BI…` | **0 entries — all five** |

The control proves the method works; five of five subjects are absent.

**The inference is airtight because these contracts emitted events.** A contract
event can only be emitted by a contract that exists. Ours produced 408
`transfer`, 18 `burn`, 1 `mint`. So they existed, and they are not in current
state now. Something removed them — which is what archival does.

Note this is independent of RPC's 7-day history retention: `getLedgerEntries`
reads present state, not history.

## Remaining investigation

- [x] ~~Confirm or kill the archival hypothesis~~ — **confirmed above.** What is
      still open is the _mechanism_ (TTL expiry vs explicit eviction) and
      whether any of them were later restored. Hubble's `evicted_keys` /
      `restored_key` would answer that; RPC cannot, because it only shows now.
- [x] ~~If confirmed: decide what we surface~~ — **moved to 0436**, which
      owns modelling archival (schema, API, UI). This task stays scoped to
      explaining the 54 rows and fixing `is_sac`.
- [ ] Independently: fix `is_sac` on stub rows. A `Bool` column cannot express
      "unknown", so a stub asserting `false` contradicts `asset_sac` for 4
      contracts today. Same defect class as 0421 — belongs with that fix, not
      with archival.
- [x] ~~Check whether `ledger_entry_changes` already carries eviction/restoration
      change types we discard~~ — **audited 2026-07-22, recorded in 0436.** Both:
      restoration reaches us and is handled everywhere EXCEPT contracts, where
      `extract_contract_deployments` (`state.rs:60`) filters to
      `change_type == "created"` and drops it; `evicted_keys` sits on
      `LedgerCloseMetaV1`/`V2` and is never read (zero occurrences in `crates/`).

## Acceptance Criteria

- [x] A verdict on the archival hypothesis with external evidence — **done
      2026-07-22 via `getLedgerEntries` with a positive control.**
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
