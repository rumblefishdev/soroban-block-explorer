# Codec trial on integer columns — 2026-10-06

## Method

Production columns without a codec, largest first (`system.columns`,
`compression_codec = ''`). The candidate list of 2026-09-23 is stale:
`contract_transactions` and `transaction_hash_index` are gone (0575, 0580);
`transaction_hash_prefix_index.hash_prefix` (39 GiB, ratio 1.0) is random
and no codec helps it.

Sample: the first 25,000 ledgers of partitions 101, 115 and 128 (three eras),
every row in that range, exported from production in 2,500-ledger chunks
(`FORMAT Native`; one export longer than ~30 s is cut by the read-only
profile's execution cap). Row counts equal production for all 12
table × era samples. Loaded into a local `clickhouse-server:26.3` table per
sample, sorted by the production sort key, each column repeated once per
codec, then `OPTIMIZE FINAL`.

Representativeness: the sample's LZ4 ratio is within ~10 % of the production
ratio of the same partition for every column checked (e.g.
`soroban_events.ledger_sequence` 13.9 / 11.2 / 22.9 vs 14.3 / 11.1 / 24.7).

## Size per codec, % of LZ4 (pooled over the three eras)

| Column                                   | LZ4 ratio | ZSTD(1) | ZSTD(3) | Delta, ZSTD(1) | T64, ZSTD(1) |
| ---------------------------------------- | --------- | ------- | ------- | -------------- | ------------ |
| `soroban_events.ledger_sequence`         | 15.3      | 27      | 29      | **20**         | 41           |
| `soroban_events.transaction_index`       | 5.9       | 48      | 50      | 48             | 50           |
| `soroban_events.application_order`       | 4.2       | 74      | 69      | **58**         | 79           |
| `soroban_events.event_index`             | 11.5      | 44      | 44      | **38**         | 46           |
| `soroban_events.operation_index`         | 17.3      | 51      | 50      | 54             | 98           |
| `soroban_events.contract_id`             | 191       | **27**  | 27      | —              | —            |
| `contract_activity.ledger_sequence`      | 4.8       | 30      | 35      | **20**         | 32           |
| `contract_activity.application_order`    | 1.6       | 73      | 69      | 69             | 65           |
| `contract_activity.invocation_count`     | 12.1      | **34**  | 34      | 44             | 32           |
| `contract_activity.caller_id`            | 4.8       | 95      | **71**  | —              | —            |
| `contract_activity.caller_contract_id`   | 29.9      | **43**  | 42      | —              | —            |
| `transactions.source_id`                 | 1.3       | 95      | **87**  | —              | —            |
| `transactions.fee_charged`               | 6.6       | 49      | **43**  | 69             | 52           |
| `transactions.operation_count`           | 4.2       | **48**  | 47      | 55             | 57           |
| `transactions.application_order`         | 26.8      | 72      | 71      | **25**         | 149          |
| `transactions.ledger_sequence`           | 134       | 32      | 31      | 34             | 110          |
| `transaction_operations.destination_id`  | 2.7       | 80      | **78**  | —              | —            |
| `transaction_operations.source_id`       | 3.7       | 69      | **67**  | —              | —            |
| `transaction_operations.asset_issuer_id` | 8.5       | **47**  | 46      | —              | —            |
| `transaction_operations.contract_id`     | 62.7      | **41**  | 41      | —              | —            |

Already coded today, measured on the same sample:
`transaction_operations.application_order` (`T64, ZSTD(1)`) is 74 % of LZ4
where `Delta, ZSTD(1)` is 35 %; `operation_index` (`T64, ZSTD(1)`) 65 % vs
50 % for `ZSTD(1)`. `T64` alone over LZ4 is larger than LZ4 for both.

## Read cost

Single thread, no uncompressed cache, best of three, 24.7M / 8.1M / 12.7M
rows: `soroban_events.ledger_sequence` LZ4 0.047 s, `Delta, ZSTD(1)` 0.172 s;
`event_index` 0.040 → 0.076 s; `transactions.source_id` 0.028 → 0.041 s
(ZSTD(3) 0.037 s); `destination_id` 0.046 → 0.061 s.

## Rewrite mechanics on 26.3

- `ALTER TABLE … MODIFY COLUMN <col> <type> CODEC(…)` is accepted on a
  sort-key column and changes only metadata; new parts use the codec.
- `ALTER TABLE … REWRITE PARTS [IN PARTITION p]` recompresses existing parts.
  It rewrites the WHOLE part (every column file gets a new inode), so the I/O
  is the partition's full size, not the changed columns'. Temporary space:
  one part at a time; the largest today is 9.3 GiB (`transactions`).
- Production disk 2026-10-06: 757.6 GiB free of 1.72 TiB; database 877 GiB.
