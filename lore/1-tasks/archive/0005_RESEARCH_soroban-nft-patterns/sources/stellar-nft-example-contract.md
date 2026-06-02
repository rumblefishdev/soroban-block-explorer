---
url: 'https://developers.stellar.org/docs/build/smart-contracts/example-contracts/non-fungible-token'
title: 'Non-Fungible Token by OpenZeppelin'
fetched_date: 2026-03-26
task_id: '0005'
image_count: 0
---

# Non-Fungible Token by OpenZeppelin

## Overview

The non-fungible token module offers three distinct contract variants for different use cases:

1. **Base**: Implements core NonFungibleToken interface logic suitable for most applications
2. **Consecutive**: Optimized for batch token minting, extending the Base variant
3. **Enumerable**: Enables on-chain token enumeration, building on Base functionality

All variants share core functionality and identical contract entry-points, though composing custom workflows requires careful consideration due to incompatibilities between business logic implementations.

## Run the Example

Set up your development environment following the Setup process, then clone the repository:

```
git clone https://github.com/OpenZeppelin/stellar-contracts
```

Alternatively, use GitHub Codespaces or Code Anywhere to skip local setup.

Navigate to the example directory and run tests:

```
cd examples/nft-sequential-minting
cargo test
```

## Code

The example demonstrates using OpenZeppelin Stellar Contracts to create non-fungible tokens with pre-built, audited implementations:

```rust
use soroban_sdk::{contract, contractimpl, Address, Env, String};
use stellar_access::ownable::{self as ownable, Ownable};
use stellar_macros::{default_impl, only_owner};
use stellar_tokens::non_fungible::{
    burnable::NonFungibleBurnable, Base, ContractOverrides, NonFungibleToken,
};

#[contract]
pub struct NonFungibleTokenContract;

#[contractimpl]
impl NonFungibleTokenContract {
    pub fn __constructor(e: &Env, owner: Address) {
        Base::set_metadata(
            e,
            String::from_str(e, "www.example.com"),
            String::from_str(e, "My NFT Collection"),
            String::from_str(e, "MNFT"),
        );

        ownable::set_owner(e, &owner);
    }

    #[only_owner]
    pub fn mint(e: &Env, to: Address) -> u32 {
        Base::sequential_mint(e, &to)
    }
}

#[default_impl]
#[contractimpl]
impl NonFungibleToken for NonFungibleTokenContract {
    type ContractType = Base;
}

#[default_impl]
#[contractimpl]
impl NonFungibleBurnable for NonFungibleTokenContract {}

#[default_impl]
#[contractimpl]
impl Ownable for NonFungibleTokenContract {}
```

## How it Works

The contract implements key features:

1. **Sequential Minting**: Automatically assigns sequential token IDs starting from 1
2. **Ownership Control**: Uses the Ownable pattern for administrative functions
3. **Burnable Tokens**: Allows token holders to burn their tokens
4. **Secure Minting**: Only the contract owner can mint new tokens

### Using OpenZeppelin Library Components

The library provides modular components for composition:

- **`Base`**: Core non-fungible token functionality
- **`Ownable`**: Access control pattern for administrative functions
- **`NonFungibleBurnable`**: Extension enabling token burning
- **Macros**: `#[only_owner]` provides declarative access control

The `#[default_impl]` macro generates standard implementations, reducing boilerplate while maintaining NFT standard compatibility.

### Enhanced Security Features

**Access Control**: The `#[only_owner]` macro ensures only designated owners perform administrative actions like minting.

**Sequential ID Generation**: The library handles secure token ID generation, preventing collisions and ensuring uniqueness.

**Secure Defaults**: The library implements secure defaults for all NFT operations, including proper authorization checks and event emissions.

## Usage

This example tracks game items as NFTs with unique attributes. When awarded to players, tokens are minted and sent to them:

```rust
use soroban_sdk::{contract, contractimpl, Address, Env, String};
use stellar_macros::default_impl;
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
        Base::sequential_mint(e, &to)
    }
}

#[default_impl]
#[contractimpl]
impl NonFungibleToken for GameItem {
    type ContractType = Base;
}

#[default_impl]
#[contractimpl]
impl NonFungibleBurnable for GameItem {}
```

## Tests

Key test scenarios include:

- **Token Operations**: Testing mint, transfer, burn, and approval functions
- **Access Control**: Verifying only the owner performs administrative actions
- **Token ID Generation**: Ensuring sequential IDs are generated correctly
- **Authorization**: Confirming proper authentication requirements for each function
- **Edge Cases**: Testing boundary conditions and error scenarios

Run tests with:

```
cd examples/nft-sequential-minting
cargo test
```

## Build the Contract

Use the `stellar contract build` command:

```
stellar contract build
```

The compiled `.wasm` file outputs to the target directory:

```
target/wasm32v1-none/release/nft_sequential_minting.wasm
```
