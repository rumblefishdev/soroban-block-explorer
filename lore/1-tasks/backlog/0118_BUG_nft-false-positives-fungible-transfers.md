---
id: '0118'
title: 'BUG: NFT false positives from fungible token transfers'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0026', '0027']
tags: [priority-high, effort-small, layer-indexer, audit-F9]
milestone: 1
links:
  - crates/xdr-parser/src/nft.rs
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-04-10'
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from pipeline audit finding F9 (HIGH severity).'
---

# BUG: NFT false positives from fungible token transfers

## Summary

`looks_like_token_id()` in `nft.rs:171-174` accepts `i128` data, which is the standard
SEP-0041 fungible token transfer amount type. Every fungible token transfer (USDC, XLM
wrapping, etc.) creates a spurious record in the `nfts` table.

## Context

SEP-0041 fungible token transfers use the same topic pattern as NFT transfers:
`["transfer", Address(from), Address(to)]` with `i128` amount as data. The current filter
excludes `void`, `map`, `vec`, `error` — but not numeric types like `i128`, `i64`, `u128`.

At mainnet scale this will flood the `nfts` table with millions of false-positive records.

## Implementation

**Caution:** Some NFT contracts use `i128` as token IDs. A blanket numeric exclusion would
cause false negatives. The fix must distinguish between fungible amounts and NFT token IDs.

Approaches (in order of reliability):

1. **WASM spec analysis** (best): Use `wasm_interface_metadata` to check if the emitting
   contract implements NFT-specific functions (e.g., `token_uri`, `owner_of`) vs SEP-0041
   fungible functions only. Only insert into `nfts` if confirmed NFT contract.
2. **Heuristic refinement** (fallback): Exclude `i128`/`u128` by default but whitelist
   contracts known to use numeric token IDs (requires a classification pass).
3. **Simple numeric exclusion** (quick fix): Exclude all numeric ScVal types. Accepts some
   false negatives for NFT contracts using numeric IDs.

Add regression tests with i128 data simulating a standard SEP-0041 transfer.

## Acceptance Criteria

- [ ] `looks_like_token_id()` rejects numeric ScVal types (i128, u128, etc.)
- [ ] Existing NFT detection tests still pass
- [ ] New test: SEP-0041 fungible transfer event does NOT produce an NFT record
- [ ] New test: NFT transfer with string/bytes token_id still detected correctly
