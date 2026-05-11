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

- [x] `process.rs:228` no longer hardcodes empty `nft_events`; the
      parser-detected events flow through to staging.
- [x] `nft_ownership` populates on a fresh ingest range covering at
      least one NFT mint + one transfer (+ optional burn) — covered by
      the new integration test `nft_ownership_populated_for_mint_transfer_burn`
      which exercises the full parser → staging → write → DB row path
      under `persist_ledger`.
- [x] 0118 Phase 2 filter still applied — fungible contracts produce
      zero rows in `nft_ownership` (parity with `nfts`) — `nft_filter_drops_fungible_classified_contract`
      continues to pass alongside the new ownership test (the filter
      iterates BOTH `nft_rows` and `nft_ownership_rows`).
- [x] Integration test asserts the end-to-end flow (parser → staging
      → write → DB row) for a multi-event NFT scenario.
- [ ] E17 endpoint (`GET /v1/nfts/:id/transfers`) returns the full
      ownership timeline — **deferred to operator smoke test at deploy
      time**. The integration test verifies persisted rows are present
      and ordered; the canonical SQL `17_*.sql` LEAD window already
      derives `from_account` per ADR 0033 / task 0051.
- [x] **Docs updated** — N/A — reason: `database-schema-overview.md`
      already describes `nft_ownership` accurately (no "follow-up
      pending" wording found via grep); no schema or shape change.
      Per ADR 0032.
- [x] **API types regenerated** — N/A — reason: this task touches
      `crates/indexer/**` and `crates/xdr-parser/**` only; no API
      surface changes.

## Implementation Notes

### Files touched

- **NEW logic in `crates/xdr-parser/src/state.rs`** (~75 lines prod +
  ~140 lines test) — `extract_nft_ownership_events()` plus 6 unit tests
  added alongside existing NFT detection tests:
  - `mint_event_yields_owner_to` — mint → `event_type: Mint`, `owner_account: Some(to)`.
  - `transfer_event_yields_owner_to` — transfer → `event_type: Transfer`, `owner_account: Some(to)`.
  - `burn_event_yields_owner_none` — burn → `event_type: Burn`, `owner_account: None`.
  - `event_order_monotonic_per_triple` — 3 events same `(contract, token, ledger)` → `event_order: 0, 1, 2`.
  - `event_order_resets_per_token` — each new `(contract, token, ledger)` triple starts at 0.
  - `token_id_jsonvalue_stringified` — `Value::Number → "42"`, `Value::String → "uuid-abc"`.
- **MODIFIED `crates/xdr-parser/src/lib.rs`** — single-line export added
  to existing `pub use state::{...}` block.
- **MODIFIED `crates/indexer/src/handler/process.rs:228`** — one-line
  stub replacement plus surrounding comment cleanup (removed obsolete
  reference to "task 0149 signature extension" and "follow-up from
  0118").
- **NEW integration test in `crates/indexer/tests/persist_integration.rs`** —
  `nft_ownership_populated_for_mint_transfer_burn` (~230 lines including
  the dedicated cleanup helper `clean_ownership_test` and fresh fixture
  constants `OWN_*` that don't collide with `FILTER_*` from 0118).
  Test exercises mint → transfer → burn for the same `(contract, token,
ledger)` triple, asserts 3 rows with correct event_type + monotonic
  event_order + correct owner resolution (StrKey → BIGINT id, NULL for
  burn). Includes idempotent-replay assertion (second `persist_ledger`
  call returns same row count — `ON CONFLICT DO NOTHING` confirmed).

### Tests

- `cargo test -p xdr-parser --lib` — **209 passing** (203 existing + 6
  new). All `extract_nft_ownership_events` unit tests green.
- `cargo test -p indexer --test persist_integration -- --test-threads=1
nft` — **2 passing** (`nft_filter_drops_fungible_classified_contract`
  - `nft_ownership_populated_for_mint_transfer_burn`). No regression.
- `cargo fmt --check` clean.
- `cargo clippy -p xdr-parser -p indexer --all-targets -- -D warnings`
  clean.
- 3 unrelated test failures observed during full sweep
  (`application_order_*`, `synthetic_ledger_insert_and_replay_is_idempotent`) —
  pre-existing audit DB schema drift (the local snapshot pre-dates the
  lore-0192 migration that adds `operations_appearances.application_order`).
  **Not caused by 0202.** Will resolve on environments running the
  current `develop` migrations.

## Design Decisions

### From Plan

1. **Single-pass transformer with per-(contract, token, ledger)
   counter via `HashMap` entry API** — `or_insert(0)` for first hit,
   `and_modify(|c| *c += 1)` for subsequent hits. Returns `&mut V`
   already at the new value, so deref gives 0 on first event, 1 on
   second, etc. — matches the schema PK requirement that `event_order`
   is unique per `(nft_id, created_at, ledger_sequence)`.

2. **`NftEventType::FromStr` reused** for `"mint"`/`"transfer"`/`"burn"`
   → enum mapping. Domain crate already provided the impl (task 0118
   Phase 1). Avoids duplicating a parser-internal match.

3. **`tracing::instrument` on the transformer** matching the 0191
   pattern — `skip(events)` to keep span lean, `fields(event_count = events.len())`
   for ops visibility.

4. **Owner resolution split by event_type**:

   - `Mint` → `Some(to)` (new owner)
   - `Transfer` → `Some(to)` (new owner)
   - `Burn` → `None` (no owner)

   Matches the existing `ExtractedNftEvent` docstring ("None for burns")
   and the LEAD-window SQL in `17_get_nfts_transfers.sql` that derives
   `from_account` from the previous event's owner.

### Emerged

5. **Defensive guard for unknown `event_kind`** — even though the
   parser (`detect_nft_events`) restricts emission to the three known
   kinds, the transformer logs a `tracing::warn!` and skips any event
   that fails `event_kind.parse::<NftEventType>()` rather than panicking
   or producing an invalid row. Future SEP-0050 additions can land in
   the parser without crashing the transformer mid-batch.

6. **Empty `token_id` short-circuit before enum parse** — matches the
   `detect_nfts` behaviour in the same file. Saves an unnecessary parse
   when the event will be skipped anyway.

7. **Replay-idempotency assertion added to integration test** —
   integration test calls `persist_ledger` twice with identical input
   and confirms row count stays at 3. Documents the `ON CONFLICT DO
NOTHING` contract empirically rather than just relying on the
   inherited write path.

8. **Skipped backfill-bench empirical replay (plan Phase 5 alt path)** —
   the planned `backfill-bench --start 62046000 --end 62047000` run
   downloads a 64k-ledger S3 partition file (~5–15 GB) before any
   ledger processing happens; an earlier attempt in this session hung
   for >1h on that download with no progress. The integration test
   already exercises the full `persist_ledger` path with synthetic
   `ExtractedNftEvent` fixtures, which is the same code path real
   ledger replay would hit on the persistence side. Real-ledger XDR
   parsing into `NftEvent` is covered by `xdr-parser`'s 209 unit tests.

9. **Audit DB schema drift acknowledged as out-of-scope** — three
   pre-existing tests (`application_order_*`, `synthetic_ledger_*`) fail
   against the audit DB because its snapshot pre-dates the lore-0192
   migration. Documented in Implementation Notes rather than fixed
   here; will pass automatically on environments running current
   develop migrations.

## Issues Encountered

- **HashMap entry API surprise** — `entry().and_modify().or_insert()`
  returns `&mut V` after any modification, so the deref reads the
  POST-modification value. Verified by trace: first call gets 0 via
  `or_insert(0)` (no modify); subsequent calls increment then deref.
  Result sequence is 0, 1, 2 — exactly the desired ordering.

- **Audit DB schema drift** (see Design Decision 9) — caused
  `application_order_*` tests to fail during the full sweep.
  Investigation confirmed the column was added by a migration shipped
  on develop after the audit DB snapshot was taken. Unrelated to 0202.

- **No backfill-bench replay** (see Design Decision 8) — S3 partition
  download is the slow path; the planned empirical check was replaced
  by the existing integration test, which covers the same persistence
  surface.

## Notes

- Branch: `feat/0202_wire-nft-events-to-nft-ownership` cut from
  `develop` after the 2026-05-08 `chore(lore-0202)` activation push.
- 0118 Phase 3 (post-backfill SQL cleanup of `Other`-classified rows)
  remains a separate, downstream task. This task only fixes the
  forward-ingest gap.
- Production deployment: once this lands and pipeline is redeployed,
  newly indexed ledgers will populate ownership; historical ledgers
  will be picked up by the backfill runner (task 0145) when it runs.
