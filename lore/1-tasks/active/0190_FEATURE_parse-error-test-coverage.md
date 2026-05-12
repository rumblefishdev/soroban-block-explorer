---
id: '0190'
title: 'Test coverage gap: transactions.parse_error=true path never exercised'
type: FEATURE
status: active
related_adr: []
related_tasks: []
tags:
  [
    priority-low,
    effort-small,
    layer-indexer,
    layer-xdr-parser,
    testing,
    observability,
  ]
links:
  - crates/xdr-parser/src/transaction.rs
  - crates/indexer/tests/persist_integration.rs
  - crates/api/src/runtime_enrichment/stellar_archive/mod.rs
history:
  - date: '2026-05-05'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned during /compare-with-stellar-api E03 (Statement B) verification.
      Sanity check on clone DB showed 0/10,118,806 transactions with
      parse_error=true. Code path is reachable (transaction.rs:133) but
      crates/indexer/tests/persist_integration.rs has 12 fixtures with
      parse_error=false and zero with true — DB persist + API Archive
      overlay handlers for parse_error rows are entirely untested.
  - date: '2026-05-12'
    status: active
    who: karolkow
    note: 'Activated. Bundling with 0193 in a single PR (both effort-small, no shared file surface but combined to reduce lifecycle overhead).'
  - date: '2026-05-12'
    status: active
    who: karolkow
    note: >
      Scope expanded after discovering a contract gap in the API
      archive-overlay path. Step 0 (production fix): explicit
      `if tx.parse_error { heavy = None }` short-circuit in
      `crates/api/src/transactions/handlers.rs` so parse_error tx serves
      `heavy: null + heavy_fields_status: "unavailable"` per the lore-0044 /
      lore-0046 contract (previously the handler unconditionally re-fetched
      from S3 and could either mask the DB flag with fresh heavy fields or
      return a status="ok" envelope with NULL XDR — both violations).
      Tests: (1) `xdr-parser::transaction::parse_error_tests` — Variant A
      missing-envelope, Variant B encode-failure via tight Limits, plus a
      default-limits regression sentinel; (2)
      `indexer::tests::persist_integration::parse_error_transaction_persists_and_replays_idempotent`
      — end-to-end persist + replay idempotence; (3)
      `api::tests_integration::detail_parse_error_tx_returns_unavailable_heavy_without_s3_contact`
      — locks the API contract. Step 3 observability counter dropped as
      overkill — the existing `info!(parse_errors = ...)` tracing log in
      `crates/indexer/src/handler/process.rs:143` already satisfies the
      spec's optional tier; CloudWatch alarm wiring spawned only if prod
      ever surfaces a non-zero count. Empty-source persist crash surfaced
      during fixture work spawned as lore-0209.
---

# Test coverage gap: `transactions.parse_error=true` path never exercised

## Summary

The `transactions.parse_error` flag is reachable code, set by
`extract_single_transaction` in `crates/xdr-parser/src/transaction.rs:133`
when XDR re-serialization fails or the envelope can't be aligned with
`tx_processing`. In production we observe **0/10.1M** rows with
`parse_error=true` (canonical Stellar archive almost never trips the
conditions), but the test pyramid covers only the `false` path. Add unit
and integration coverage so the degraded-tx pipeline is validated, and
optionally an observability counter so silent accumulation triggers an
alert.

## Context

Discovered during `/compare-with-stellar-api` E03 verification (Statement B
header). Sanity check on the clone DB:

```sql
SELECT parse_error, COUNT(*) FROM transactions GROUP BY parse_error;
→ false | 10,118,806
→ true  | 0
```

Code review confirmed the flag is **not dead code**. Three reachable
triggers in `transaction.rs:133`:

1. `envelope.is_none()` — `extract_envelopes` failed to align `tx_set`
   with `tx_processing` (corrupt archive / hash mismatch). Warns
   `"envelope missing for transaction — parse_error"` at
   `transaction.rs:127`.
2. `envelope_xdr.is_empty()` — `encode_xdr` returned `""` after
   `to_xdr` failure, e.g. XDR `Limits` exceeded. Warns
   `"XDR serialization failed: {e}"` at `transaction.rs:166`.
3. `result_xdr.is_empty()` — same shape, for the `TransactionResult`
   blob.

The DB persist path and the API Archive-overlay handlers for
`parse_error=true` rows (`crates/api/src/runtime_enrichment/stellar_archive/mod.rs:241,326`,
which gate behind `if !t.parse_error` and `if tx.parse_error`) are not
exercised by any test. If a real parse failure ever lands in production,
its blast radius across persist + read paths is untested.

`crates/indexer/tests/persist_integration.rs` carries 12 fixtures with
`parse_error: false`, zero with `true`.

## Implementation Plan

### Step 1: `xdr-parser` unit test

Add a unit/integration test in `crates/xdr-parser/tests/` that constructs
a `LedgerCloseMeta` with intentionally bad input and asserts the failure
shape:

- Variant A — missing envelope: build a meta where `tx_set` lacks the
  envelope corresponding to a `tx_processing` entry (or hash mismatch
  defeats `extract_envelopes`); assert `ExtractedTransaction.parse_error
== true` and `envelope_xdr.is_empty()` and `source_account.is_empty()`.
- Variant B (optional) — encode failure: feed an envelope past
  `xdr_limits::serialization_limits()` and assert the same shape via
  the `encode_xdr` empty-string branch.

### Step 2: indexer integration fixture

Extend `crates/indexer/tests/persist_integration.rs` with at least one
`ExtractedTransaction { parse_error: true, … }` fixture:

- Empty `envelope_xdr`, `result_xdr` (mirrors real parse-failure shape).
- Empty `source_account` per `transaction.rs:129`.
- Assert persist completes without error and the row lands in
  `transactions` with `parse_error = true`.
- Assert downstream queries (E02 Statement A, E03 Statement B) return
  the row with the flag intact.
- Verify the API Archive-overlay path (`stellar_archive/mod.rs:326`)
  short-circuits on `parse_error=true` and surfaces only DB-only fields
  (no envelope/memo/signatures synthesis on degraded rows).

### Step 3 (optional): observability counter

Emit a `tracing` / Prometheus counter `parse_error_total` from the
indexer so production occurrences trigger an alert before they
accumulate silently. Suggested threshold: any non-zero increment in a
24-hour window pages oncall (these should be effectively never).

## Acceptance Criteria

- [x] Unit tests in `crates/xdr-parser/src/transaction.rs::parse_error_tests`
      assert `parse_error=true` for **both** corrupt-input scenarios:
      Variant A (`variant_a_missing_envelope_marks_parse_error_true_for_unaligned_slot`)
      and Variant B (`variant_b_encode_failure_marks_parse_error_true_even_with_aligned_envelope`),
      plus a `default_limits_keep_parse_error_false_for_aligned_envelope`
      regression sentinel.
- [x] Integration test in `crates/indexer/tests/persist_integration.rs`
      (`parse_error_transaction_persists_and_replays_idempotent`) covers a
      `parse_error: true` fixture end-to-end (persist + replay idempotence + DB row + transaction_hash_index + empty appearance tables for the
      degraded row).
- [x] API `stellar_archive` overlay path verified to gracefully skip
      enrichment on `parse_error=true` rows. Implemented via Step 0
      production fix in `crates/api/src/transactions/handlers.rs` (explicit
      short-circuit before S3 fetch) and locked by
      `crates/api/src/tests_integration.rs::detail_parse_error_tx_returns_unavailable_heavy_without_s3_contact`.
- [x] (Optional) Indexer emits a counter for `parse_error_total`.
      **Satisfied via existing tracing field**: `info!(parse_errors = ...)`
      in `crates/indexer/src/handler/process.rs:143` is scrapable via the
      Lambda Logs metric-filter declarative path. Explicit CloudWatch
      MetricDatum emission deferred — recommended only if a non-zero count
      ever surfaces in production (zero in 10.1M rows today).
- [x] **Docs updated** — original spec said N/A, but Step 0 changed the
      handler's archive-fetch behavior for parse_error rows, so per ADR
      0032 the affected sections of `docs/architecture/**` were refreshed: - `docs/architecture/xdr-parsing/xdr-parsing-overview.md` §7.1 —
      documents the read-time short-circuit + cross-references the test
      suite. - `docs/architecture/backend/backend-overview.md` §6 cache-control
      table — documents the parse_error short-circuit alongside the
      existing `heavy_fields_status` conditional.

## Notes

- Source DB during discovery: clone of `sbe-audit-postgres-1` snapshotted
  at ledger 62046000 (~10.1M transactions). Distribution:
  `parse_error=false` in 100% of rows.
- Root finding emerged via `/compare-with-stellar-api
docs/architecture/database-schema/endpoint-queries/03_get_transactions_by_hash.sql`
  Statement B sanity check.
- The flag is intentionally preserved (records never dropped on parse
  failure) per `crates/xdr-parser/src/transaction.rs:1-5` and
  `crates/xdr-parser/src/error.rs:1-5`. The downstream contract is "you
  may receive a transactions row whose XDR-derived fields are absent —
  handle it." That contract has no automated guarantor today.
