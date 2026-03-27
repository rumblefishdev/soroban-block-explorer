---
prefix: G
title: Known Soroban NFT contracts for testing
status: developing
spawned_from: null
spawns: []
sources:
  - ../sources/bachini-soroban-nft-tutorial.md
---

# Generation: Known Soroban NFT Contracts for Testing

## Status

The Soroban NFT ecosystem is nascent. At time of research (2026-03-26), only one publicly documented mainnet NFT contract was found.

## Known Contracts

### jamesbachini/Soroban-NFT

Community implementation, not production-grade (author explicitly warns: "Not Tested, Not Audited, Not Safe For Production").

| Network     | Contract ID                                                |
| ----------- | ---------------------------------------------------------- |
| **Mainnet** | `CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY` |
| **Testnet** | `CCOMN26TIW2LJW4A7NWY5UF47X6HNYAF44ZFA2GIIGCSMBEIWVIH2DMA` |

**Source:** https://jamesbachini.com/soroban-nft/

**Implementation notes:**

- Token IDs use `i128` (not `u32`) — non-standard vs SEP-0050
- Events emitted with `symbol_short!("Transfer")`, `symbol_short!("Mint")`, `symbol_short!("Approval")` — capitalized, single-word symbols (differs from SEP-0050's lowercase `"transfer"`, `"mint"`)
- Metadata is a single static IPFS URL per collection (not per-token `token_uri`)
- Supply fixed at 1000 tokens, open minting (no access control)
- GitHub: https://github.com/jamesbachini/Soroban-NFT

**Detection relevance:** This contract will NOT match SEP-0050 event patterns due to capitalized event symbols. WASM spec detection may still flag it (has `owner_of`, `token_uri`, `transfer` with integer token_id). Demonstrates the need for fuzzy matching.

## OpenZeppelin Reference Implementation

The canonical SEP-0050-compliant implementation is at https://github.com/OpenZeppelin/stellar-contracts (example: `examples/nft-sequential-minting`). No known mainnet deployment address was found at time of research.

## Implications for Testing

- Use the jamesbachini mainnet contract to test **non-standard detection** (capitalized events, i128 token IDs)
- Use a locally deployed OZ contract on testnet to test **SEP-0050 compliant detection**
- Absence of other known mainnet contracts suggests most NFT activity is currently on testnet or not yet deployed

## Gap Note

No exhaustive registry of Soroban NFT contracts exists. The block explorer itself, once deployed, will become the primary discovery mechanism. Until then, manual search via Stellar Expert or Horizon may reveal additional deployed contracts.
