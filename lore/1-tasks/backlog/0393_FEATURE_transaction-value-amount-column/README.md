---
id: '0393'
title: 'FEATURE: transaction value ("amount moved") column — net-settled per-asset value for tx-list views'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0359', '0383', '0261', '0247']
tags:
  [
    clickhouse,
    indexer,
    xdr-parser,
    frontend,
    transactions,
    effort-large,
    priority-medium,
  ]
milestone: 1
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/xdr-parser/src/event_filters.rs
  - crates/backfill-runner/src/soroban_token_flow_backfill.rs
history:
  - date: '2026-07-15'
    status: backlog
    who: karolkow
    note: >
      Task drafted from a research + design investigation. Requirement:
      surface a "value moved" figure per transaction in list views (fee is
      uninformative for locating a transaction). Definition, storage,
      source, and cost are researched below; implementation not started.
---

# FEATURE: transaction value ("amount moved") column

## Summary

Surface a **value-moved figure per (transaction, asset)** in transaction-list
views (the global transactions list and the per-account transactions list), so
a transaction can be located and scanned by amount rather than by fee. On
Stellar the fee is near-constant (~0.00001 XLM) and useless for this. The
figure is a **single aggregated value per (transaction, asset)** — a product
constraint — defined as the **net settled value** (see
[S-formula-and-edge-cases](notes/S-formula-and-edge-cases.md)).

## Context

### There is no single "transaction amount" in the protocol

A Stellar transaction is a **container of N operations**. The only scalar
money field at transaction grain is `fee_charged` (and `max_fee`); the Horizon
`Transaction` object exposes no amount/value/total. Value lives on
**operations** (13 operation types carry a money field: payment, path payment,
create account, offers, claimable balance, clawback, LP deposit/withdraw).
This is a protocol-level fact, not an oversight — the reference explorer and
the major third-party explorers all omit a per-transaction amount in their
transaction lists (they show operation count, native value, or a per-transfer
list). See [R-metric-research](notes/R-metric-research.md).

Because the product requires a **single** aggregated value anyway, this task
defines the least-wrong one.

### Value moves via three mechanisms

1. **Native XLM** — classic native asset, 7 decimals. Amount in the operation
   body / result (path-payment claim atoms).
2. **Classic issued assets** (`credit_alphanum4/12`) — issuer + trustline, 7
   decimals. Same operation-body source.
3. **Soroban tokens** — moved inside `invoke_host_function`, surfaced as
   `transfer`/`mint`/`burn`/`clawback` **events**, decimals per token metadata.
   Two flavours: **SAC** (built-in contract wrapping a classic asset incl.
   XLM) and **bespoke SEP-41** contract tokens.

Protocol 23 / CAP-67 (mainnet activation ledger **58 762 517**, ~Sept 2025)
makes classic operations **also** emit the unified event format, so from that
ledger onward a single event stream covers all three mechanisms. Before it,
classic movements are only in the operation body (events cover Soroban only).

### What exists today

- Transfer **amounts are not stored** in a queryable column. The presence
  indexes (`operation_asset_appearances`, `transaction_participants`) carry
  fold-counts, not money. The transaction-detail page parses amounts from
  archive XDR on demand.
- Two extraction paths already compute the raw amounts but **discard** them:
  - The classic operation/result path (task 0359) extracts path-payment
    claim-atom amounts (both legs) and classic operation amounts; today they
    feed LP `gross_volume_a` and the detail JSON only.
  - The Soroban token-event decoder (task 0383) decodes
    transfer/mint/burn/clawback including the CAP-67 unified asset identity,
    but explicitly does not read the amount out of `data_xdr`.
- The Soroban token-flow **historical backfill (0383) has not been run** on
  prod yet — only the live ingest hook is deployed. This is an opportunity:
  the amount backfill can run in the same CH-local pass (see below).
- Token `decimals`/`symbol` live in `soroban_contract_metadata`, populated
  from a possibly-different ledger than the transfer — so scaling must happen
  **at read time** (raw `Int128` stored, decimals joined on read), the same
  pattern already used for balances.

## Decision: net settled value

**`amount` per (transaction, asset) = `max(Σ positive account deltas, Σ negative account deltas)`.**

Per transaction and asset: sign each transfer (`from` −amount, `to` +amount),
accumulate a signed delta per account, then take the larger of the total gained
and total lost. This is the industry "net settled value" — it nets out
routing/pass-through hops automatically (a pass-through account has delta ~0),
which gross summation would double-count.

Three rules complete the definition:

1. **`max` of both sides** (not just the gained side) — required so burns and
   payments-to-issuer (one-sided, nobody gains) are counted, not shown as 0.
2. **Native XLM canonicalised to one `asset_id`** before grouping — native has
   two surrogate conventions in the codebase; split deltas would not cancel.
3. **`fee` events excluded** — the transaction fee is tracked separately; its
   native outflow must not inflate the native figure.

Rationale, the alternatives rejected (gross, `Σ+` only, `Σ|delta|`), the
edge-case stress tests (mint/burn/issuer/mixed), and the representation-
robustness argument are in
[S-formula-and-edge-cases](notes/S-formula-and-edge-cases.md). Prod
measurements that size the work and bound the edge cases are in
[R-prod-measurements](notes/R-prod-measurements.md).

## Implementation Plan

### 1. Storage — reuse the existing per-(tx, asset) index

Add to `operation_asset_appearances` (already keyed `(asset_id, ledger, tx)`):

- `amount Int128` — raw, net-settled per (tx, asset). Scaled at read.
- `transfer_count` (small int) — number of folded transfers, for the display
  "+ N others" and for debugging.

Reusing this table avoids re-storing the incompressible `transaction_id`
column (~71 GiB) that a separate table would duplicate. Net-new storage is the
`amount` column only (~35–50 GiB projected — see measurements).

### 2. Read path — projection for tx-keyed reads

The table's sort key is `asset_id`-leading; the transaction-list read fetches
by `(ledger, tx)` (a page of ~25 transaction ids), which is a scan against that
key. Add a **ClickHouse projection ordered `(ledger, transaction_id)`** so the
list read is a prefix seek. Cheaper than a full duplicate table (base data is
not re-stored). Wire the read through the existing shared list-aggregate
fetch (`fetch_tx_list_aggregates`), which already runs one keyed second query
per page for `operation_types`.

### 3. Source & indexer — compute net per (tx, asset)

Compute per-account signed deltas per (tx, asset) and reduce to the net-settled
value. Cleanest source is **ledger-entry balance changes** (before/after),
which auto-net hops; the deferred version-safe `TransactionMeta` reader
(`meta.rs`, held in `stash@{0}` off the 0359 branch) is exactly this reader and
should be revived. Bespoke Soroban token balances live in contract storage
(`ContractData`) — decode those keys too. Combine:

- classic operation/result amounts (0359 path) — covers classic across all
  history, terminal amounts (no hop inflation), and pre-CAP-67 history.
- Soroban token-event amounts (0383 path) — read the amount out of `data_xdr`
  (already stored; a **CH-local transform, no S3 re-parse** — the same pattern
  as the existing `nft_reparse` and the 0383 backfill).

The op-path/event-path split already de-duplicates classic vs Soroban (0383 is
scoped to `has_soroban`); the same split prevents double counting here.

### 4. Backfill — fold into the pending 0383 backfill

The Soroban token-flow historical backfill has not run yet. Run the amount
derivation in the **same CH-local pass**, not a second scan of billions of
events. Full history, not forward-only.

### 5. Frontend — "X ASSET + N others"

Per (tx, asset) rows aggregate into a cell: the primary asset (`150 USDC`) plus
`+ N other transfers`. Same figure on the account list and the global list (it
is a transaction-level intrinsic); only display ordering may differ.

## Acceptance Criteria

- [ ] `operation_asset_appearances` carries `amount Int128` (raw) +
      `transfer_count`, net-settled per (tx, asset) per the formula and its 3
      rules.
- [ ] Native XLM canonicalised to one `asset_id`; `fee` events excluded;
      `max(Σ+, Σ−)` used (burns/redeems non-zero).
- [ ] Amount derived CH-local from `data_xdr` + operation/result amounts; no
      S3 re-parse; folded into the 0383 historical backfill (one pass).
- [ ] Projection (or equivalent) makes the tx-list read a prefix seek, not a
      full scan on the `asset_id`-leading key.
- [ ] Read scales raw `Int128` by `decimals` from `soroban_contract_metadata`
      at read time (no baked decimals).
- [ ] Frontend cell renders "X ASSET + N others" on both the global and
      per-account transaction lists.
- [ ] **Docs updated** — `docs/architecture/**` schema + frontend-contract +
      ingestion sections updated per ADR 0032 (adds a stored amount, changes
      the tx-list data contract).
- [ ] **API types regenerated** — `crates/api/**` gains an amount field on the
      transaction-list DTO; run `npx nx run @rumblefish/api-types:generate` and
      commit `libs/api-types/src/{openapi.json,generated/}`.

## Open Decisions (product)

1. **Primary-asset selection** for "X + N others" when a transaction moves
   several assets. Without a price feed there is no "largest by value"; candidate
   rules: native-XLM-first, or by `transfer_count`. Ordering only.
2. **Display vs sort/filter.** The originating request was to _find_ by amount,
   which implies sort/filter, not just display — that requires the amount to be
   sortable/filterable (the projection helps; a genuine filter is more work).
   Decide scope.
3. **Account view semantic.** The stored figure is the transaction-total net
   (identical for both views). On complex multi-party transactions this differs
   from the viewing account's own delta. Measured to be a minority on classic
   accounts (see measurements); accepted as tx-level intrinsic. Revisit only if
   account-own-delta is wanted.

## Future Work

- USD-denominated volume (sum across assets by price) is blocked on the Prices
  API (task 0247); this task deliberately stays asset-native. When prices land,
  a USD figure is a read-time join on top of the stored raw amounts. Spawn a
  follow-up backlog task at that point.
