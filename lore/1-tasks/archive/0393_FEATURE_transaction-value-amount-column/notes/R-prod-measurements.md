---
prefix: R
title: Prod ClickHouse measurements — scale, space, backfill, edge-case bounds
status: mature
---

# R — Prod measurements (as of 2026-07-15)

All figures measured against the production ClickHouse dataset. Protocol 23
(CAP-67) mainnet activation ledger = **58 762 517**. Ingested ledger range =
**50 457 424 – 63 486 212**.

## Event volume (`soroban_events`)

| Metric       | Value  |
| ------------ | ------ |
| Total events | 13.5 B |
| `transfer`   | 6.2 B  |
| `mint`       | 713 M  |
| `burn`       | 97.6 M |

The amount is present in the already-stored `data_xdr` column (decoded JSON),
so the amount backfill is a **CH-local transform, not an S3 re-parse**.

## Target table (`operation_asset_appearances`)

| Column            | Compressed                    | Bytes/row             |
| ----------------- | ----------------------------- | --------------------- |
| Whole table       | 81 GiB (213 GiB uncompressed) | —                     |
| `transaction_id`  | 71.25 GiB                     | 8.03 (incompressible) |
| `ledger_sequence` | 9.41 GiB                      | 1.06                  |
| `asset_id`        | 0.37 GiB                      | 0.04                  |

Rows ≈ **9.5 B**. `transaction_id` dominates and barely compresses — which is
why a **separate** table (re-storing it) would cost ~110 GiB net-new, versus
adding an `amount` column to this table (~35 GiB).

Measured `Int128` amount compression on an existing amount column (balances):
ratio 4.66 → **~3.43 compressed bytes/row**.

## Backfill size (pending 0383 historical backfill, folded with amount)

Net-new presence rows the token-flow backfill adds, measured (not
extrapolated), split at the P23 ledger — pre-P23 events are all net-new (classic
did not emit events before CAP-67); post-P23 net-new = the `has_soroban` share:

| Range                   | Net-new events            | Dedup (events/row) | Net-new rows |
| ----------------------- | ------------------------- | ------------------ | ------------ |
| pre-P23 (< 58 762 517)  | 3.95 B (100%)             | 4.60               | ~860 M       |
| post-P23 (≥ 58 762 517) | ~0.50 B (15.8% of 3.18 B) | 2.24               | ~225 M       |
| **Total**               | ~4.46 B                   | —                  | **~1.08 B**  |

Post-table: ~9.5 B → **~10.6 B rows**.

| Space component                                  | Estimate                                                                 |
| ------------------------------------------------ | ------------------------------------------------------------------------ |
| Backfill presence rows (existing columns)        | ~9–10 GiB                                                                |
| **`amount` column** (Int128 @ ~3.43 B × ~10.6 B) | **~34 GiB** (up to ~48 if transfer amounts compress worse than balances) |
| `transfer_count`                                 | <1 GiB                                                                   |
| Table final                                      | 81 GiB → **~115–130 GiB**                                                |

## Transaction complexity (bounds the account-view UX nuance)

Operation-count distribution:

| Type                        | single-op | multi-op | % single                                  |
| --------------------------- | --------- | -------- | ----------------------------------------- |
| classic (`has_soroban = 0`) | 3.43 B    | 727 M    | **82.5%**                                 |
| Soroban (`has_soroban = 1`) | 1.06 B    | 0        | 100% (but one op can hold many transfers) |

On classic accounts, **82.5% of transactions are single-operation** → the
transaction-total net equals the account's own delta. The divergence (net-tx ≠
own-delta) is a minority there. Network-wide (DeFi-heavy, 200 k-ledger window),
62.8% of transactions carry more than one transfer — but that is contract
activity, not typical user accounts.

## Edge-case bound: mint + burn in one transaction

The net formula's only non-obvious output (0 for a fully self-cancelling
mint→transfer→burn) requires a transaction to carry **both** a mint and a burn
(200 k-ledger window):

| Metric                               | Value                     |
| ------------------------------------ | ------------------------- |
| Transactions with both mint and burn | 27 680                    |
| Transactions with any mint or burn   | 7 875 232                 |
| Total transactions in window         | 70 627 112                |
| **Both / all transactions**          | **0.039%** (~4 in 10 000) |
| Both / mint-or-burn transactions     | 0.351%                    |

This is an upper bound — the actually-cancelling same-asset subset is smaller
still, and on classic accounts it is effectively 0%. The margin is negligible.
