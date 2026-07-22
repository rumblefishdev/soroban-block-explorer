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

1. ~~**Do we ingest it, or query it on demand?**~~ — **ANSWERED 2026-07-22.
   Ingest. Every byte already reaches us; nothing needs fetching.** Details in
   the section below — this is the finding that resizes the task.
2. **What is the minimum useful representation?** Ranges from one nullable
   `archived_at_ledger` on `soroban_contracts` to full TTL tracking per entry.
   Start from the question a user asks — "is this contract alive?" — not from
   the protocol's full model.
3. **Does archival apply to entries we index beyond contracts?** Contract data
   and balances are `Persistent` entries too. Scope this deliberately.
4. **How does restoration interact with `ReplacingMergeTree`?** A restored
   contract reappears in state. The version column must make the restore win —
   the same trap as 0421, where a defaulting write outversioned the truth.

## Signal audit — where archival data already flows (2026-07-22)

Traced through the parser. The protocol gives us everything; we use part of it.

**`LedgerEntryChangeType` includes `Restored = 4`**, and
`LedgerCloseMetaV1`/`V2` both carry **`evicted_keys: VecM<LedgerKey>`** — in the
very structure we already deserialize.

| signal                                              | status                                         | what it costs                |
| --------------------------------------------------- | ---------------------------------------------- | ---------------------------- |
| restoration — accounts, balances, asset appearances | **already handled**                            | nothing                      |
| restoration — **contracts**                         | **arrives, then discarded by a filter**        | ~2 lines                     |
| eviction (`evicted_keys`)                           | **never read — zero occurrences in `crates/`** | new read path, no new source |

**Restoration is handled almost everywhere.** `ledger_entry_changes.rs:159` maps
the variant to `"restored"`, and `state.rs` consumes it in four places —
including `is_creation = matches!(change.change_type.as_str(), "created" |
"restored")` at `:478`.

**Contracts are the exception, and it is an inconsistency inside one file.**
`extract_contract_deployments` (`state.rs:60`) drops everything that is not
`created`:

```rust
if change.entry_type != "contract_data" || change.change_type != "created" {
    continue;
}
```

So a restored contract produces no row, while `state.rs:478` — same file —
treats `restored` as a creation for other entities. `contract.rs:23` has the
same shape for contract _code_: `Created | Updated` matched, `_ => continue`, so
a restored WASM is never parsed for its interface.

**Eviction is the only genuinely missing piece**, and even that is a missing
_read_, not a missing _source_: `evicted_keys` sits on the meta we already hold.

### What this changes about the task

The expensive-sounding part — getting the data — does not exist. Remaining work
is:

- two filter relaxations (contracts + contract code) to stop discarding
  restorations
- one new read of `evicted_keys` off `LedgerCloseMeta`
- **the real work:** deciding representation (question 2), settling the RMT
  versioning question (4) so a restore outranks the row that preceded it, and
  backfilling history

`effort-large` was set before this audit. On the ingest side it is small; the
size now lives in backfill and in the surfacing decision.

## Implementation sketch (not a decision)

- [x] ~~Determine whether eviction/restoration change types reach our parser~~ —
      **done 2026-07-22, see the signal audit above.** Restoration arrives and is
      discarded for contracts only; eviction arrives on the meta and is never
      read.
- [ ] **Filter fix 1 — `state.rs:60`.** `extract_contract_deployments` skips
      unless `change_type == "created"`. Accept `"restored"` too, matching
      `state.rs:478` in the same file. Careful: a restored contract must not be
      attributed a _new_ deployer — the restore carries no deploy authorization,
      so `deployer_id` should stay whatever it was (or NULL), not be re-derived.
      Coordinate with 0430, which is rewriting the deployer path.
- [ ] **Filter fix 2 — `contract.rs:23`.** `Created | Updated` matched, `_ =>
    continue`. A restored WASM never gets its interface parsed, so the contract
      stays unclassified. Add `Restored`.
- [ ] **New read — `evicted_keys`.** Present on `LedgerCloseMetaV1`/`V2`; we
      never touch it. Decide where it lands before writing the read — this is
      the only part that needs a new table or column, so question 2 gates it.
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

~~Question 1 is a half-hour read-only check…~~ — done, and it did change the
size. Ingest is cheap; the cost moved to backfill and to the RMT versioning
question. Re-estimate before scheduling.
