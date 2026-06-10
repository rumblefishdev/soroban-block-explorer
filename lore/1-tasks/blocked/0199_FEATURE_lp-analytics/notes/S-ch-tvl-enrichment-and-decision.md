---
title: 'S: LP analytics on ClickHouse — TVL-only decision + Prices-API contract'
type: synthesis
status: seed
spawned_from: ../README.md
spawns: []
tags: [clickhouse, enrichment, lp, prices-api, tvl, decision]
links: []
history:
  - date: '2026-06-09'
    status: seed
    who: stkrolikiewicz
    note: >
      Records the daily decision (ship TVL only; defer volume/fee) and the
      confirmed Prices-API contract. Recreated from session chat after a fresh
      re-clone wiped the earlier uncommitted notes.
---

# LP analytics on ClickHouse — TVL-only decision + Prices-API contract

## Scope decision — 2026-06-09 daily: ship TVL only

Ship **TVL** now; **drop `volume` + `fee_revenue`** from the chart/detail and the
endpoint contract until the `gross_volume_a` backfill is feasible.

- TVL is derivable from the Prices API: `reserve_a·price_a + reserve_b·price_b`
  (reserves already stored, price from the API). No heavy backfill.
- `volume`/`fee_revenue` need `gross_volume_a` (per-pool swap volume) which is
  **on-chain**, not on the Prices API, and the historical XDR re-parse over the
  full snapshot range is too long to block launch.

**Consequence for launch:** no `gross_volume_a`, no Phase 1 `claimedOffers`
extractor, no XDR re-parse; task 0247 deferred (not launch-blocking). Remaining
launch scope collapses to: **price-sync job → `prices` table → TVL at read.**

## Implementation (Variant B — compute at read)

![TVL enrichment on ClickHouse](./G-lp-tvl-flow.svg)

Each piece is a **single writer**:

1. **Indexer** → `liquidity_pool_snapshots`: `reserve_a/b`, `total_shares`
   (on-chain, price-independent). Only writer of this table.
2. **Price-sync job** → local `prices(asset, time_bucket) → usd`. The only
   Prices-API consumer; per-asset OHLCV, never per-snapshot.
3. **Read path** → `chart`/`detail` `JOIN snapshots × prices`
   (ledger → `closed_at` → candle) and compute TVL in SQL.

Why compute-at-read and not a write-back lambda: `liquidity_pool_snapshots` is
`ReplacingMergeTree` with no version column, so a per-snapshot `UPDATE` is a racy
read-modify-write of the whole row (a later plain insert can silently erase the
analytics). Writing only the `prices` table keeps the snapshot table
single-writer — no race. This applies equally if/when volume/fee return.

## Prices-API contract — confirmed (2026-06-08/09)

Sources: `prices-api-design-after-2nd-review.md` (rumblefishdev/stellar-scf-submissions) + Oskar.

| Need               | Answer                                                                                             |
| ------------------ | -------------------------------------------------------------------------------------------------- |
| History depth      | 1h/1d candles backfill to **2024-02-20** — covers our whole range                                  |
| Per-asset pull     | `GET /assets/{id}/ohlcv?timeframe=all` (1d) or `start`/`end`; 100 req/s; no cursor paging on OHLCV |
| Denomination price | per-candle **`vwap`** (or `close`)                                                                 |
| Asset identifier   | `{code}:{issuer}` / `{contract}` / `native` — matches our keys                                     |
| No price for asset | returns **`null`** (distinct from 5xx → write NULL vs retry)                                       |

→ For TVL the Prices API covers live + the full 273M-snapshot history. **No second API.**

Backfill pattern (TVL): pull per-asset OHLCV series once, cache in `prices`, join
locally against snapshots — never 273M per-snapshot API calls.

## Deferred — volume / fee_revenue (gated on 0247)

`gross_volume_a` = per-pool swap volume from PathPayment `claimedOffers[].amount_sold`.

- **Not** on the Prices API (that is per-asset, cross-venue) and **not**
  reconstructable from stored reserves (reserve-delta nets opposite swaps in a
  ledger — rejected in 0199 §Per-op volume).
- **Live (future):** cheap — indexer extracts at parse time (0199 Phase 1) and
  writes a new `gross_volume_a` column at insert. Schema ADD required.
- **Historical:** re-parse the ledger XDR (273M snapshots back to 2024-02-20) —
  a re-backfill. Heavy. Under research in **0247** (paths: A = on-demand XDR
  fetch + new extractor; C = ingest-side; E = CH reserve-delta via `lagInFrame`,
  exact only for single-LP-op-per-ledger — gated on collision rate).

## Provenance (boss question, 2026-06-09)

The customer RFP (`RFP 4: Soroban-first Block Explorer`) does **not** mention LP
analytics — it asks for human-readable transactions, CAP-67 events, per-tx swap
identification, and transaction history from 2024. TVL/volume/fee + the chart
endpoint come from **our own submission** (`soroban-first-block-explorer-after-review.md`),
i.e. self-imposed scope, not a customer requirement. Not an LLM invention. The
submission's own risk section permits "launching with recent history if backfill
is not complete" and "building [LP] pages last" — which is exactly what the
TVL-only + defer-volume decision does.

## Cross-links

0211 (price exposure), 0231 (CH SEP-1/NFT enrichment), 0247 (LP per-tx amounts /
gross_volume_a source), ADR 0043 (field-allocation rule — amendment for
compute-at-read), ADR 0047 (CH primary read store).
