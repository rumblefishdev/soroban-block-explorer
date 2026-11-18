---
title: 'Where 64-bit integers lose digits in the browser'
type: research
status: complete
spawns: []
tags: [api, frontend, precision]
links: []
history:
  - date: '2026-11-18'
    status: complete
    who: karolkow
    note: 'Whole-project sweep after the account sequence number fix.'
---

# Where 64-bit integers lose digits in the browser

A browser's `JSON.parse` keeps integers exact only up to 2^53 − 1
(9,007,199,254,740,991); above that the last digits change. Rust and
ClickHouse hold exact values. Swept on `origin/develop` 2026-11-18 (after
#736, which made the account `sequence_number` a string).

## Digits wrong on screen today

| #   | Where (origin/develop)                                                          | Value                                                                                                                                                                         | Above 2^53 in real data                                                                                               | Covered by                                         |
| --- | ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| 1   | `crates/xdr-parser/src/operation.rs:355-534` → `humanizeOp.ts` and the raw JSON | classic operation amounts (i64): `amount`, `startingBalance`, `sendMax`, `destAmount`, `sendAmount`, `destMin`, `buyAmount`, pool `maxAmountA/B`, `minAmountA/B`, claim atoms | measured: `pool_operation_amounts.amount`, same claim atoms, 2,323,349 of 1,134,207,564 rows; max 9223371920739123801 | decision 13 ("64-bit operation-detail fields")     |
| 2   | `humanizeOp.ts:185-196`                                                         | trustline `limit` (i64); i64::MAX ("unlimited") is guessed from the rounded value `>= 9.2e18`                                                                                 | not measurable (`details` is decoded at request time)                                                                 | decision 13; the guess goes once the value is text |
| 3   | `humanizeOp.ts:440-442`                                                         | `bumpTo` (i64 sequence)                                                                                                                                                       | can be any i64 (derived)                                                                                              | decision 13                                        |
| 4   | `crates/xdr-parser/src/scval.rs:22-28`                                          | soroban `u64`, `i64`, `timepoint`, `duration`, `ledger_key_nonce` in events, call args, return values                                                                         | measured: `asset_transfers.to_muxed_id` 131,893 of 422,424 non-null values                                            | decision 13                                        |
| 5   | `humanizeOp.ts:274-284`                                                         | `offerId` (i64)                                                                                                                                                               | offer ids are a ledger counter, small today (derived)                                                                 | decision 13                                        |
| 6   | `operation.rs:681-713`                                                          | claimable-balance predicate `timePoint`, `seconds` (i64); raw JSON only                                                                                                       | can (derived)                                                                                                         | decision 13                                        |

## Possible, not ours to fix here

- NFT off-chain `metadata` (`crates/api/src/nfts/dto.rs:70`, `NftMetadata.tsx:31`): the author's JSON
  is passed through; a number above 2^53 in it would round. Not measured (17,498 NFT rows).

## Clean

- All 126 `integer` properties in `openapi.json`: ledger sequences are u32, counts are bounded by row
  counts, `fee_charged` max 5,577,830,576 (measured).
- Amounts, supplies, balances, reserves, USD values, memo ids, event ids: already strings.
- Pagination cursors: decoded only in Rust.
- `Number()` on scaled amounts for rates, compact figures and charts (`PoolActivity.tsx`,
  `amount.ts`, `PoolCharts.tsx`): rounded to about four significant digits on purpose, so no digit
  shown is wrong.

Not searched: testnet; per-value maxima inside event payloads (decoded at request time, not stored);
Rust `as` / f64 casts before serialising; `web/src/search/federation.ts`.
