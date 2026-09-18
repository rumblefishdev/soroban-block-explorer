---
id: '0558'
title: 'FEATURE: persist the concrete NFT token id on every asset transfer'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0424', '0538', '0540', '0542']
tags:
  [
    'clickhouse',
    'indexer',
    'xdr-parsing',
    'nft',
    'api',
    'effort-medium',
    'priority-medium',
  ]
links:
  - crates/xdr-parser/src/asset_transfers.rs
  - crates/db-clickhouse/src/persist/value_flow.rs
  - crates/db-clickhouse/src/persist/rows.rs
  - crates/db-clickhouse/schema/init.sql
  - crates/api/src/accounts/balance_changes.rs
history:
  - date: 2026-09-16
    status: backlog
    who: karolkow
    note: >
      Filed after task 0540's balance-change read had to reconstruct NFT piece
      ids from the previous-owner timeline. Production measurement disproved
      storage cost as a reason to omit the value: a Nullable(String) token_id
      on all 5.59 bn asset_transfers rows is estimated at 11.00 MiB (0.02446%
      of the current 43.91 GiB table). The remaining cost is the safe historical
      migration, not steady-state storage.
---

# Persist NFT token ids on asset transfers

## Summary

Make `asset_transfers` self-contained for non-fungible movements. A fungible
row carries `amount`; an NFT row carries the concrete `token_id`. Today an NFT
row says only that _some piece_ of a collection moved (`amount = NULL`), so the
account balance-change read must infer the pieces from `nft_ownership` history.
That inference is deliberately conservative, but it is a plaster over data the
event decoder already sees and then discards.

Target invariant for every accepted movement:

```text
fungible:      amount = Some(value), token_id = None
non-fungible:  amount = None,        token_id = Some(value)
```

An accepted row with both fields set or both fields empty is invalid. An event
whose semantics cannot be resolved is a counted reject, not an unnamed NFT.

## Why this is separate

- **0540** created the production value-flow table and is complete.
- **0538** gives `asset_transfers`, `soroban_events`, and `nft_ownership` one
  canonical event location. That exact identity remains necessary across the
  database even when `asset_transfers` also carries the value directly.
- **0542** creates one trusted token-event decoder, but its current plan
  explicitly says "no new column on `asset_transfers`". This task revisits that
  storage decision without widening 0542 implicitly.
- **0424** is superseded for this read only after the direct value is complete;
  ordering ownership history remains its own concern.

Implement after 0542 supplies the shared movement definition. Use 0538's NFT
ownership-location step for historical reconciliation and, if measured best,
for a ClickHouse-local backfill instead of another raw-XDR pass.

## Production measurement (2026-09-16)

Read-only queries ran against production ClickHouse. The storage estimate used
ClickHouse's `estimateCompressionRatio('ZSTD(3)')` on the real NULL distribution
of `asset_transfers`; no production table was created.

| Measure                                     |        Result |
| ------------------------------------------- | ------------: |
| `asset_transfers` rows                      | 5,591,548,039 |
| NFT rows (`amount IS NULL`)                 |        22,462 |
| NFT share                                   |   0.00040171% |
| current table size                          |     43.91 GiB |
| estimated `Nullable(String) CODEC(ZSTD(3))` | **11.00 MiB** |
| table growth                                |  **0.02446%** |
| estimated bytes per transfer                |      0.002062 |

Five partitions from different epochs estimated 228–422 KiB each. Across
`nft_ownership` plus quarantine there are 23,721 ownership rows and 14,258
distinct `(contract, token)` pieces. Token ids have median and p95 length 5,
maximum 19; their entire observed payload is 104,736 bytes raw and about
27.12 KiB under the same ZSTD estimator. Even allowing 10–23% unmerged-part
overhead, the new column should remain below roughly 15 MiB.

This measurement settles only steady-state storage. Migration I/O, merge load,
and read latency still require their own gates.

## Design constraints

1. **One decoder.** Extend 0542's shared movement result so NFT recognition
   returns its canonical string token id. `nft.rs`, `asset_transfers.rs`, and
   ownership derivation must not parse the same payload independently.
2. **One event may contain several pieces.** Consecutive/batch mint shapes can
   encode multiple token ids in one event. Prove the production shape inventory
   before fixing the row design. If an event expands to several movements,
   either add a stable movement ordinal to row identity and emit one row per
   piece (preferred, consistent with "one row per movement"), or document and
   measure an array representation. A singular column that silently drops all
   but one id is not acceptable.
3. **Canonical rendering.** Use the same token-id string representation as
   `nft_ownership`; byte/string/integer forms must compare exactly after the
   shared decoder resolves them.
4. **Safe unknowns.** Ambiguous `i128`, unsupported batch shapes, and missing
   executable-version evidence remain counted rejects under 0542. `NULL` must
   never mean both "fungible amount absent" and "NFT id unknown".
5. **Safe rollout.** The driver validates row structs against `DESCRIBE`, so
   DDL lands before the writer. The current table is a version-less
   `ReplacingMergeTree`; do not backfill by inserting competing rows and hoping
   the new copy wins. Use a measured rebuild-and-swap, or a versioned migration
   whose winner is deterministic.

## Implementation plan

1. **Settle the multi-piece row identity.** Inventory accepted and rejected
   production NFT shapes from 0542, measure the largest expansion, then choose
   singular rows plus a movement ordinal or an array. Record the choice and its
   effect on the `ORDER BY` key.
2. **Preserve the value in the parser.** Extend the shared decoded movement and
   `ExtractedAssetTransfer`; enforce the amount/token-id exclusive invariant in
   tests covering mint, transfer, burn, self-transfer, packed data, consecutive
   mint, and contract executable changes.
3. **Persist it.** Extend `AssetTransferRow`, staging, DDL, column-order tests,
   targeted-writer tests, and the 0540 reconciliation oracle. Apply
   `Nullable(String) CODEC(ZSTD(3))` unless the multi-piece decision requires a
   different physical shape.
4. **Backfill safely.** Benchmark two sources: raw-XDR targeted replay, and an
   exact join to canonically located `nft_ownership` after 0538. Choose the
   cheaper complete source, build a replacement table or deterministic versioned
   overlay, gate it partition by partition, and cut over using the established
   DDL-before-writer procedure.
5. **Simplify the reader.** Make account balance changes read token ids directly
   from `asset_transfers`; delete the previous-owner history reconstruction and
   retain the count guard only if it still protects a measured inconsistency.
6. **Verify production.** Reconcile every accepted NFT transfer against raw XDR
   and `nft_ownership`, record rejects by shape, benchmark the account read, and
   record the final compressed column size.

## Acceptance criteria

- [ ] Every accepted fungible movement has `amount` and no `token_id`; every
      accepted NFT movement has `token_id` and no `amount`
- [ ] Multi-piece events preserve every token id with a deterministic row
      identity; no value is selected arbitrarily or collapsed silently
- [ ] Live ingest and historical replay use the same 0542 decoder and produce
      byte-identical transfer rows
- [ ] Historical coverage is gated partition by partition; every
      `asset_transfers` NFT row has its token id or a documented counted reject
- [ ] Reconciliation by 0538's canonical event location reports zero unexplained
      token-id mismatches against `nft_ownership`
- [ ] The migration is deterministic on `ReplacingMergeTree`; no competing
      version-less duplicates are introduced
- [ ] Account balance changes no longer infer NFT pieces from previous-owner
      history and retain the Alice/Bob/Carol, mint, burn, duplicate, and
      incomplete-data regression cases
- [ ] Actual compressed size and account-read latency are recorded and compared
      with the 11.00 MiB estimate and the pre-change query
- [ ] **Docs updated** — XDR parsing, ClickHouse schema, and API data contract
