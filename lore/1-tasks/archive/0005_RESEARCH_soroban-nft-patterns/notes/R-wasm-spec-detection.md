---
prefix: R
title: WASM contract spec analysis for NFT detection
status: mature
spawned_from: null
spawns: []
sources:
  - ../sources/stellar-fully-typed-contracts.md
  - ../sources/stellar-nft-example-contract.md
  - ../sources/sep-0050-nft-standard.md
  - ../sources/sep-0041-token-interface.md
  - ../sources/stellar-rpc-getledgerentries.md
  - ../sources/bachini-soroban-nft-tutorial.md
---

# Research: WASM Contract Spec Analysis for NFT Detection

## How Soroban Contract Specs Work

Soroban WASM binaries embed a machine-readable interface description in a WASM custom section (technical name: `contractspecv0`). This is analogous to Ethereum ABIs but stored on-chain from deployment. _(Source: stellar-fully-typed-contracts.md confirms the "WASM custom section" mechanism; the `contractspecv0` identifier comes from Soroban SDK internals.)_

The spec contains serialized `SCSpecEntry` records — one for every exported function, struct, and union. An optional metadata section (technical name: `contractmetav0`) holds metadata such as name, version, and author.

**Extraction methods:**

- CLI: `stellar contract bindings typescript --network <net> --id <contract_id> --output-dir ./out` (generates typed client from live contract spec)
- Programmatic: Parse WASM binary, extract `contractspecv0` custom section, deserialize XDR `SCSpecEntry` stream

**JSON spec format for a function** _(illustrative — exact shape depends on SDK version):_

```json
{
  "type": "function",
  "name": "owner_of",
  "inputs": [{ "name": "token_id", "value": { "type": "u32" } }],
  "outputs": [{ "type": "address" }]
}
```

## Retrieving WASM from Chain

Two-step process via the Soroban RPC `getLedgerEntries` method _(Source: stellar-rpc-getledgerentries.md):_

1. Fetch contract instance with `LedgerKey.contractData` using `SCV_LEDGER_KEY_CONTRACT_INSTANCE` to get the WASM hash
2. Fetch WASM bytecode with `LedgerKey.contractCode` using that hash

This can be done at indexing time when a new contract deployment is detected.

## NFT Detection Heuristic via Spec

Parse the contract spec and check for characteristic function signatures:

### Strong NFT Indicators (any 2+ = high confidence)

| Function                                   | Signature Pattern                         | Weight |
| ------------------------------------------ | ----------------------------------------- | ------ |
| `owner_of`                                 | `(token_id: TokenID) -> Address`          | High   |
| `token_uri`                                | `(token_id: TokenID) -> String`           | High   |
| `balance` / `balance_of`                   | `(owner: Address) -> Balance`             | Medium |
| `approve`                                  | `(... token_id: TokenID ...)`             | Medium |
| `transfer`                                 | `(... token_id: TokenID ...)`             | Medium |
| `set_approval_for_all` / `approve_for_all` | `(owner: Address, operator: Address ...)` | High   |

### Strong Fungible Indicators (exclude NFT classification)

| Function    | Signature Pattern                           | Meaning                                                     |
| ----------- | ------------------------------------------- | ----------------------------------------------------------- |
| `decimals`  | `() -> u32`                                 | Fungible token (SEP-0041) — definitively absent in SEP-0050 |
| `balance`   | `(owner: Address) -> i128`                  | Fungible amount (i128), not NFT count                       |
| `allowance` | `(from: Address, spender: Address) -> i128` | Fungible allowance pattern                                  |

### Decision Logic

```
IF has owner_of(TokenID) -> Address:
    → NFT (high confidence)
ELIF has token_uri(TokenID) -> String:
    → NFT (high confidence)
ELIF has balance(Address) -> Balance AND has transfer(..., token_id: TokenID):
    → NFT (medium confidence)
ELIF has decimals() AND has balance(Address) -> i128:
    → Fungible token (SEP-0041)
ELSE:
    → Unknown / other contract type
```

> **Note on TokenID matching:** In practice, check for integer-typed `token_id` parameter (u32 or i128 — not `amount`). The key discriminator vs SEP-0041 is the presence of `owner_of` / `token_uri` and absence of `decimals`.

## Non-Standard NFT Contracts

Community implementations may diverge from SEP-0050:

- Token IDs as `i128` instead of `u32` (seen in jamesbachini tutorial)
- Different function names (e.g., `token_image()` instead of `token_uri()`)
- Missing functions (no `approve_for_all`, no metadata functions)

The heuristic should score contracts on a confidence scale rather than binary classification:

- **High confidence:** Matches 3+ SEP-0050 functions with correct signatures
- **Medium confidence:** Has `owner_of` or `token_uri` but with non-standard types
- **Low confidence:** Has transfer-with-ID pattern but lacks ownership functions
- **Not NFT:** Matches SEP-0041 fungible pattern or has no token-related functions

## Pipeline Integration

The WASM spec analysis should run:

1. **At deployment detection** — when a new `contractCode` ledger entry appears
2. **Results cached** — store classification in `soroban_contracts.contract_type`
3. **Re-classify on upgrade** — if contract WASM is updated, re-run detection
