---
id: '0375'
title: 'Fee-bump inner_tx_hash lookup — resolve the hard 404'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-small, layer-api, fee-bump]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker. K3-2.'
  - date: 2026-07-13
    status: active
    who: karolkow
    note: 'Activated for implementation.'
  - date: 2026-07-13
    status: completed
    who: karolkow
    note: >
      Fix in PR #329. 3 code changes (writer index inner hash, fetch_detail
      hash-OR-inner, heavy fetch on outer hash) + 1 staging test. Verified
      against mainnet on 4 explorers. Historical backfill of inner-hash index
      rows + dict reload is a live CH-side dependency (ops, separate).
---

# Fee-bump inner_tx_hash lookup

## Summary

A lookup by a fee-bump's **inner** transaction hash returns a hard 404 — the
`inner_tx_hash` is stored on the transaction but never indexed for lookup, so
`/transactions/{inner_hash}` cannot resolve to the fee-bump.

## Context

Spawned from 0359 (K3-2). Read-side / index only: `ExtractedTransaction.inner_tx_hash`
is already captured (0359 read verified). Needs an index / hash-index row so the
inner hash resolves to the wrapping fee-bump tx (matches Horizon's
`inner_transaction.hash` lookup).

## Implementation

- Index the fee-bump `inner_tx_hash` (extend `transaction_hash_index` or add a
  lookup path) so `/transactions/{inner_hash}` resolves to the fee-bump.
- No re-parse: the hash is already stored; this is a read/index change.

## Acceptance Criteria

- [x] `/transactions/{inner_tx_hash}` resolves to the fee-bump tx (no 404) —
      immediate for newly-ingested fee-bumps; **historical rows resolve once the
      CH-side backfill of inner-hash index rows lands** (separate ops task).
- [x] resolves to the fee-bump, both hashes exposed (`hash` = outer,
      `inner_tx_hash` = inner). We canonicalize the primary `hash` to the outer
      (fee-bump), like Stellar Expert — **not** Horizon's echo-inner. See Design
      Decisions.

## Implementation Notes

Three code touch-points (the lookup is a two-step: hash-index → detail read),
each blind to the inner hash before the fix:

1. `crates/db-clickhouse/src/persist/stage.rs` — writer now pushes a second
   `TransactionHashIndexRow { hash: inner_tx_hash, ledger_sequence }` for
   fee-bumps, so step 1 (`transaction_hash_index`) resolves the inner hash.
2. `crates/api/src/transactions/queries.rs` (`fetch_detail`) — `WHERE
ledger_sequence = ? AND (t.hash = unhex(?) OR t.inner_tx_hash = unhex(?))`;
   the `OR` runs inside the already-pruned partition, so it is cheap despite
   no bloom index on `inner_tx_hash`.
3. `crates/api/src/transactions/handlers.rs` — key the archive/heavy fetch on
   the resolved outer `tx.hash`, not the queried hash; otherwise the heavy
   block is silently blank on an inner-hash lookup (the archive meta indexes
   the applied/outer hash).

Test: `prepare_fee_bump_indexes_inner_hash` in `persist/tests_cross.rs`
asserts a fee-bump stages **two** index rows (outer + inner) → same ledger.
No API schema change → api-types codegen produces no diff.

## Issues Encountered

- Fee-bumps are ~20% of all txs on prod (762M / 3.86B measured via `chq`), so
  the companion-index and its backfill are load-bearing.
- Single-shot `INSERT SELECT` / `GROUP BY inner_tx_hash` over the full set
  OOMs on prod CH (hit `MEMORY_LIMIT_EXCEEDED`). The backfill must be batched
  by ledger partition. Flagged to ops.

## Design Decisions

### From Plan

1. **One row per fee-bump, inner hash as a lookup key (not a second row).**
   Matches protocol reality (exactly one applied tx per fee-bump; the inner is
   nested, not independently applied) and matches Horizon / Stellar Expert.
   Two rows would double-count tx totals and invent a ledger entry.

### Emerged

2. **Canonicalize primary `hash` to the outer, not echo the inner.** Surveyed
   5 sources on mainnet: Horizon / Blockchair / stellarchain echo the queried
   inner hash; Stellar Expert (and now us) return the fee-bump with `hash` =
   outer. All expose both hashes, so information parity holds. Echo-inner would
   need a new DTO field + api-types regen for a cosmetic label swap — deferred
   as an optional follow-up, not built.
3. **No uniqueness guard added.** A hash mapping to two ledgers is prevented by
   seqnum consumption (a tx applies once) on top of sha256 collision-resistance;
   measured 0 collisions over ~4M recent fee-bumps. The index invariant holds
   without extra code.
4. **Backfill left to the CH-side ops job**, not scripted here — it is already
   owned separately. Writer covers forward; ops covers historical + dict
   reload.

## Future Work

- Optional (not spawned): echo-inner primary hash for Horizon-exact parity —
  low value, needs DTO change. Revisit only if requested.
- Optional (not spawned): replace the PG-derived `transaction_hash_index` +
  `transaction_hash_dict` with a native CH projection keyed by hash. Larger
  refactor; explicitly out of scope.
