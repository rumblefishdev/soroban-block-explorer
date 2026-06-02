---
url: 'https://docs.openzeppelin.com/stellar-contracts/tokens/non-fungible/non-fungible'
title: 'Non-Fungible Token - OpenZeppelin Stellar Contracts'
fetched_date: 2026-03-26
task_id: '0005'
image_count: 0
---

# Non-Fungible Token

## Overview

The non-fungible module provides three different NFT variants that differ in how certain features like ownership tracking, token creation and destruction are handled:

1. **Base**: Contract variant that implements the base logic for the NonFungibleToken interface. Suitable for most use cases.
2. **Consecutive**: Contract variant for optimized minting of batches of tokens. Builds on top of the base variant, and overrides the necessary functions from the `Base` variant.
3. **Enumerable**: Contract variant that allows enumerating the tokens on-chain. Builds on top of the base variant, and overrides the necessary functions from the `Base` variant.

These three variants share core functionality and a common interface, exposing identical contract functions as entry-points. However, composing custom flows must be handled with extra caution due to incompatible business logic between variants or the need to wrap base functionality with additional logic.

## Usage

This example demonstrates NFT implementation for tracking game items with unique attributes. When awarded to players, tokens are minted and sent to them. Players can keep, burn, or trade their tokens.

```rust
use soroban_sdk::{contract, contractimpl, Address, Env, String};
use stellar_tokens::non_fungible::{
    burnable::NonFungibleBurnable,
    Base, ContractOverrides, NonFungibleToken,
};

#[contract]
pub struct GameItem;

#[contractimpl]
impl GameItem {
    pub fn __constructor(e: &Env) {
        Base::set_metadata(
            e,
            String::from_str(e, "www.mygame.com"),
            String::from_str(e, "My Game Items Collection"),
            String::from_str(e, "MGMC"),
        );
    }

    pub fn award_item(e: &Env, to: Address) -> u32 {
        // access control might be needed
        Base::sequential_mint(e, &to)
    }
}

#[contractimpl(contracttrait)]
impl NonFungibleToken for GameItem {
    type ContractType = Base;
}

#[contractimpl(contracttrait)]
impl NonFungibleBurnable for GameItem {}
```

## Extensions

The following optional extensions are provided to enhance capabilities:

### Burnable

The `NonFungibleBurnable` trait extends the `NonFungibleToken` trait to provide the capability to burn tokens.

### Consecutive

The `NonFungibleConsecutive` extension is optimized for batch minting of tokens with consecutive IDs. This approach drastically reduces storage writes during minting by storing ownership only at boundaries and inferring ownership for other tokens.

This extension builds around the contract variant `Consecutive`. See [Non-Fungible Consecutive](https://docs.openzeppelin.com/stellar-contracts/tokens/non-fungible/nft-consecutive) for detailed documentation.

### Enumerable

The `NonFungibleEnumerable` extension enables on-chain enumeration of tokens owned by an address. See [Non-Fungible Enumerable](https://docs.openzeppelin.com/stellar-contracts/tokens/non-fungible/nft-enumerable) for detailed documentation.

This extension builds around the contract variant `Enumerable`.

### Royalties

The `NonFungibleRoyalties` trait extends the `NonFungibleToken` trait to provide royalty information for tokens, similar to ERC-2981 standard. This allows marketplaces to query royalty information and pay appropriate fees to creators.

Note: The royalties extension allows both collection-wide default royalties and per-token royalty settings.
