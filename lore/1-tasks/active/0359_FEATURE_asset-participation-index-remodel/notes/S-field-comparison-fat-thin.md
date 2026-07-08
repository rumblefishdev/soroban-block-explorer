---
title: 'Field comparison — our FE vs THIN vs FAT vs stellar.expert (fat/thin + capture scope)'
type: synthesis
status: developing
spawned_from: notes/S-design-options.md
spawns: []
tags: ['data-model', 'design', 'adr-input', 'fat-thin', 'backfill']
links: []
history:
  - date: 2026-07-07
    status: developing
    who: karolkow
    note: >
      4-way field matrix feeding the fat/thin ADR decision + the "what to
      capture in the one backfill" call. Frontend verified THIN; stellar.expert
      (reference) is FAT. Cross-validated field provenance (op-body vs
      result/meta) against Horizon + Hubble/stellar-etl + XDR ClaimAtom.
---

# Field comparison — what to capture in the asset-participation index

> **Correction (2026-07-07, post devils-advocate — see
> [S-devils-advocate](S-devils-advocate.md)).** Three cost claims below were
> overstated and are corrected inline: (1) "adding a field later = the whole
> backfill again" is false — CH `ADD COLUMN` is metadata-only and a later
> single-column mutation hardlinks untouched columns; a new per-leg field needs
> an S3 re-parse only to SOURCE its values, not a table rewrite. (2) Tier-2
> result/meta is NOT a separate expensive parse — the `OperationResult` is
> already deserialized in the live parse (`operation.rs:100-171`), so realized
> amounts + trade atoms are marginal reads; the claimed-CB asset is in the
> same-op `LedgerEntryChanges`, not a CB-id join. (3) The real open call is
> SEQUENCING (build the fan-out now vs gate it on a frontend render spec), not
> Tier-2 cost.

## Decision this feeds

Two open ADR calls:

1. **Fat vs thin payload** (open decision #2 in [S-design-options](S-design-options.md)).
2. **What per-leg fields to capture in the one XDR-re-parse backfill** — because
   a NEW per-leg field still needs an S3 re-parse to SOURCE its values (legs are
   not in CH; S3 re-parse mandatory, see
   [S-diagnosis-calibration](S-diagnosis-calibration.md) + the CH-schema audit).
   It does NOT need a full table rewrite — CH `ADD COLUMN` is metadata-only and a
   later single-column mutation hardlinks untouched columns. So the real tradeoff
   is **one combined S3 pass now vs a second narrower S3 pass later** (cost
   amortization), weaker for fields we are unsure we need.

## The cost insight — it's not the column, it's where you read it

The expensive, one-shot thing is the XDR re-parse pass over ~6.4 B ops. Within
that pass there are two cost tiers:

- **Op-body fields — cheap.** The parser already reads the operation body. Roles,
  declared amounts, offer price, `path[]`, trustline/claimable terms cost ~zero
  extra to capture.
- **Result / meta XDR fields — cheaper than first framed.** Realized amounts
  (Horizon/Hubble silently overwrite the body amount with the result value),
  per-offer/per-hop fills (`ClaimAtom`), trade counterparty, venue live in the
  `OperationResult` — which is **already deserialized in the live parse**
  (`operation.rs:100-171` already iterates `offers`/`offers_claimed` for LP
  atoms), so these are marginal field reads, not a second S3 pass. The asset of a
  `ClaimClaimableBalance` (op body carries only `balanceId`) is in the **same-op
  `LedgerEntryChanges`** (the removed `ClaimableBalanceEntry`), also local — not a
  cross-partition CB-id join.

Provenance cross-validated against Horizon operation objects + effects, Hubble /
stellar-etl `enriched_history_operations`, and XDR `ClaimAtom` accessors.

## Reference gap

Our frontend renders a **thin tx-summary** row (`AssetTransactions.tsx`,
`AssetTransactionItem` in `dto.rs`): hash · ledger · source · op-types · status ·
time. **No per-asset amount, no role.** stellar.expert (the reference) renders a
**fat** row per asset: `sent 49.65 USDC`, `swapped X → USDC`, path hops, a price,
counterparty, and a separate **Trades** tab. StellarChain / LOBSTR show only DEX
trades / market data on the asset page.

## The matrix

Legend: ✓ present/worth it · ◐ partial · — none. Source: **BODY** (cheap) /
**RESULT** (result-meta, expensive) / **TX** (tx-level).

| Field                                                          | TL;DR                                                                    | FE now | THIN | FAT | stellar.expert | Source      | Recommendation       |
| -------------------------------------------------------------- | ------------------------------------------------------------------------ | :----: | :--: | :-: | :------------: | ----------- | -------------------- |
| **A · Transaction summary — already have (thin)**              |
| `transaction_hash`                                             | Tx hash, link                                                            |   ✓    |  ✓   |  ✓  |       ✓        | BODY        | Have                 |
| `ledger_sequence`                                              | Ledger number                                                            |   ✓    |  ✓   |  ✓  |       ✓        | BODY        | Have                 |
| `source_account`                                               | Op source account                                                        |   ✓    |  ✓   |  ✓  |       ✓        | BODY        | Have                 |
| `operation_type`                                               | Op type chip + "+N"                                                      |   ✓    |  ✓   |  ✓  |       ✓        | BODY        | Have                 |
| `successful`                                                   | Success / fail                                                           |   ✓    |  ✓   |  ✓  |       ◐        | RESULT      | Have                 |
| `created_at`                                                   | Close time (UTC)                                                         |   ✓    |  ✓   |  ✓  |       ✓        | TX          | Have                 |
| `fee_charged`                                                  | Fee charged                                                              |   ✓    |  ◐   |  ✓  |       —        | RESULT      | Have                 |
| `operation_count`                                              | Ops in tx                                                                |   ✓    |  ✓   |  ✓  |       ◐        | BODY        | Have                 |
| **B · Fan-out keys — new, required**                           |
| `asset_id` (lead key)                                          | Which asset the row is for — the core of the fix                         |   —    |  ✓   |  ✓  |       ✓        | BODY        | **Required**         |
| `role`                                                         | sent/received/sold/bought/traded/… — else a hop renders as "sent"        |   —    |  ✓   |  ✓  |       ✓        | BODY+RES    | **Required**         |
| `leg_index`                                                    | Deterministic dedup key (live == backfill) or RMT mis-counts             |   —    |  ✓   |  ✓  |       —        | BODY        | **Required**         |
| **C · Per-leg — FAT candidates, cheap from body**              |
| `amount` (per-asset)                                           | How much of THIS asset moved ("sent 49.65 USDC")                         |   —    |  —   |  ✓  |       ✓        | BODY/RES    | **Capture now**      |
| `price` / `price_r`                                            | DEX offer price                                                          |   —    |  —   |  ✓  |       ✓        | BODY        | **Capture now**      |
| `offer_id`                                                     | Offer id (submitted=body, matched=result)                                |   —    |  —   |  ✓  |       ✓        | BODY/RES    | Consider             |
| `counterparty` (`to`)                                          | Payment recipient / other side                                           |   —    |  —   |  ✓  |       ✓        | BODY/RES    | **Capture now**      |
| **D · Result / trade grain — completeness ceiling, EXPENSIVE** |
| `realized amount`                                              | Actual sent/delivered path-payment amount (overwritten from result)      |   —    |  —   |  ✓  |       ✓        | RESULT      | Consider             |
| `trade legs` (ClaimAtom)                                       | Both traded assets + amounts; unbounded N/op; only complete offer source |   —    |  —   |  ✓  |       ✓        | RESULT      | Consider             |
| `trade counterparty`                                           | Seller / base_account                                                    |   —    |  —   |  ✓  |       ✓        | RESULT      | Consider             |
| `venue` (pool/offer)                                           | Where crossed: LP pool or a specific offer                               |   —    |  —   |  ✓  |       ✓        | RESULT      | Consider             |
| **E · Special cases**                                          |
| `path hops`                                                    | Intermediate route assets. External consensus: redundant with trades     |   —    |  —   |  ◐  |       ✓        | BODY        | Consider / via trade |
| `slippage min`                                                 | Slippage bound (min … asset)                                             |   —    |  —   |  —  |       ✓        | BODY        | Skip / later         |
| `claimable asset+amount`                                       | Claimed CB asset — NOT in op body (balanceId → CB-id join)               |   —    |  —   |  ◐  |       ◐        | RES/meta    | Consider (join)      |
| `soroban call detail`                                          | fn + args + return                                                       |   —    |  —   |  —  |       ✓        | other table | Union SAC, not here  |
| `memo`                                                         | Note — tx-level, not per-leg                                             |   —    |  —   |  —  |       —        | TX          | Skip                 |

## Recommendation — 3 tiers + skip

**Tier 0 · ship now** (thin, zero re-parse): the tx summary we already have +
native surrogate + F-F SAC union close the native gap with no backfill.

**Tier 1 · capture in the backfill** (op body, ~free): `asset_id`, `role`,
`leg_index`, declared amount, `price`, `offer_id`, `to`, path hops. The parser
already reads the body → marginal cost ~zero. Future-proofs "show `sent 49.65
USDC` on the asset page" (which stellar.expert already does) without a re-backfill.

**Tier 2 · result/meta grain** (realized amounts, `ClaimAtom` trades +
counterparty + venue, claimed-CB asset): the only **complete** source for offers
AND exactly what stellar.expert's Trades tab shows. Cost is SMALL — the
`OperationResult` is already deserialized live and the claimed-CB asset is in the
same-op `LedgerEntryChanges`, so it is marginal reads, not a second S3 pass. When
Phase 2 runs, capture the trade grain in the same pass; gate the **display**
(Trades tab) on a real frontend render spec, not the capture.

**Skip / later:** `slippage min`, `memo` (tx-level), soroban call detail (arrives
via the SAC union), StellarChain-style pair/price (derive at read time).

## What to additionally display (frontend)

- **Amount + asset + role per row** ("sent 49.65 USDC") — biggest UX win, matches
  stellar.expert. Needs Tier 1.
- **A per-asset "Trades" sub-tab** — both stellar.expert and StellarChain have it.
  Needs Tier 2.
- **Counterparty (to/from) inline** — Tier 1 (`to`) / Tier 2 (trade seller).
- **Path-payment hops inline** — Tier 1 (declared) or via the trade grain.

## Archive-on-demand sharpens THIN (2026-07-08)

The transaction-DETAIL endpoint already re-parses the raw ledger XDR from the S3
archive on request (the E3 heavy-fields overlay, ADR 0029) — the app's
"details on demand" mechanism. This does **not** change the core need for the
fan-out: the archive answers _"give me tx X"_ by key, but **cannot** answer
_"which ops touched asset Y"_ without an index — that mapping exists only once we
build it, which is exactly what the backfill does. But it does two things:

1. **It confirms THIN.** The asset-page list needs only find-keys (`asset_id`,
   `role`) + a tx-summary to render a row; the heavy per-leg detail (amounts,
   route, trades) is fetched from the archive when the user drills into a
   transaction. So the fan-out stays lean.

2. **It turns per-leg AMOUNTS from a completeness requirement into a performance
   choice.** Storing `amount` in the index is needed ONLY if the asset-page LIST
   renders per-asset amounts inline ("sent 49.65 USDC", stellar.expert style) —
   you can't archive-fetch per list row (N fetches/page = slow). If the list
   stays thin (no inline amount), skip the amount columns and let drill-in fetch
   them from the archive. Adding amounts later = one more narrow archive pass
   (`ADD COLUMN` + single-column re-parse) — the amortization tradeoff, not a
   completeness gap.

**Net:** the fan-out MUST carry find-keys + a thin summary; per-leg amounts are
gated on whether the LIST shows inline amounts, not on completeness — decide the
amount columns against the asset-page list render spec, the same way fat/thin was.

## The one real call for karolkow

Corrected post devils-advocate: Tier-2 capture is cheap (result already in hand),
so the fork is NOT "op-body vs result cost". The real open call is **sequencing**:
build the full role-tagged fan-out now (the THIN frontend renders none of its
extra grain) — or ship **Phase 0** (native surrogate + F-F SAC-union, no backfill)
first, then **Phase 1** (offers by asset), and gate the **Phase 2** fan-out on an
actual frontend render spec that needs per-leg role/amount/trades. See
[S-devils-advocate](S-devils-advocate.md) for the full sequencing rationale.

Related: [[project_native_two_conventions]], [[feedback_fundamental_complete_backward_data]],
[[feedback_sources_are_interpretations]].
