---
prefix: S
title: NFT detection strategy recommendation
status: mature
spawned_from: null
spawns: []
sources:
  - ../sources/sep-0050-nft-standard.md
  - ../sources/sep-0041-token-interface.md
  - ../sources/stellar-fully-typed-contracts.md
  - ../sources/stellar-events-structure.md
  - ../sources/stellar-rpc-getevents.md
  - ../sources/openzeppelin-stellar-nft.md
  - ../sources/stellar-nft-example-contract.md
  - ../sources/stellar-rpc-getledgerentries.md
  - ../sources/stellar-rpc-simulatetransaction.md
  - ../sources/bachini-soroban-nft-tutorial.md
---

# Synthesis: NFT Detection Strategy Recommendation

## Recommended Approach: Dual Detection (WASM Spec + Event Confirmation)

### Primary: WASM Spec Analysis at Deployment Time

When the indexing pipeline detects a new contract deployment:

1. Fetch WASM bytecode via `getLedgerEntries` (two-step: instance → code)
2. Parse `contractspecv0` custom section to extract function signatures
3. Score against NFT heuristic (see R-wasm-spec-detection.md)
4. Store classification in `soroban_contracts.contract_type`:
   - `'nft'` — high confidence match
   - `'token'` — matches SEP-0041 fungible pattern
   - `'unknown'` — no match or low confidence

**Why primary:** Contract specs are deterministic and available immediately at deployment. No need to wait for runtime events. High accuracy for SEP-0050-compliant contracts.

### Secondary: Event-Time Confirmation

When processing events from `TransactionMeta`:

1. If contract already classified as `'nft'` → extract NFT data from event
2. If contract classified as `'unknown'` → check event patterns:
   - `mint` event with integer (TokenID) data → reclassify as potential NFT
   - `transfer` event with integer (TokenID) data → reclassify as potential NFT
3. Queue reclassified contracts for full WASM re-analysis

**Why secondary:** Catches contracts that:

- Were deployed before the indexer started running
- Have non-standard specs but emit standard events
- Were classified as `'unknown'` initially

### Detection Confidence Levels

| Level      | Criteria                                              | Action                                        |
| ---------- | ----------------------------------------------------- | --------------------------------------------- |
| **High**   | 3+ SEP-0050 functions with correct signatures         | Classify as `'nft'`, start indexing           |
| **Medium** | `owner_of` or `token_uri` present, non-standard types | Classify as `'nft'`, flag for review          |
| **Low**    | Event patterns match but spec doesn't                 | Store as `'unknown'`, queue for manual review |
| **None**   | Matches SEP-0041 or no token functions                | Not NFT                                       |

## Token ID Extraction

- **Standard (SEP-0050):** `TokenID` (generic unsigned int) from function parameters and event data; OpenZeppelin implementation uses `u32`
- **Non-standard:** May be `i128`, `String`, or other `ScVal` types (e.g., jamesbachini tutorial uses `i128`)
- **Storage:** Use `VARCHAR(128)` for `nfts.token_id` to accommodate variations
- **Uniqueness:** Scoped by `(contract_id, token_id)` as designed in schema

## Metadata Retrieval

SEP-0050 defines `token_uri(token_id) -> String` returning a URL to JSON metadata:

```json
{
  "name": "Token Name",
  "description": "Description",
  "image": "ipfs://...",
  "external_url": "https://...",
  "attributes": [{ "trait_type": "Color", "value": "Blue" }]
}
```

**Retrieval strategy:**

1. Call `token_uri()` via `simulateTransaction` RPC (read-only, no fees) _(Source: stellar-rpc-simulatetransaction.md)_
2. Fetch the returned URL (IPFS gateway or HTTP)
3. Parse JSON, store in `nfts.metadata` (JSONB) and `nfts.media_url`
4. Handle failures gracefully — metadata is optional per schema design

**Collection-level metadata:**

- `name()` and `symbol()` provide collection identity
- Store in `soroban_contracts.metadata` for search discoverability

## Sparse Metadata Tolerance

The architecture expects incomplete data. Handle gracefully:

| Field             | If Missing                                             |
| ----------------- | ------------------------------------------------------ |
| `name`            | Display token_id or "Unnamed NFT"                      |
| `media_url`       | Show placeholder image                                 |
| `collection_name` | Show contract_id truncated                             |
| `metadata`        | Show empty attributes list                             |
| `owner_account`   | Should always be available from `owner_of()` or events |

## False Positive Mitigation

| Risk                                                | Mitigation                                                       |
| --------------------------------------------------- | ---------------------------------------------------------------- |
| Fungible token misclassified as NFT                 | Check for `decimals()` — present in SEP-0041, absent in SEP-0050 |
| Custom contract with `owner_of` for non-NFT purpose | Require 2+ NFT indicators, not just one                          |
| Test/demo contracts on mainnet                      | No filtering — index everything, let users assess                |
| `balance` return type ambiguity (`u32` vs `i128`)   | Check exact return type in spec                                  |

## Event Indexing for Transfer History

The `GET /nfts/:id/transfers` endpoint reconstructs ownership history from `soroban_events`:

```sql
SELECT * FROM soroban_events
WHERE contract_id = :contract_id
  AND topics @> '["transfer"]'
ORDER BY ledger_sequence DESC;
```

Extract `from`, `to` from topics[1], topics[2] and `token_id` from data.

**Critical:** RPC `getEvents` only retains 7 days. The indexing pipeline must capture events from `TransactionMeta` at ingestion time for complete history.

## Summary

| Aspect                        | Recommendation                                                                    |
| ----------------------------- | --------------------------------------------------------------------------------- |
| **Primary detection**         | WASM `contractspecv0` analysis at deployment                                      |
| **Secondary detection**       | Event pattern matching at ingestion                                               |
| **Token ID type**             | `TokenID` (generic unsigned int); OZ uses `u32`, support `i128`/`String` variants |
| **Metadata**                  | `token_uri()` → fetch JSON → store in JSONB                                       |
| **Collection name**           | From `name()` contract function                                                   |
| **False positive mitigation** | Multi-signal scoring, `decimals()` exclusion                                      |
| **Transfer history**          | From `soroban_events` table, not separate storage                                 |
| **Standard to target**        | SEP-0050 (Draft) + OpenZeppelin implementation                                    |
