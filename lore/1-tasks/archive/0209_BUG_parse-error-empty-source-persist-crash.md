---
id: '0209'
title: 'Persist crash on parse_error tx with empty source_account'
type: BUG
status: completed
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
  - date: 2026-05-13
    status: active
    who: karolkow
    note: >
      Promoted backlog→active. Reproducer already on
      `feat/0190-0193` as
      `persist_integration.rs::parse_error_empty_source_crashes_persist_until_bug_fixed`
      (DATABASE_URL-gated, asserts exact "unresolved StrKey for
      transactions.source" message). Plan: Option A sentinel
      G-strkey in staging.rs; regression test flips reproducer
      to happy-path assertion.
  - date: 2026-05-13
    status: completed
    who: karolkow
    note: >
      Option B (nullable source_id) implemented instead of Option A.
      Migration drops NOT NULL; staging skips empty source from both
      `account_keys_set` and `participants_per_tx` (latter critical —
      `insert_participants` hard-resolves); write layer routes via
      `resolve_opt_id`. API DTO + 3 query sites (transactions list/detail,
      ledger tx list) moved to `Option<String>` + LEFT JOIN. Reproducer
      flipped to happy-path: persist OK, `source_id IS NULL`, replay
      idempotent. Arch docs (schema-overview + technical-design)
      updated. Types regenerated.
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

- [x] Persist path no longer aborts on `ExtractedTransaction { parse_error: true, source_account: "", ... }`
- [x] Replay of same ledger is idempotent
- [x] New test in `persist_integration.rs` covers empty-source variant end-to-end (`parse_error_empty_source_persists_with_null_source_id`)
- [x] Existing `parse_error_transaction_persists_and_replays_idempotent` preamble updated (no more gap caveat)
- [x] **Docs updated** — `docs/architecture/database-schema/database-schema-overview.md` + `docs/architecture/technical-design-general-overview.md` reflect nullable `source_id` + LEFT JOIN rule
- [x] **API types regenerated** — `libs/api-types/src/{openapi.json,generated/types.gen.ts}` updated; `source_account` now `Option<String>` on `TransactionListItem` + `TransactionDetailLight`

## Implementation Notes

**Chosen approach**: Option B (nullable `source_id`), not Option A as originally planned.

Changes (12 files):

- Migration `20260513090000_transactions_source_id_nullable.{up,down}.sql` — `DROP NOT NULL` on `transactions.source_id`
- `crates/indexer/src/handler/persist/staging.rs` — `TxRow.source_str_key: Option<String>`; skip empty source from `account_keys_set` AND `participants_per_tx` (latter critical: `insert_participants` hard-resolves and would crash with the same kind of error as the original bug)
- `crates/indexer/src/handler/persist/write.rs` — `source_ids: Vec<Option<i64>>` via `resolve_opt_id`
- `crates/api/src/transactions/{queries,dto}.rs` — `TxListRow`/`TxDetailRow`/`TransactionListItem`/`TransactionDetailLight.source_account: Option<String>`; 2× `JOIN` → `LEFT JOIN` (unfiltered list + detail by hash)
- `crates/api/src/ledgers/queries.rs` — `LedgerTxRow.source_account: Option<String>`; 1× `JOIN` → `LEFT JOIN`
- `crates/indexer/tests/persist_integration.rs` — reproducer flipped to happy-path; preamble of populated-source test updated
- 2 arch docs (schema-overview, technical-design)
- API types regen

Paths that drive through operations / events / pools / assets stay as plain `JOIN` — parse_error tx never has rows in those tables, so the inner join silently excludes them which is the desired behaviour.

## Design Decisions

### Emerged

1. **Option B over A**: Original task recommended Option A (sentinel G-strkey). Reversed during implementation after deeper analysis — sentinel pollutes `accounts` table and introduces theoretical collision with real all-zero ed25519 keypair. NULL is semantically correct ("source unknown"), respects arch-doc contract from `technical-design-general-overview §5.4` + `xdr-parsing-overview §7.1` ("transaction still displayed with partial columns"), and frontend already differentiates via the `parse_error` flag.

## Notes

- Source: `crates/xdr-parser/src/transaction.rs:115-131` envelope-missing branch of `extract_envelopes` mismatch with `tx_processing`.
- Other parse_error variants (`envelope_xdr.is_empty()`, `result_xdr.is_empty()`) keep source filled — not affected.
- Prefer Option A unless wider NULL-source semantics are wanted for other reasons.
