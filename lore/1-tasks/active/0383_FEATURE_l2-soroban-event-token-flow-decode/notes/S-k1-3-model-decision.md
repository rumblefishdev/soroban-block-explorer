---
prefix: S
title: 'K1-3 model decision — presence-only, reuse existing indexes (Option A)'
status: mature
spawned_from: '0383'
date: 2026-07-13
who: karolkow
---

# S — Decode model: presence-only (Option A). DECIDED.

## Decision

Decoded Soroban token movements are surfaced as **pure presence**, reusing the
two indexes 0359 already landed. **No new table, no stored amount.**

- account page ← register `from`/`to` into `transaction_participants`
- asset page ← write asset presence into `operation_asset_appearances`
  (asset_id from the event's trailing SEP-11 string; `native`→`NATIVE_ASSET_ID`)

## Why A over B (new `soroban_token_transfers` table)

1. **Consistency with 0359.** 0359 deliberately reverted a role/`leg_index`/
   amount design (soft-reset + stash) and landed a minimal presence index,
   because an activity **list** is per-tx-deduped — a single stored per-tx amount
   is meaningless there. 0383 is the Soroban-event side of the _same_ activity
   model; it should match, not diverge.
2. **Zero read-side change.** Account read already sources
   `transaction_participants`; asset read arm A already sources
   `operation_asset_appearances`. Write there → both pages light up, no API diff.
3. **Diff size.** A = 1 parser + 1 ingest hook + 1 backfill. B = new ~1B+ row
   table + emitter + backfill + new read arms on both pages + amount hygiene
   surface. B's only payoff (per-move amount display, USD/volume analytics) has
   no concrete consumer yet.

## Why amount is not needed anywhere (confirmed 2026-07-13)

Not "deferred" — genuinely unneeded on these surfaces:

- **Activity lists** (account-detail + asset-detail) show only tx-header rows +
  op-type tags — no amounts (verified: `accounts/queries.rs`, `assets/queries.rs`
  page SELECTs; list path is "archive-free").
- **Tx-detail** decodes the full tx (incl. transfer/mint/burn amounts) from
  **archive XDR** at read time (E3, ADR 0029). Amount already exists there.

So the parser's `amount` field was dead and was **removed** (not kept "for a
future B"). Per-move amount/USD _analytics_ (e.g. 0199) is a different surface
with its own fact table if/when needed — additive, not blocked by A.

## Scope confirmations baked into A

- **K1-7**: resolved, no key fix (see [[R-soroban-event-shapes]]).
- **Backfill covers all four verbs, both targets** (final call). Transfer
  participants mostly exist already (ingest ran since day one), but re-deriving
  them is idempotent (RMT dedup) and guarantees completeness regardless of when
  each hook landed — cheaper to reason about than a verb-conditional pass. Asset
  presence never existed for any verb, so it genuinely needs all four. One
  uniform scan → `transaction_participants` + `operation_asset_appearances`.
- **K4-3/4**: `soroban_invocations_appearances.amount` stays a fold-count; ensure
  no read path renders it as tokens. Guard/doc only.
- **Backfill home**: `backfill-runner` (this is a pure in-CH rebuild, not
  enrichment → the wasm-upgrade precedent applies, not the "enrichment = new
  crate" rule).

## Emerged during Phase 2 (ingest wiring)

NFT events share the `events` stream with fungible ones (both are contract
events → both land in `soroban_events`). An NFT `[transfer, from, to]` has no
SEP-11 asset string → decodes to `EventAsset::Contract`. Two scoping calls made:

- **Participants (`transaction_participants`)**: register `from`/`to` for **all
  four verbs regardless of fungible/NFT**. Correct either way — those accounts
  _are_ tx participants. (transfer participants were already registered pre-0383;
  this only adds mint/burn/clawback, exactly K2-7.)
- **Asset presence (`operation_asset_appearances`)**: **`Native` + `Credit`
  written, `Contract` (bespoke) skipped.** Measured split of Soroban-context
  token events: **Credit 98.9% / Native 0.7% / Contract 0.4%** — so we keep 99.6%
  of the Soroban asset-flow. The "Soroban = bespoke contracts" intuition is wrong:
  Soroban activity is overwhelmingly **classic assets (USDC/KALE/BLND…) moving
  _through_ contracts** (DeFi), which carry `"CODE:ISSUER"` → `Credit`. Pure
  Soroban-native tokens (no classic issuer → no SEP-11 string → `Contract`) are
  the 0.4% margin.

  **Why `Contract → None` is correct, not a scope-plaster (verified, /devils-advocate):**
  a bespoke type-3 token's asset page is already served by **arm B**
  (`soroban_invocations_appearances`, keyed by `contract_id`). For a type-3 token
  `asset_id == contract_id` (**4172/4172**), and a contract that emits an event
  was invoked, so its `(contract, tx)` is already in arm B under that **same key**
  (**0 absent in 8362** bespoke pairs). Bespoke fungible tokens all have an
  `assets` row (**100/100**), so their page exists. Writing arm A for bespoke
  would be a pure duplicate. The earlier "ambiguous with NFT" justification was
  wrong (bespoke data is 93%+ clearly `i128` fungible, and the WASM classifier
  `soroban_contracts.contract_type` distinguishes NFT anyway).

  **Caveat (documented dependency):** this couples bespoke asset-page coverage to
  the invocation graph's completeness — no arm-A backstop if it regresses
  (auth-tree fallback has a ~53% gap when diagnostics are absent; 0 gaps today).
  A consistency/robustness alternative (write bespoke-**fungible** to arm A via
  `contract_type`) was considered and rejected as display-redundant + needing a
  hot-path classification lookup (`contract_type` is not prefetched for general
  fungible contracts at ingest). NFT coverage tracked as a separate follow-up.
