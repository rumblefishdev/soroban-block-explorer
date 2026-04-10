---
id: '0134'
title: 'BUG: add envelope/meta ordering validation in indexer'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0024', '0029']
tags: [priority-medium, effort-small, layer-indexer, audit-F18]
milestone: 1
links:
  - crates/xdr-parser/src/envelope.rs
  - crates/indexer/src/handler/process.rs
  - crates/xdr-parser/src/transaction.rs
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-04-10'
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from pipeline audit finding F18 (MEDIUM). Silent data corruption risk from envelope/meta mismatch.'
---

# BUG: add envelope/meta ordering validation in indexer

## Summary

`extract_envelopes()` and `collect_tx_metas()` are extracted independently from different
fields of `LedgerCloseMeta`. There is no assertion that their lengths match and no
hash-based cross-check that each envelope corresponds to its meta. On mismatch, the code
silently produces corrupted data (wrong operations attributed to wrong transactions) via
`envelopes.get(i)` returning `None`.

For V1/V2 parallel Soroban phases (`TransactionPhase::V1`), the flattening order across
execution stages and clusters relies on protocol convention with no runtime verification.

## Context

Currently correct by Stellar protocol convention. But if a future protocol change,
Galexie bug, or edge case causes ordering divergence, the indexer will silently pair
envelopes with wrong metas. The `parse_error` flag will be set on missing envelopes,
but a reordering (same count, wrong order) would produce silently corrupt data.

## Implementation

1. Add `assert_eq!(envelopes.len(), tx_infos.len())` in `process.rs` after extraction.
   On mismatch, fail the entire ledger (do not silently degrade).
2. Add hash-based cross-check: compute SHA256 of each envelope and verify it matches the
   `transaction_hash` from `TransactionResultPair`. This confirms correct pairing.
3. If hash check fails for any transaction, log error and set `parse_error = true` for
   that specific transaction instead of silently using wrong data.

## Acceptance Criteria

- [ ] Length assertion: indexer fails loudly if envelope count != meta count
- [ ] Hash cross-check: each envelope's computed hash matches its result pair hash
- [ ] On hash mismatch: transaction marked with parse_error, not silently corrupted
- [ ] Tests: simulated mismatch triggers expected error behavior
