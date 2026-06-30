---
prefix: R
title: Where actual LP amounts live in XDR — op body vs result-meta
status: finding
spawned_from: '0247'
---

# Finding: actual LP amounts are NOT in the operation body

## What was checked

- `crates/api/src/runtime_enrichment/stellar_archive/` — the E3 read-time
  archive fetcher (`StellarArchiveFetcher`) + `extractors.rs`. Working,
  tested, used by `GET /transactions/:hash`.
- `crates/xdr-parser/src/operation.rs` — what `extract_operations` puts in
  per-op `details` JSON for LP ops.
- `crates/xdr-parser/src/ledger_entry_changes.rs` — LP entry parsing.
- `crates/api/src/liquidity_pools/handlers.rs::list_pool_transactions` +
  `PoolTransactionItem` DTO.

## Key result — Path A is NOT "free"

The original 0247 framing assumed Path A is a cheap reuse of E3: fetch
ledger XDR, call the existing extractor, read amounts out. **False.**

`extract_operations` LP-op `details` carry only the caller's **request
bounds**, not the executed amounts:

- `LiquidityPoolDeposit` → `maxAmountA`, `maxAmountB` (operation.rs:288-289)
- `LiquidityPoolWithdraw` → `minAmountA`, `minAmountB` (operation.rs:299-300)

Figma needs the **actual filled amounts** (`5,000 XLM + 2,000 USDC`), which
differ from the max/min the user submitted. The executed amounts only exist
in the transaction **result meta** — specifically the pre/post
`LiquidityPool` reserve deltas in `TransactionMeta` `LedgerEntryChanges`.
`ledger_entry_changes.rs:436` already parses `reserve_a` / `reserve_b` from
a `LiquidityPoolEntry`; the actual deposit/withdraw amount = post − pre on
the op's `changes` (State → Updated) for that pool entry.

Pool **trades** (path-payment / offer routed through a pool) are worse: the
in/out amounts come from the **operation result** (`OperationResult`), not
the op body either. Need to confirm `xdr-parser` exposes op results (it may
not today — possible new extractor surface).

## Consequence for path selection

Computing real amounts requires a **result-meta LedgerEntryChanges diff**
regardless of WHERE it runs. That parse is the same work whether done:

- at **read time** (Path A) — per request, per LP-op, on the hot path, OR
- at **ingest time** (Path C) — once, during the indexer's existing XDR pass.

So the A-vs-C decision is **not** "S3 latency vs DB latency". It is "run the
same parse once at ingest, or repeatedly at read time". Path C amortizes a
parse that Path A pays on every page view of a popular pool.

This reframes the whole task — see `S-recommendation.md`.

## Still open (needs the benchmark)

- Per-request CPU of the result-meta diff (decompress + full TransactionMeta
  walk) on a 20-row page touching ~8 ledgers — heavier than the E3
  single-tx extract that the latency note assumed.
- Whether `xdr-parser` exposes `OperationResult` for trade direction, or
  that's a new extractor.
