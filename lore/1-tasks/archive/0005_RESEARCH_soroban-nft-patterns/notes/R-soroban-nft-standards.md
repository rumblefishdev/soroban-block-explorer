---
prefix: R
title: Soroban NFT standards landscape
status: mature
spawned_from: null
spawns: []
sources:
  - ../sources/sep-0050-nft-standard.md
  - ../sources/sep-0041-token-interface.md
  - ../sources/sep-0039-classic-nft.md
  - ../sources/openzeppelin-stellar-nft.md
---

# Research: Soroban NFT Standards Landscape

## Key Findings

### SEP-0050: The Emerging Soroban NFT Standard

SEP-0050 (Draft, v0.1.0, 2025-03-10) is the first formal NFT standard for Soroban. Authored by OpenZeppelin, Boyan Barakov, and Ozgun Ozerk, it defines a `NonFungibleToken` trait modeled after ERC-721 but adapted for Soroban's architecture.

**Core trait functions:**

| Function              | Signature                                                                           | Purpose                  |
| --------------------- | ----------------------------------------------------------------------------------- | ------------------------ |
| `balance`             | `(owner: Address) -> Balance`                                                       | Number of tokens owned   |
| `owner_of`            | `(token_id: TokenID) -> Address`                                                    | Owner of specific token  |
| `transfer`            | `(from: Address, to: Address, token_id: TokenID)`                                   | Direct transfer by owner |
| `transfer_from`       | `(spender: Address, from: Address, to: Address, token_id: TokenID)`                 | Transfer via approval    |
| `approve`             | `(approver: Address, approved: Address, token_id: TokenID, live_until_ledger: u32)` | Approve single token     |
| `approve_for_all`     | `(owner: Address, operator: Address, live_until_ledger: u32)`                       | Approve all tokens       |
| `get_approved`        | `(token_id: TokenID) -> Option<Address>`                                            | Get approved address     |
| `is_approved_for_all` | `(owner: Address, operator: Address) -> bool`                                       | Check operator approval  |

**Metadata functions:**

| Function    | Signature                       | Purpose              |
| ----------- | ------------------------------- | -------------------- |
| `name`      | `() -> String`                  | Collection name      |
| `symbol`    | `() -> String`                  | Collection symbol    |
| `token_uri` | `(token_id: TokenID) -> String` | URI to metadata JSON |

**Key differences from ERC-721:**

- Includes `transfer()` for direct owner transfers (ERC-721 lacks this)
- Excludes `safeTransferFrom` (no receiver hooks in Soroban)
- Approval has `live_until_ledger` parameter (time-bounded approvals)
- Token IDs are `TokenID` (generic unsigned integer — SEP-0050 leaves the concrete type to the implementor; OpenZeppelin uses `u32`, community contracts may use `i128`)

### SEP-0041: Fungible Token Interface (Contrast)

SEP-0041 defines the fungible token standard. Key differences useful for detection:

| Feature           | SEP-0041 (Fungible) | SEP-0050 (NFT)                                  |
| ----------------- | ------------------- | ----------------------------------------------- |
| Amount type       | `i128`              | N/A (uses `token_id: TokenID`)                  |
| `decimals()`      | Yes                 | No                                              |
| `owner_of()`      | No                  | Yes                                             |
| `token_uri()`     | No                  | Yes                                             |
| `balance` returns | `i128` (amount)     | `Balance` (count; same generic type as TokenID) |

### SEP-0039: Classic Stellar NFTs (Legacy)

SEP-0039 defines NFTs using native Stellar assets (not smart contracts). Relevant because the explorer may encounter both types:

- Uses `ManageData` operations to store IPFS hashes
- Metadata via `stellar.toml` (SEP-1)
- Account freezing for immutability
- Fundamentally different detection approach than Soroban NFTs

### OpenZeppelin Reference Implementation

OpenZeppelin provides the canonical implementation via `stellar-contracts` (experimental):

- **Base** — standard ownership tracking
- **Consecutive** — optimized batch minting (stores ownership only at boundaries)
- **Enumerable** — enables on-chain token enumeration

Extensions: Burnable, Royalties (ERC-2981 equivalent).

Uses `sequential_mint()` for auto-incrementing `u32` token IDs.

## Implications for Block Explorer

1. **SEP-0050 is the primary detection target** — its function signatures and events are well-defined
2. **Non-standard contracts will exist** — the ecosystem is nascent, expect variations
3. **Classic NFTs (SEP-0039) are a separate concern** — different detection path entirely
4. **Token IDs are `TokenID`** (generic unsigned int) in the standard; OpenZeppelin uses `u32`, but community contracts may use `i128`
