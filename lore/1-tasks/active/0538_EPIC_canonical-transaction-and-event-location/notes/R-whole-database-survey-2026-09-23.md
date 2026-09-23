---
prefix: R
title: Whole-database survey after task 0575 — where the space is and what can still go
status: developing
---

# R — Whole-database survey (2026-09-23, after 0575)

Production, read-only (`system.parts`, `system.parts_columns`,
`system.columns`, `system.tables`). Active parts: **976.4 GiB** of data;
free disk 661.8 GiB of 1,758.2 GiB.

## Largest tables

| table                             | GiB    | ratio | rows     |
| --------------------------------- | ------ | ----- | -------- |
| `transactions`                    | 220.19 | 1.83  | 4.22 bn  |
| `soroban_events`                  | 195.98 | 15.2  | 10.70 bn |
| `transaction_hash_index`          | 174.96 | 1.09  | 5.14 bn  |
| `operations_appearances`          | 102.79 | 4.84  | 7.03 bn  |
| `asset_transfers`                 | 43.37  | 9.5   | 5.66 bn  |
| `transaction_participants`        | 20.12  | 9.13  | 10.96 bn |
| `soroban_invocations_appearances` | 14.70  | 3.28  | 1.13 bn  |
| `operation_asset_appearances`     | 12.39  | 16    | 11.82 bn |
| `lp_operation_amounts`            | 11.62  | 5.25  | 0.99 bn  |
| `contract_transactions`           | 9.29   | 5.4   | 2.99 bn  |
| `operation_pools`                 | 6.96   | 4.11  | 0.64 bn  |

Outside `default`: `system.*` logs ~52 GiB (30-day TTL since task 0563),
`prices.*` ~108 GiB, of which backup tables `rollout_0286_bak_*` 33.26 GiB
and `price_ohlcv_*_bak` 15.27 GiB.

## Columns that do not compress (ratio ≤ 1.6, ≥ 4 GiB)

| table.column                                     | GiB    | ratio | B/row | note                                              |
| ------------------------------------------------ | ------ | ----- | ----- | ------------------------------------------------- |
| `transaction_hash_index.hash`                    | 153.86 | 1.0   | 32.13 | ORDER BY `hash`; also holds fee-bump inner hashes |
| `transactions.hash`                              | 126.22 | 1.0   | 32.13 | shown on every page — irreducible                 |
| `operations_appearances.transaction_id`          | 33.26  | 1.57  | 5.08  | step 5b                                           |
| `transactions.id`                                | 31.57  | 1.0   | 8.03  | step 7                                            |
| `transactions.source_id`                         | 24.04  | 1.31  | 6.12  | account hash surrogate                            |
| `transaction_hash_index.ledger_sequence`         | 21.10  | 1.82  | 4.41  | random order behind the hash                      |
| `soroban_invocations_appearances.transaction_id` | 8.42   | 1.0   | 8.03  | step 5c                                           |
| `operation_pools.transaction_id`                 | 4.78   | 1.0   | 8.03  | step 5c                                           |
| `lp_operation_amounts.transaction_id`            | 4.46   | 1.66  | 4.82  | step 5c                                           |

`transactions.inner_tx_hash` (29.32 GiB, ratio 4.42) is real data: 39.6% of
the transactions in 64,000,000–64,050,000 are fee-bumps.

`transaction_hash_index` has more distinct hashes than `transactions` in the
same ledgers (60,000,000–60,010,000: 3,505,976 against 2,582,749;
64,000,000–64,010,000: 4,746,630 against 3,298,127), no duplicates — it
indexes inner hashes too.

## Candidates, by saving

| #   | change                                                                                                                                                                                  | saving (_estimate_)                                   | needs a window                                                                                                                                     | notes                                                                                                                                                                                                                                                                                                        |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | `transaction_hash_index` keyed by an 8-byte hash prefix instead of the 32-byte hash, the full hash checked in `transactions`                                                            | **~120 GiB** (5.14 bn × ~11.5 B ≈ 55 GiB against 175) | yes                                                                                                                                                | both readers (`search/queries.rs:239`, `transactions/queries.rs:653`) are exact-hash `LIMIT 1` seeks; a prefix can return more than one ledger (~0.7 expected 64-bit collisions over 5 bn keys), so the reader verifies. Inner-hash lookup path to trace first. Overlaps task 0396 (`transaction_hash_dict`) |
| 2   | `operations_appearances` by position (step 5b) + drop `pool_ids` (task 0372) + codecs                                                                                                   | ~40–50 GiB                                            | yes                                                                                                                                                | largest `transaction_id` left; frees the last `t.id` join of the account / asset lists (`operation_types`). Its `application_order` is the operation position → `operation_index` (ADR 0059)                                                                                                                 |
| 3   | `transactions.id` dropped (step 7)                                                                                                                                                      | 31.57 GiB                                             | yes                                                                                                                                                | only after 2 and 4                                                                                                                                                                                                                                                                                           |
| 4   | the three small presence tables (step 5c)                                                                                                                                               | ~15–18 GiB                                            | yes, one for all three                                                                                                                             | same shape as 0575                                                                                                                                                                                                                                                                                           |
| 5   | codecs only: integer columns with no codec (`soroban_events` ledger / transaction / event positions ~22 GiB, `contract_transactions` 9.1 GiB, `transaction_hash_index.ledger_sequence`) | ~15–25 GiB                                            | **no** — `ALTER … MODIFY CODEC` is invisible to the driver's `DESCRIBE` check; existing parts rewrite on merge or `OPTIMIZE … FINAL` per partition | measure one partition locally first, as 0575 did                                                                                                                                                                                                                                                             |
| 6   | `prices` backup tables                                                                                                                                                                  | 48.53 GiB                                             | —                                                                                                                                                  | stellar-prices-api's tables; hand over, not ours to drop                                                                                                                                                                                                                                                     |
| 7   | account hash surrogates (`source_id`, `destination_id`, `to_id`, `from_id`, `caller_id`, `asset_issuer_id`; ~99 GiB together, ratio 1.3–7.3) → dense ids                                | unknown, large                                        | yes, everywhere                                                                                                                                    | research only; touches every table and the API                                                                                                                                                                                                                                                               |

Covered elsewhere: `soroban_events.topics_xdr` (155.96 GiB) and `data_xdr`
are `ZSTD(3)` at ratio 13 / 37, but hold JSON text despite their names —
task 0572 measures raw XDR instead, task 0416 storage against read-time
decode; the price service reads this JSON (`JSONExtractString`), so any
change there is also a prices-api change. System logs are on the 30-day TTL
decided in 0563.
