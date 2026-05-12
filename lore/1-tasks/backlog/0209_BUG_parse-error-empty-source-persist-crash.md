---
id: '0209'
title: 'Persist crash on parse_error tx with empty source_account'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0190']
tags: ['indexer', 'persist', 'parse-error', 'robustness']
links: []
history:
  - date: 2026-05-12
    status: backlog
    who: karolkow
    note: >
      Spawned from lore-0190 integration test work. Originally drafted
      as 0208 but renumbered to 0209 because `0208_FEATURE_ch-liquidity-pools-state-semantics-correction`
      is already archived on `develop`. Live-DB reproducer landed on
      `feat/0190-0193` branch as
      `crates/indexer/tests/persist_integration.rs::parse_error_empty_source_crashes_persist_until_bug_fixed`
      (gated by `#[should_panic(expected = "unresolved StrKey for transactions.source")]`).
      Confirmed exact error chain documented below against a clean
      snapshot. The `#[should_panic]` will flip to a happy-path
      assertion once the fix lands. No production occurrences yet
      (0/10.1M rows).
---

# Persist crash on parse_error tx with empty source_account

## Summary

Indexer persist path aborts when `ExtractedTransaction { parse_error: true, source_account: "", ... }` reaches staging. Production-shape output of `crates/xdr-parser/src/transaction.rs:115-131` envelope-missing branch. Currently unreachable in real data (0/10.1M rows) but corrupt `LedgerCloseMeta` in the wild would crash indexer.

## Context

Failure chain:

1. `crates/indexer/src/handler/persist/staging.rs:317` inserts empty `source_account` into `account_keys_set`.
2. `staging.rs:454` filter `k.len() <= 56 && k.starts_with('G')` drops empty key.
3. `accounts` upsert never inserts row for `account_id = ''`.
4. `write.rs:643` calls `resolve_id(account_ids, &r.source_str_key, ...)` for tx source → `Err(HandlerError::Staging("unresolved StrKey for transactions.source: "))`. Whole persist tx aborts.

`transactions.source_id BIGINT NOT NULL REFERENCES accounts(id)` per [crates/db/migrations/0003_transactions_and_operations.sql:21](../../../crates/db/migrations/0003_transactions_and_operations.sql) — persist path can't skip it.

Surfaced during lore-0190 integration test work. Test in `crates/indexer/tests/persist_integration.rs` (`parse_error_transaction_persists_and_replays_idempotent`) uses `SRC_STRKEY` for source and documents empty-source gap in preamble.

## Implementation Plan

Pick lower blast-radius option:

### Option A (preferred): sentinel G-strkey

- In `staging.rs`, substitute deterministic sentinel for empty `source_account` before staging (e.g. all-zero `GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF`).
- Pre-insert one `accounts` row for sentinel.
- Adds one "ghost" accounts row globally. Contained.

### Option B: nullable source_id

- Pass `parse_error` flag to staging so tx row gets `NULL source_id`.
- Schema migration: `transactions.source_id` nullable.
- Broader blast radius (every consumer of `source_id` must handle NULL).

### Step: add regression test

After fix, new test in `crates/indexer/tests/persist_integration.rs` alongside `parse_error_transaction_persists_and_replays_idempotent`. Fixture: `source_account: String::new()`. End-to-end persist + replay idempotence.

Update preamble of existing test: remove empty-source gap caveat.

## Acceptance Criteria

- [ ] Persist path no longer aborts on `ExtractedTransaction { parse_error: true, source_account: "", ... }`
- [ ] Replay of same ledger is idempotent
- [ ] New test in `persist_integration.rs` covers empty-source variant end-to-end
- [ ] Existing `parse_error_transaction_persists_and_replays_idempotent` preamble updated (no more gap caveat)
- [ ] **Docs updated** — N/A unless schema changes (Option B). Option A = no schema/API doc change.
- [ ] **API types regenerated** — N/A (indexer-internal, no `crates/api/**` touch)

## Notes

- Source: `crates/xdr-parser/src/transaction.rs:115-131` envelope-missing branch of `extract_envelopes` mismatch with `tx_processing`.
- Other parse_error variants (`envelope_xdr.is_empty()`, `result_xdr.is_empty()`) keep source filled — not affected.
- Prefer Option A unless wider NULL-source semantics are wanted for other reasons.
