---
id: '0383'
title: 'L2: Soroban event token-flow decode (from/to/amount + event participants)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0359']
tags: [priority-high, effort-large, layer-indexer, soroban-events]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker (§15 roadmap B). Bundles K1-3, K1-7, K2-7, K3-4, K4-3, K4-4.'
---

# L2: Soroban event token-flow decode

## Summary

Decode the actual token movements inside Soroban contracts (transfer / mint /
burn / clawback: from, to, amount, asset) from `soroban_events` and surface them
on account + asset pages. The classic-op fan-out (0359) covers classic
operations; this is the Soroban-event side.

## Context

Spawned from 0359. Verified (0359 §16): the raw event content is **already in
CH** — `soroban_events.topics_xdr` / `data_xdr` hold ScVal-decoded JSON (9.68B
rows, prod). So this is a **CH-side transform, NOT an S3 re-parse**. The decode
for `transfer` already exists (`event_filters.rs`); mint/burn/clawback are the
same SEP-41 topic shape (one address each).

## Model decision (K1-3) — DECIDED: Option A, presence-only

Surface Soroban token moves as **pure presence**, reusing the two indexes 0359
landed. **No new table, no stored amount.** Full rationale + rejected Option B in
[notes/S-k1-3-model-decision.md](notes/S-k1-3-model-decision.md). Empirical
event shapes + code anchors in
[notes/R-soroban-event-shapes.md](notes/R-soroban-event-shapes.md).

Consequence: **no API read-side change** — account page already reads
`transaction_participants`, asset page arm A already reads
`operation_asset_appearances`. Write there → both pages light up.

## Plan

1. **Parser** (`xdr-parser/src/event_filters.rs`) — decoder for transfer / mint /
   burn / clawback returning `from?`/`to?` + asset (native / `CODE:ISSUER` /
   bespoke=contract). Verified prod shapes; TDD.
2. **Ingest hook** (`stage.rs:503-515`) — extend transfer-only loop: register
   mint/burn/clawback `from`/`to` → `transaction_participants` (K2-7); write
   event asset presence → `operation_asset_appearances` (K3-4 asset side).
3. **Backfill** (`backfill-runner soroban-token-flow-backfill`) — in-CH rebuild
   over existing `soroban_events` (no S3 re-parse), scoped to `has_soroban = true`
   via PREWHERE, windowed by ledger. **All four verbs incl. transfer** — the live
   write path is now also `has_soroban`-scoped + all-verb, so re-deriving all four
   keeps historical participant rows consistent with it; idempotent (RMT dedup).
   Asset presence: SAC classic/native only.
4. **Read side** — nothing (see decision above). K3-4 satisfied by write path.
5. **K4-3/4 hygiene** — guard/doc that `soroban_invocations_appearances.amount`
   (fold-count) is never rendered as a token value.
6. **Docs** (ADR 0032) — add "SAC-event → presence" step to the ingestion doc.

## Progress

- **Phase 1 (parser)** ✅ `parse_token_event` + `TokenEvent`/`EventAsset` in
  `xdr-parser/src/event_filters.rs`, 11 tests (real prod shapes).
- **Phase 2 (ingest)** ✅ shared `derive_token_event` in `stage.rs`; event loop
  registers all-verb participants + SAC-classic asset presence. `event_asset_id`
  - `derive_token_event` unit-tested.
- **Phase 3 (backfill)** ✅ `backfill-runner soroban-token-flow-backfill`
  (`--start --end [--dry-run]`), reuses `derive_token_event` for row parity,
  windowed scan of `soroban_events`, idempotent (RMT). 5 tests.
- **Phase 5 (amount hygiene, K4-3/4)** ✅ audit: `soroban_invocations_appearances.amount`
  is **never SELECTed** in any read path (reads use `count()` / `caller_id` / keys
  only) — the fold-count is never rendered as a token value. No code change needed.
- **Phase 6 (docs)** ✅ xdr-parsing §5.6 (`parse_token_event`), indexing-pipeline
  §5.3 (event→presence feed) + §6 (backfill pass).
- **Pending**: run backfill on prod (gated OPS step) → Phase 4 page spot-check.

## Devil's-advocate outcome (measured — see [notes/S-devils-advocate-findings.md](notes/S-devils-advocate-findings.md))

- **Applied — scope `has_soroban = true`** (ingest + backfill). Measured: 99.4% of
  transfer events are classic payments already covered by the 0359 op path
  (participant superset **proven 670/670**). Keeps net-new contract-internal flows,
  drops the redundant classic firehose. ~99% less work, zero loss.
- **Applied — removed dead `amount`** from parser/decoder. Confirmed unneeded: the
  activity lists show no amounts, the tx-detail page decodes them from archive XDR
  (E3, ADR 0029). A (presence-only) is final — same decision as 0359; B rejected.
- **Dependency (not a bug)**: asset half targets `operation_asset_appearances`,
  **not yet in prod** — 0359 not deployed. Only the asset-backfill RUN waits for
  that table (created on 0359 deploy). Code is complete; participant half unblocked.

## Acceptance Criteria

- [x] mint/burn/clawback participants registered at ingest (K2-7)
- [x] K1-3 model decided — Option A, presence-only (no new table, no amount)
- [x] K1-7 loss risk confirmed-absent — `event_index` in RMT key, no fix needed
- [x] event asset presence written to `operation_asset_appearances` (native → `NATIVE_ASSET_ID`)
- [x] backfill re-derives history from `soroban_events` (no S3 re-parse), idempotent
- [ ] backfill run on prod + account/asset pages show Soroban activity (spot-checked)
- [x] amount hygiene (K4-3/4) — audit: no code SELECTs/renders `invocations.amount` as tokens

## Docs updated (ADR 0032)

- `docs/architecture/xdr-parsing/xdr-parsing-overview.md` — updated (§5.6 `parse_token_event`)
- `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` — updated (§5.3 event→presence, §6 backfill pass)
- `docs/architecture/database-schema/**` — N/A: no new table/column (reuses `transaction_participants` + `operation_asset_appearances`)
- `docs/architecture/backend/**` (API endpoints) — N/A: no endpoint added/removed/renamed; read paths unchanged
- `docs/architecture/frontend/**` — N/A: no frontend data-contract change
