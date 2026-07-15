---
id: '0383'
title: 'L2: Soroban event token-flow decode (from/to/amount + event participants)'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0359', '0391']
tags: [priority-high, effort-large, layer-indexer, soroban-events]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker (§15 roadmap B). Bundles K1-3, K1-7, K2-7, K3-4, K4-3, K4-4.'
  - date: '2026-07-14'
    status: completed
    who: karolkow
    note: >
      PR #332 merged to develop. Shipped: parse_token_event (transfer/mint/burn/
      clawback) + shared derive_token_event ingest hook + soroban-token-flow
      backfill (has_soroban-scoped, ledger-windowed, RMT-idempotent) + docs
      (xdr-parsing §5.6, indexing §5.3/§6). Presence-only (Option A); dead
      `amount` removed. Externally validated (Horizon + stellar.expert + raw XDR)
      and — post-merge — real-prod decode + surrogate parity confirmed on live CH
      (accounts.id / assets.id byte-match, L/liquidity-pool addresses correctly
      filtered). Remaining: OPS backfill RUN (now unblocked — see Completion).
      Spawned 0391 (NFT token-flow coverage audit).
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

## Completion (2026-07-14)

PR #332 merged to `develop`. CI green (Rust fmt/clippy/test/lambda build).

**Post-merge verification (this session):**

- **Real-prod decode + surrogate parity.** Ran the actual `read_events` SQL on a
  live CH window (filter cuts the classic firehose 115,478 → 11,687, ~90%), fed
  the real `soroban_events` bytes through `build_rows`/`derive_token_event`, and
  confirmed the Rust surrogates **byte-match** what prod stores: `accounts.id`
  for mint recipients + transfer parties, `assets.id` for KALE / ETH / native
  (`asset_type=0`). Verbs covered on real data: mint (Credit), transfer (native +
  Credit).
- **Address filter validated.** `is_strkey_account` (G-only) drops C (contract) /
  L (liquidity-pool) legs but keeps every account; measured **0 muxed (M) topics**
  chain-wide (muxing rides in `data.to_muxed_id`, never the topic) — no real
  account activity lost. Backfill uses the SAME `derive_token_event` as live, so
  zero parity drift.
- **Contract→None vs Credit→arm-A asymmetry confirmed correct.** Asset-page arm B
  (`soroban_invocations_appearances`) is keyed by the asset's OWN contract; it
  covers a bespoke type-3 token (asset_id == contract_id) but NOT a Credit asset
  flowing through a foreign DeFi contract — so `Credit → arm A` is genuinely
  net-new, not redundant. (assets/queries.rs:664-671.)

**Correction to an earlier note:** `operation_asset_appearances` (0359's table) IS
live on prod (~9B rows, verified 2026-07-14). The "0359 not deployed" caveat in
the Devil's-advocate section is **stale** — the asset-side backfill is unblocked.

**Design decisions — Emerged:**

1. **Backfill args removed; full auto-detected range.** Dropped `--start/--end`;
   `ledger_bounds` (cheap part-metadata min/max) + internal 5000-ledger windows.
   The window is required (the `has_soroban` semi-join would otherwise be billions
   of tx ids), but from the operator's view it "just runs on everything."
2. **Contract→None justification rewritten** to the real reason (arm-B coverage +
   asset_id==contract_id), after the first ("ambiguous with NFT") was refuted.

**Remaining (OPS, out of code scope):**

- [ ] Run `backfill-runner soroban-token-flow-backfill` on prod, then spot-check
      account + asset pages. Unblocked. This is the one open acceptance criterion.

**Spawned follow-up:** [0391](../../backlog/0391_RESEARCH_nft-token-flow-coverage-audit/README.md)
— NFT token-flow coverage audit (confirmed the account-side is already covered;
the NFT-page gap is the `nfts_pending` promotion drain, owned by 0217/0306/0309).
