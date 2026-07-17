---
id: '0399'
title: 'REFACTOR: arm-A NFT pollution — root cause + fungible/NFT separation in operation_asset_appearances'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0393', '0383', '0359', '0309']
tags:
  [
    clickhouse,
    indexer,
    xdr-parser,
    soroban,
    nft,
    data-hygiene,
    effort-small,
    priority-low,
  ]
milestone: 1
links:
  - crates/db-clickhouse/src/persist/stage.rs
  - crates/api/src/common/ch.rs
history:
  - date: '2026-07-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0393 future work. The event-derived arm-A write is
      classification-blind, so bespoke NFT token events write dead rows (with a
      bogus amount parsed from token_id) into operation_asset_appearances. Read
      is correct today (INNER JOIN assets is fungible-only), but the rows pollute
      the table at rest. Investigate the root cause and pick a permanent strategy.
---

# REFACTOR: arm-A NFT pollution — fungible/NFT separation

## Summary

The event-derived write into `operation_asset_appearances` (arm A) pushes a
presence + amount row for **every** decoded SEP-41/CAP-67 token event, without
consulting the emitting contract's `contract_type`. Bespoke **NFT** contracts
emit shape-identical `transfer`/`mint` events, so they write dead rows — with an
`amount` mis-parsed from the NFT's `token_id`. The read is already correct (the
value query's `INNER JOIN assets` sees fungible-only rows, so NFT rows are
dropped), but the table carries garbage at rest. Decide the permanent fix.

## Context

Parent: [0393](../active/0393_FEATURE_transaction-value-amount-column/README.md)
added the net-settled `amount` and routed **bespoke fungible** tokens into arm A
(`EventAsset::Contract → emitting contract surrogate`). Bespoke NFTs ride the
same code path because nothing separates them at write time.

## Root Cause (already investigated — this is the seed, not open)

Three layers, deepest last:

1. **Write is unconditional** — `stage.rs` (~L1134-1148, the event-derived
   `op_asset_rows` loop) pushes a row for every token event; it never checks the
   contract's verdict. `derive_token_event` / `event_asset_surrogate` resolve an
   `asset_id` for `EventAsset::Contract` regardless of NFT vs fungible.
2. **NFT and fungible events are shape-identical (the crux).** CAP-67 unifies
   `transfer`/`mint`/`burn` topics — `parse_token_event(topics)` cannot tell them
   apart. The only in-event difference is `data`: NFT = `token_id`, fungible =
   `amount i128`. **Both are a bare integer**, so `token_event_amount` happily
   reads an NFT `token_id` as an "amount". The discriminator does not live in the
   event.
3. **The real discriminator (WASM classification) is out-of-band and late.**
   `contract_type` (Fungible/Nft) comes from WASM-spec analysis, but
   `uploadContractWasm` and `createContract` are **separate transactions** (often
   different ledgers/batches). A token event can be ingested before the verdict is
   known (mid-stream backfill; upload in a later pass). Unknown ⇒ `Other`/`None`,
   never `Nft`, so you cannot cleanly "drop NFT" at write time without risking a
   dropped-but-unclassified fungible. Late classification means rows are already
   written by the time the verdict lands.

That trio is why 0393 shipped **write-all + read-time filter** (`INNER JOIN
assets`, which is fungible-only and 100% certain because `assets` gets a row only
for `contract_type == Fungible`).

## Candidate strategies (pick one, or a combination)

1. **Keep read-time filter, add periodic dead-row GC.** Leave the write as-is;
   the read is already correct. Add a scheduled `DELETE`/`OPTIMIZE`-style sweep of
   `operation_asset_appearances` rows whose `asset_id` has no `assets` row (or a
   confirmed `Nft` verdict). Simplest; accepts transient pollution.
2. **Ingest-time gate.** Consult the verdict (`prior_contract_verdicts` /
   `prior_wasm_verdicts`, task 0283/0309) in the arm-A write and skip a confirmed
   `Nft`. Must define behaviour for **unknown** (write, to avoid dropping a
   not-yet-classified fungible) and a **backfill re-gate** for contracts classified
   _after_ their events were ingested. Partial by construction (late classification).
3. **Data-shape discrimination** — reject a movement whose `data` looks like a
   token_id. Fragile: token_id and amount are both integers; NOT recommended alone.

The NFT gate (option 2) is explicitly in scope here as one candidate — evaluate,
don't assume.

## Investigation steps

- Measure the actual dead-row volume in prod (NFT token-event rows vs fungible)
  to size the problem — a launchpad mint burst is the worst case.
- Confirm whether the bogus `amount` (token_id-as-amount) can ever leak to a read
  (it should not: no `assets` row → `INNER JOIN` drop → `HAVING != 0` moot).
- Decide: is read-time filter + GC enough, or is the ingest gate worth the
  late-classification complexity?

## Acceptance Criteria

- [ ] Root cause documented (done above) and confirmed against prod row counts.
- [ ] A permanent strategy chosen with rationale (read-filter+GC vs ingest gate).
- [ ] If a gate: unknown-verdict + late-classification re-gate behaviour defined
      and covered by a test.
- [ ] No regression to the read: fungible bespoke still surfaces, NFT never does.
- [ ] Docs updated per ADR 0032 if the ingest/schema shape changes.
