---
prefix: Q
title: S3 archive fetch latency — measured p50/p95/p99
status: seed
spawned_from: '0242'
---

# Question

What is the measured end-to-end latency for fetching a single ledger
XDR from `aws-public-blockchain` (Stellar public archive), decompressing
zstd, and parsing it enough to extract LP-specific operation amounts?

## Sub-questions

1. What is p50, p95, p99 from each of our AWS compute regions?
2. How much of that is network (S3 GET RTT) vs decompression vs XDR
   parsing?
3. What is the ledger file size distribution (mean, p99)?
4. How does the existing E3 (`GET /transactions/:hash`) handle this in
   production? What does its latency dashboard show?
5. Is there an in-process cache today, or is every E3 request a cold
   S3 fetch?

## Why it matters

Path A in 0242 (the default proposal) hinges on this number. If a
single ledger fetch is p95 = 80 ms, a typical 20-row page hitting 8
distinct ledgers = ~640 ms. If p95 = 200 ms, it's 1.6 s. The user-
visible threshold for "feels instant" is ~300 ms, "acceptable" ~1 s,
"slow" >1.5 s. Where the number actually lands decides whether Path A
is shippable as-is or needs Path C (server-side cache + pre-fetch)
or Path B/C (DB-side extraction).

## Approach

1. Identify the existing E3 archive-fetch code path (look in
   `crates/api/src/transactions/handlers.rs::get_transaction` →
   archive client → S3 client).
2. Write a small bench harness (criterion or ad-hoc) that loops over
   N=100 random recent ledger sequences, fetches + parses, records
   latency per stage.
3. Run from each candidate compute region (us-east-1, eu-west-1, our
   actual region).
4. Compare warm vs cold (same ledger twice in a row).
5. Capture file size distribution.

## Linked notes

- (none yet)

## Outcome → feed into

- `S-recommendation.md` (when written) — Path A viability gate.
