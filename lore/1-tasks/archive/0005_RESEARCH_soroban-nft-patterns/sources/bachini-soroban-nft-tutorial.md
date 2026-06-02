---
url: 'https://jamesbachini.com/soroban-nft/'
title: 'Deploying An NFT Using Stellar Soroban'
author: 'James Bachini'
fetched_date: 2026-03-26
task_id: '0005'
overwritten: false
image_count: 2
images:
  - original_url: 'https://jamesbachini.com/wp-content/uploads/2024/11/image-3.png'
    local_path: 'images/jamesbachini-com__soroban-nft/img_2.png'
    alt: "Pinata IPFS file manager showing a list of uploaded files including metadata.json and image files, with an 'Upload File' dialog open prompting to click to upload a file up to 25 GB"
  - original_url: 'https://jamesbachini.com/wp-content/uploads/2024/11/image-4.png'
    local_path: 'images/jamesbachini-com__soroban-nft/img_3.png'
    alt: 'Pixel art illustration of the Stellar Stroopy robot mascot in a yellow suit holding coins, standing on a cyan alien landscape under a purple sky with the Stellar logo in the upper left and a glowing teal moon in the background'
---

# Deploying An NFT Using Stellar Soroban

## Overview

This tutorial demonstrates how to deploy a simple NFT contract to Soroban, Stellar's smart contract platform. The guide covers everything from prerequisites through minting your first token.

## Prerequisites

You'll need:

- An image file for the artwork
- A free Pinata account for IPFS storage
- Rust/Stellar-cli development environment installed

## The Soroban NFT Contract

The open-source contract is available on GitHub at: [https://github.com/jamesbachini/Soroban-NFT](https://github.com/jamesbachini/Soroban-NFT)

This implementation is "loosely based on the ERC-721 standard alongside the SEP-0039 proposal."

> **Warning:** Released under 3N consideration — Not Tested, Not Audited, Not Safe For Production.

### Contract Code

```rust
#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Bytes, String, Env, Vec};

#[contract]
pub struct SorobanNFT;

#[contracttype]
pub enum DataKey {
    Owner(i128),
    TokenCount,
    Approvals(i128),
}

#[contractimpl]
impl SorobanNFT {
    const SUPPLY: i128 = 1000;
    const NAME: &'static str = "SorobanNFT";
    const SYMBOL: &'static str = "SBN";
    const METADATA: &'static str = "https://ipfs.io/ipfs/QmegWR31kiQcD9S2katTXKxracbAgLs2QLBRGruFW3NhXC";
    const IMAGE: &'static str = "https://ipfs.io/ipfs/QmeRHSYkR4aGRLQXaLmZiccwHw7cvctrB211DzxzuRiqW6";

    pub fn owner_of(env: Env, token_id: i128) -> Address {
        env.storage().persistent().get(&DataKey::Owner(token_id)).unwrap_or_else(|| {
            Address::from_string_bytes(&Bytes::from_slice(&env, &[0; 32]))
        })
    }

    pub fn name(env: Env) -> String {
        String::from_str(&env, Self::NAME)
    }

    pub fn symbol(env: Env) -> String {
        String::from_str(&env, Self::SYMBOL)
    }

    pub fn token_uri(env: Env) -> String {
        String::from_str(&env, Self::METADATA)
    }

    pub fn token_image(env: Env) -> String {
        String::from_str(&env, Self::IMAGE)
    }

    pub fn is_approved(env: Env, operator: Address, token_id: i128) -> bool {
        let key = DataKey::Approvals(token_id);
        let approvals = env.storage().persistent().get::<DataKey, Vec<Address>>(&key).unwrap_or_else(|| Vec::new(&env));
        approvals.contains(&operator)
    }

    pub fn transfer(env: Env, owner: Address, to: Address, token_id: i128) {
        owner.require_auth();
        let actual_owner = Self::owner_of(env.clone(), token_id);
        if owner == actual_owner {
            env.storage().persistent().set(&DataKey::Owner(token_id), &to);
            env.storage().persistent().remove(&DataKey::Approvals(token_id));
            env.events().publish((symbol_short!("Transfer"),), (owner, to, token_id));
        } else {
            panic!("Not the token owner");
        }
    }

    pub fn mint(env: Env, to: Address) {
        let mut token_count: i128 = env.storage().persistent().get(&DataKey::TokenCount).unwrap_or(0);
        assert!(token_count < Self::SUPPLY, "Maximum token supply reached");
        token_count += 1;
        env.storage().persistent().set(&DataKey::TokenCount, &token_count);
        env.storage().persistent().set(&DataKey::Owner(token_count), &to);
        env.events().publish((symbol_short!("Mint"),), (to, token_count));
    }

    pub fn approve(env: Env, owner: Address, to: Address, token_id: i128) {
        owner.require_auth();
        let actual_owner = Self::owner_of(env.clone(), token_id);
        if owner == actual_owner {
            let key = DataKey::Approvals(token_id);
            let mut approvals = env.storage().persistent().get::<DataKey, Vec<Address>>(&key).unwrap_or_else(|| Vec::new(&env));
            if !approvals.contains(&to) {
                approvals.push_back(to.clone());
                env.storage().persistent().set(&key, &approvals);
                env.events().publish((symbol_short!("Approval"),), (owner, to, token_id));
            }
        } else {
            panic!("Not the token owner");
        }
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, token_id: i128) {
        spender.require_auth();
        let actual_owner = Self::owner_of(env.clone(), token_id);
        if from != actual_owner {
            panic!("From not owner");
        }
        let key = DataKey::Approvals(token_id);
        let approvals = env.storage().persistent().get::<DataKey, Vec<Address>>(&key).unwrap_or_else(|| Vec::new(&env));
        if !approvals.contains(&spender) {
            panic!("Spender is not approved for this token");
        }
        env.storage().persistent().set(&DataKey::Owner(token_id), &to);
        env.storage().persistent().remove(&DataKey::Approvals(token_id));
        env.events().publish((symbol_short!("Transfer"),), (from, to, token_id));
    }
}

mod test;
```

### Key Features

- Fixed name ("SorobanNFT"), symbol ("SBN"), and links to token metadata and image on IPFS
- A supply limit of 1000 NFTs that anyone can mint without restrictions
- Standard NFT operations: `transfer`, `approve`, and `transfer_from`
- Event emissions for Transfer, Mint, and Approval actions
- Metadata retrieval functions: `name`, `symbol`, `token_uri`, `token_image`

A unit test suite is available at: [https://github.com/jamesbachini/Soroban-NFT/blob/main/src/test.rs](https://github.com/jamesbachini/Soroban-NFT/blob/main/src/test.rs)

## IPFS Metadata

Store both image and metadata on IPFS using Pinata.

### Example Metadata Format

```json
{
  "name": "SorobanNFT",
  "description": "A prototype Soroban NFT contract",
  "url": "ipfs://QmeRHSYkR4aGRLQXaLmZiccwHw7cvctrB211DzxzuRiqW6",
  "issuer": "GB2QDUX7OJZ64BBG2PIFIY3WKUCOSFQSP6QJ7MZ32NOYAJJJ3FBOXA36",
  "code": "SBN"
}
```

This follows the SEP-0039 proposal standards.

### Upload Process

1. Upload your image to Pinata first
2. Create metadata JSON with the image IPFS hash
3. Upload the metadata JSON to Pinata
4. Update the contract constants with IPFS URLs

![Pinata IPFS file manager showing a list of uploaded files including metadata.json and image files, with an 'Upload File' dialog open prompting to click to upload a file up to 25 GB](images/jamesbachini-com__soroban-nft/img_2.png)

Update the contract constants (lines 30-31):

```rust
const METADATA: &'static str = "https://ipfs.io/ipfs/QmegWR31kiQcD9S2katTXKxracbAgLs2QLBRGruFW3NhXC";
const IMAGE: &'static str = "https://ipfs.io/ipfs/QmeRHSYkR4aGRLQXaLmZiccwHw7cvctrB211DzxzuRiqW6";
```

Note: You can use either `ipfs://` protocol or gateway URLs like `https://ipfs.io/ipfs/`.

## Deploying Your Soroban NFT

### Compile the Contract

```bash
cargo build --target wasm32-unknown-unknown --release
```

This generates a `soroban_nft.wasm` file in the target directory.

### Fund Your Account

For testnet deployment:

```bash
stellar keys generate --global james --network testnet --fund
```

Mainnet deployments require sending real XLM to cover transaction fees.

### Deploy the Contract

```bash
stellar contract deploy --wasm target/wasm32-unknown-unknown/release/soroban_nft.wasm --source james --network testnet
```

Save the resulting contract ID (alphanumeric string starting with C).

## Mint A Soroban NFT

```bash
stellar contract invoke --id CONTRACT_ID --source james --network testnet -- mint --to YOUR_ADDRESS
```

### Pre-Deployed Contracts

You can mint from these existing contracts:

- **Testnet**: `CCOMN26TIW2LJW4A7NWY5UF47X6HNYAF44ZFA2GIIGCSMBEIWVIH2DMA`
- **Mainnet**: `CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY`

![Pixel art illustration of the Stellar Stroopy robot mascot in a yellow suit holding coins, standing on a cyan alien landscape under a purple sky with the Stellar logo in the upper left and a glowing teal moon in the background](images/jamesbachini-com__soroban-nft/img_3.png)

## Summary

This contract can be extended to include business logic like royalties, whitelists, or dApp integration.

**Disclaimer**: The contract is minimally tested and unaudited, making it unsuitable for production use.

## Additional Resources

- **YouTube**: [https://youtu.be/fTsXL8g4fAw](https://youtu.be/fTsXL8g4fAw)
- **GitHub**: [https://github.com/jamesbachini/Soroban-NFT](https://github.com/jamesbachini/Soroban-NFT)
