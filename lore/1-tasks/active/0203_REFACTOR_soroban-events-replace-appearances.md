---
id: '0203'
title: 'Replace soroban_events_appearances with full-content soroban_events table'
type: REFACTOR
status: active
related_adr: ['0033']
related_tasks: ['0157', '0158']
tags:
  [
    layer-backend,
    layer-db,
    layer-indexer,
    layer-api,
    schema,
    s3-read-path,
    adr-0033,
    priority-medium,
    effort-large,
  ]
links:
  - lore/2-adrs/0033_soroban-events-appearances-read-time-detail.md
  - /home/fishuser/Downloads/SOROBAN_EVENTS_V3.md
history:
  - date: '2026-05-08'
    status: backlog
    who: fmazur
    note: >
      Spawned from /home/fishuser/Downloads/SOROBAN_EVENTS_V3.md spec. Goal:
      graduate the v3 spike — full-content event store keyed on
      (contract_id, created_at, transaction_id, event_index) with
      topics_xdr/data_xdr/signature columns — into the canonical
      soroban_events table, replacing the folded soroban_events_appearances
      design from ADR 0033 / task 0157.
  - date: '2026-05-08'
    status: active
    who: fmazur
    note: 'Promoted to active to begin implementation.'
---

# Replace soroban_events_appearances with full-content soroban_events table

## Summary

Promote the `soroban_events_v3` spike to a canonical `soroban_events` table
that holds the full event content (`topics_xdr`, `data_xdr`, `signature`,
`event_type`, `event_index`, `ledger_sequence`, `created_at`) and partitions
on `created_at`. Replace the folded `soroban_events_appearances` design (ADR
0033 / task 0157) so that `GET /v1/contracts/{id}/events` reads directly from
the DB instead of fetching + ZSTD-decompressing + XDR-parsing whole
`LedgerCloseMeta` blobs from the public S3 archive on every page.

## Context

Today `/contracts/{id}/events` performs up to 20 parallel S3 GETs per page,
ZSTD-decompresses MB-scale `LedgerCloseMeta` blobs, decodes the entire ledger
(1000+ tx) just to surface a handful of events, and pays a per-page network
round-trip to `us-east-2`. The folded appearances row carries only
`(contract_id, transaction_id, ledger_sequence, amount)` — no event content.

The v3 spike measured the trade-off on a local backfill (51 936 folded rows
→ 86 667 expanded events):

| Variant                                          |     Total | S3 fetch? |
| ------------------------------------------------ | --------: | :-------: |
| `soroban_events_appearances` (today)             |     13 MB |    yes    |
| `soroban_events` JSONB + GIN                     |     50 MB |    no     |
| `soroban_events_v2` (XDR + side-table addresses) |     51 MB |    no     |
| **`soroban_events_v3` (XDR + collapsed PK)**     | **27 MB** |  **no**   |

Projected DB-wide cost: **+17 %** versus today's folded baseline, in exchange
for sub-millisecond `/events` reads, archive-independent availability, and a
cheap `?topic_eq=…` filter via the partial `signature` btree.

## Implementation Plan

### Step 1 — Confirm schema and finalise open questions

Resolve open questions from the v3 spec before promoting to a real migration:

1. Keep `event_type` column? (100 % `Contract` in staging — drop saves ~700 kB)
2. Keep `ledger_sequence` inline or JOIN on `transactions`? (~700 kB, response-render trade-off)
3. Keep `idx_se_v3_signature` partial btree? (only if endpoint plans `?topic_eq=…`)
4. Backfill historical events from S3 vs accept "v3 from ledger N onward"?
5. Coexist with `soroban_events_appearances` during transition vs hard cutover?

These decisions belong in an ADR revision (see Step 5).

### Step 2 — Migration

Replace migration `20260507000200_soroban_events_v3` (test) with a canonical
`soroban_events` migration:

- `CREATE TABLE soroban_events (...) PARTITION BY RANGE (created_at)`
- PK `(contract_id, created_at, transaction_id, event_index)` — collapsed
  pkey trick saves ~7 MB vs separate pkey + keyset index
- `idx_se_transaction (transaction_id, created_at DESC)` — tx-detail fallback
- `idx_se_signature (signature, created_at DESC) WHERE signature IS NOT NULL`
  (conditional on Step 1 outcome)
- FK `(transaction_id, created_at) REFERENCES transactions(id, created_at) ON DELETE CASCADE`
- `CHECK (event_type BETWEEN 0 AND 2)`, `CHECK (event_index >= 0)`
- Default partition + integration with `db-partition-mgmt` lambda (task 0139)

### Step 3 — Indexer write path

Update `crates/indexer/src/handler/persist/write.rs`:

- New `insert_events` that writes to `soroban_events` per ledger transaction
- `xdr_parser::extract_single_event` already produces `topics_xdr`, `data_xdr`,
  `signature` (per spec) — wire the columns through the existing staging path
- Decide: drop `insert_events_appearances` immediately, or run dual-write
  through a deprecation window (see Step 1.5)

### Step 4 — API read path

Refactor `crates/api/src/contracts/handlers.rs::list_events`:

- Replace `state.fetcher.fetch_ledgers + ParsedLedger + expand_events` with a
  single `SELECT ... FROM soroban_events ORDER BY created_at DESC, transaction_id DESC, event_index DESC` keyset query
- Per-row XDR→ScVal→JSON via existing `scval_to_typed_json`
- Drop archive-failure cursor-rewind logic (no longer needed)

### Step 5 — ADRs and docs

- Revise ADR 0033 (or supersede with a new ADR) — the appearance-only design
  is no longer the canonical answer for this table
- Update `docs/architecture/**` per ADR 0032: schema docs, ingestion pipeline
  docs, API docs (events endpoint section)
- Regenerate `libs/api-types/src/{openapi.json,generated/}` if response shape
  changes

### Step 6 — Backfill (if Step 1 chose to backfill)

One-shot offline job that rehydrates `soroban_events` from S3 archive for
ledger ranges already indexed before this task lands. Likely a separate
spawned task — out of scope for the core refactor.

## Acceptance Criteria

- [ ] Canonical `soroban_events` table migration in place, replacing the
      `soroban_events_v3` spike migration
- [ ] Indexer writes events to `soroban_events` per ledger
- [ ] `/v1/contracts/{id}/events` reads from `soroban_events`, no S3 fetch on
      the hot path
- [ ] Decision on `soroban_events_appearances` lifecycle (drop / dual-write
      window) documented in revised ADR 0033
- [ ] Backfill strategy decided and documented (one-shot job vs accept gap)
- [ ] **Docs updated** — `docs/architecture/**` schema + endpoint sections
      updated per ADR 0032
- [ ] **API types regenerated** — `npx nx run @rumblefish/api-types:generate`
      run if response shape changes; otherwise mark `N/A — internal-only`
- [ ] ADR 0033 revised (or new ADR + 0033 marked superseded) with the new
      design and reasoning
- [ ] Workspace build, clippy, lib tests green; representative `/events`
      benchmark run shows the expected latency drop vs S3 path

## Out of Scope

- `/transactions/{hash}` heavy fields (memo, signatures, decoded operations,
  return values, envelope/result XDR) still need S3 — eliminating archive
  dependency for tx-detail is a separate, larger effort
- `soroban_invocations_appearances` analogue (task 0158) — same pattern,
  measure separately
- Backfill of historical events from S3 — likely spawned as its own task once
  the core refactor lands

## Notes

- Source spec: `/home/fishuser/Downloads/SOROBAN_EVENTS_V3.md`
- Predecessor work: ADR 0033, task 0157 (folded appearances refactor), task
  0158 (invocations analogue)
- Open questions from the spec (Step 1) should be resolved before the
  migration is written — they affect column count, index count, and the
  coexistence story
