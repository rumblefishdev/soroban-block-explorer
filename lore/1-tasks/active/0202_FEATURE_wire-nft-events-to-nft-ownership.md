---
id: '0202'
title: 'Indexer: wire nft_events → nft_ownership population'
type: FEATURE
status: active
related_adr: ['0027', '0029', '0031', '0033']
related_tasks: ['0051', '0118']
tags: [layer-indexer, nfts, soroban, priority-medium, effort-small, follow-up]
links:
  - crates/indexer/src/handler/process.rs
  - crates/indexer/src/handler/persist/staging.rs
  - crates/indexer/src/handler/persist/write.rs
  - crates/xdr-parser/src/nft.rs
  - docs/architecture/database-schema/endpoint-queries/17_get_nfts_transfers.sql
history:
  - date: '2026-05-08'
    status: active
    who: stkrolikiewicz
    note: 'Activated for implementation. Picked up after CH mirror analysis confirmed nft_ownership = 0 rows across all partitions; surrounding plumbing (schema, parser, staging, write, API) already ready — only the wiring step in process.rs:228 stub remains.'
  - date: '2026-05-07'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from /compare-with-stellar-api E17 verification. Indexer
      hardcodes `let nft_events: Vec<...> = Vec::new();` in
      process.rs:228 with comment "follow-up from 0118". The 0051 NFT
      API module (incl. /v1/nfts/:id/transfers) is fully implemented
      against this table; the schema (36 partitions, indexes, FK) is
      ready; the staging + write paths exist. Only the parser→process
      plumbing is stubbed. Result: nft_ownership = 0 rows across all
      partitions even though nfts has 9.7M rows. Frontend §6.12
      "Transfer history" tab is permanently empty until this lands.
---

# Indexer: wire `nft_events` → `nft_ownership` population

## Summary

Connect `xdr_parser::detect_nft_events` (or equivalent) to the indexer
`process_ledger` flow so per-NFT ownership change events (mint /
transfer / burn) populate the `nft_ownership` partitioned table. The
write path already exists; the parser already produces the events; only
the wiring step in `crates/indexer/src/handler/process.rs:228` is
missing.

## Context

### What's already in place

- **Schema** (ADR 0027 §13, migration `0005_tokens_nfts.sql`):
  `nft_ownership` partitioned table with PK `(nft_id, created_at,
ledger_sequence, event_order)`, 36 monthly partitions populated
  through y2026m08, FK cascade from `transactions`.
- **Parser** (`crates/xdr-parser/src/nft.rs`): `detect_nft_events`
  exists and emits `ExtractedNftEvent` with all fields the indexer
  needs (`contract_id`, `token_id`, `transaction_hash`, `owner`,
  `event_type`, `ledger_sequence`, `event_order`, `created_at`).
- **Staging** (`crates/indexer/src/handler/persist/staging.rs:1006-1011`):
  consumes a `&[ExtractedNftEvent]` slice and constructs
  `Vec<NftOwnershipRow>` already.
- **Write** (`crates/indexer/src/handler/persist/write.rs:1592-1610`,
  step "12b. nft_ownership"): bulk-inserts the rows via UNNEST under
  the same persist envelope as `nfts`.
- **Task 0118 Phase 2 filter** (`resolve_nft_filter`,
  `crates/indexer/src/handler/persist/write.rs:1442-1496`): already
  filters both `nft_rows` AND `nft_ownership_rows` by classification —
  fungible/token contracts get dropped from BOTH slices in one pass.
  No additional filter work needed here.
- **API layer** (task 0051,
  `crates/api/src/nfts/queries.rs` + canonical SQL `17_*.sql`): wired
  to `nft_ownership` with the LEAD-window `from_account` derivation;
  acceptance criteria all checked.

### What's missing

`crates/indexer/src/handler/process.rs:228`:

```rust
// nft_events → `nft_ownership` rows (follow-up from 0118)
let nft_events: Vec<xdr_parser::types::ExtractedNftEvent> = Vec::new();
```

The vector is hardcoded empty. As long as it stays empty, `nfts` keeps
getting populated (mint detection via `detect_nfts`) but the per-event
ownership timeline never reaches DB. Result on the audit clone
snapshot: 9.7M `nfts` rows / 0 `nft_ownership` rows.

This was deferred when 0118 Phase 2 landed (2026-04-22) — the comment
explicitly tags it as a 0118 follow-up but no dedicated task tracked
the gap. 0051 was implemented as if `nft_ownership` were populated,
because the feature was scoped to the API layer; the empty table was
not visible at the API surface during tests (integration tests use
fixture rows inserted directly).

### Why pick this up now

- E17 endpoint (`GET /v1/nfts/:id/transfers`) currently always returns
  empty. Frontend §6.12 "Transfer history" tab is dead.
- Without ownership history, NFT detail page (E16, §6.12) cannot show
  the "Alice → Bob" provenance chain.
- 0118 Phase 3 (post-backfill cleanup) is gated on backfill (0145);
  wiring this in **before** backfill runs means the historical sweep
  populates `nft_ownership` correctly in one pass instead of needing a
  separate backfill of just ownership data later.
- The work is small (~50–100 LOC, plumbing only) and unblocks a whole
  feature surface that's already shipped and tested at the API layer.

## Implementation Plan

### Step 1: Verify parser surface

Confirm `xdr_parser::detect_nft_events` emits an `ExtractedNftEvent`
shape that matches `staging::NftOwnershipRow` field-for-field. Both
consume identically per current code (`staging.rs:1006-1013`); audit
for any drift since the parser PRs landed (Phase 1 PR #104, Phase 2
2026-04-22).

If a field is missing on the parser side (e.g. `event_order` is not
emitted yet — the parser may currently produce `0` or skip it), add it
in this step. `event_order` must be the per-`(nft_id, ledger_sequence)`
ordinal (SMALLINT, CHECK 0..=15 per ADR 0027 §13 — required for
deterministic LEAD-window pagination on multi-event ledgers).

### Step 2: Plumb the call site

Replace the hardcoded empty in `process.rs:228`:

```rust
// before
let nft_events: Vec<xdr_parser::types::ExtractedNftEvent> = Vec::new();

// after
let mut nft_events: Vec<xdr_parser::types::ExtractedNftEvent> =
    Vec::with_capacity(/* a sensible cap derived from event count */);
for tx_meta in &ledger_close_meta.transactions {
    nft_events.extend(xdr_parser::detect_nft_events(tx_meta));
}
```

Exact iteration shape depends on what `process.rs` already does for
`detect_nft_events` per-tx — line 114 already calls it (`let
nft_events = xdr_parser::detect_nft_events(&events);`) and pushes into
`all_nft_events`, so the existing accumulator may already hold the
right data. Likely fix is a one-liner: remove line 228's `Vec::new()`
shadow and pass `all_nft_events` (the real accumulator) to staging
instead.

### Step 3: Update integration test

Augment `nft_filter_drops_fungible_classified_contract` (or add a new
test) to ingest a fixture range with a real NFT contract emitting
mint + transfer + burn events, and assert:

- `nfts` row count for the NFT contract.
- `nft_ownership` row count = mint + transfer + burn events combined.
- `event_type` distribution matches expected (one mint, N transfers,
  optional burn).
- The 0118 filter still drops fungible-contract events from BOTH
  slices.

### Step 4: Sanity check on mainnet sample

After landing, point a development indexer at a small ledger range
known to contain real NFT activity (e.g. a Fractal NFT deploy + a
handful of transfers) and verify `nft_ownership` populates with the
expected rows. Confirm E17 endpoint returns a non-empty list with
correct LEAD-window `from_account` values.

## Acceptance Criteria

- [ ] `process.rs:228` no longer hardcodes empty `nft_events`; the
      parser-detected events flow through to staging.
- [ ] `nft_ownership` populates on a fresh ingest range covering at
      least one NFT mint + one transfer (+ optional burn).
- [ ] 0118 Phase 2 filter still applied — fungible contracts produce
      zero rows in `nft_ownership` (parity with `nfts`).
- [ ] Integration test asserts the end-to-end flow (parser → staging
      → write → DB row) for a multi-event NFT scenario.
- [ ] E17 endpoint (`GET /v1/nfts/:id/transfers`) returns the full
      ownership timeline (mint / transfer / burn) for at least one
      NFT in the test range.
- [ ] **Docs updated** — `docs/architecture/database-schema/**`:
      check the `nft_ownership` description for any "follow-up
      pending" wording and remove it. Mark `N/A — reason` if no
      shape changes. Per ADR 0032.
- [ ] **API types regenerated** — N/A — reason: this task touches
      `crates/indexer/**` and `crates/xdr-parser/**` only; no API
      surface changes.

## Notes

- Branch naming convention follows the project's existing pattern;
  suggested branch name: `feat/0202_wire-nft-events-to-nft-ownership`.
- 0118 Phase 3 (post-backfill SQL cleanup of `Other`-classified rows)
  remains a separate, downstream task. This task only fixes the
  forward-ingest gap.
- Consider whether the parser-emitted `event_order` is currently
  monotonic per `(nft_id, ledger_sequence)`. If it is not, that's a
  parser-side fix in the same PR — the SQL `17_*.sql` LEAD window and
  the 0051 cursor pagination both depend on it for determinism.
- Production deployment: once this lands and pipeline is redeployed,
  newly indexed ledgers will populate ownership; historical ledgers
  will be picked up by the backfill runner (task 0145) when it runs.
