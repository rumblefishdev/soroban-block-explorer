---
title: 'Event payload storage: measurements and the typed-tables idea (2026-11-17)'
type: research
status: developing
spawns: []
tags: [clickhouse, storage, events]
links: []
history:
  - date: '2026-11-17'
    status: developing
    who: karolkow
    note: 'Measured while deciding the value format for 0363; the payload stays JSON for now.'
---

# Event payload storage: measurements and the typed-tables idea (2026-11-17)

Decision for now (0363): the payload stays JSON, rewritten in the official
`stellar-xdr` value format. This note keeps what was measured, for this task.

## Size

- `topics_xdr` 163.8 GiB + `data_xdr` 17.7 GiB compressed, 11.53 B rows
  (measured, `system.parts_columns`).
- As raw binary XDR: 12–28% smaller compressed (_estimate_: two samples, 400 k
  and 640 k rows, XDR rebuilt from the stored JSON; one 64 KiB / 1 MiB block
  bracket). As base64 XDR: 7–20% smaller.

## Readers of the JSON

- The price service (`prices_writer`) runs a weekly sweep with
  `JSONExtractString(topics_xdr, …)` / `JSONExtractArrayRaw` (8 runs
  2026-10-19..11-16, `system.query_log`); grant in
  `crates/db-clickhouse/users.d/services.xml`.
- Operator SQL: 308 of 807 `dev_read` queries on `soroban_events` touch the
  JSON columns; `balance_seed.rs` extracts addresses by regex; the 0517 and
  0374 re-derivations read the payload in SQL.

## Costs of raw XDR found

- The stored JSON is lossy (`<invalid-utf8>`: 1 row in 450,861,344 in partition
  130; error codes dropped), so an exact migration is a re-parse of ~11.7 B
  events from S3, not a conversion.
- An API without a protocol bump cannot decode new ScVal arms: on every XDR
  bump the API deploys before the indexer.
- 0 of 31 past backfill clusters were caused by a value-format change.

## Idea to test: standard events only in typed tables

On ledgers 65,000,000–65,020,000: `transfer` 45%, `fee` 38.2%, `mint` 12.1%,
`burn` 4%, other 0.7% (measured, `signature`). Transfer, burn and clawback
counts equal `asset_transfers`; the 2 extra `mint` rows are non-SEP-41 shapes
(`mint_amount: u128`). If every standard event can be rebuilt byte for byte
from `asset_transfers` (+ the fee payer, 0549), the payload could be kept only
for the ~0.7% other events — up to ~170 GiB less (_estimate_). Cheapest test:
rebuild one slice from the typed tables and compare with the stored payload.
