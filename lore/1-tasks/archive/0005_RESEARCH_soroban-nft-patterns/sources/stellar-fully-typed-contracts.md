---
url: 'https://developers.stellar.org/docs/learn/fundamentals/contract-development/types/fully-typed-contracts'
title: 'Fully-Typed Contracts'
fetched_date: 2026-03-26
task_id: '0005'
image_count: 0
---

# Fully-Typed Contracts

When you compile a contract created with [soroban-sdk](https://developers.stellar.org/docs/tools/sdks/contract-sdks#soroban-rust-sdk), the Wasm file includes a [custom section](https://webassembly.github.io/spec/core/appendix/custom.html) containing a machine-readable description of your contract's interface types, known as its spec or API. This resembles Ethereum's ABIs, but Soroban stores every interface on-chain from inception and includes author comments.

These interface types use [XDR](https://developers.stellar.org/docs/learn/fundamentals/data-format/xdr), a data format prevalent throughout Stellar. While XDR can be difficult to create or consume manually, tools like [Stellar CLI](https://developers.stellar.org/docs/tools/cli#cli) and [Stellar SDK](https://developers.stellar.org/docs/tools/sdks/client-sdks#javascript-sdk) simplify fetching these interface types.

## Stellar CLI: `stellar contract invoke`

Each smart contract functions as its own program deserving its own CLI. Stellar CLI provides a unique interface for each contract, constructed dynamically from on-chain interface types and including author comments — an _implicit CLI_.

For example, invoking the native asset contract on the Test network:

```
$ stellar contract invoke --network testnet --id CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC -- --help
Usage: CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC [COMMAND]

Commands:
  balance         Returns the balance of `id`.

                      # Arguments

                      * `id` - The address for which a balance is being queried. If the
                      address has no existing balance, returns 0.
  ...
  transfer        Transfer `amount` from `from` to `to`.

                      # Arguments

                      * `from` - The address holding the balance of tokens which will be
                      withdrawn from.
                      * `to` - The address which will receive the transferred tokens.
                      * `amount` - The amount of tokens to be transferred.

                      # Events

                      Emits an event with topics `["transfer", from: Address, to: Address],
                      data = amount: i128`
  ...

Options:
  -h, --help  Print help
```

Like other CLIs, you can obtain help for subcommands. Running `… -- balance --help` fetches on-chain interface types to generate a comprehensive argument list and examples.

> If you're unfamiliar with the `--` separator, everything after it gets passed to the child process. CLIs like `npm run` and `cargo run` use this pattern.

## Stellar JS SDK: `contract.Client`

Create a contract client using JavaScript:

```javascript
import { contract } from '@stellar/stellar-sdk';
const xlm = contract.Client.from({
  contractId: 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC',
  networkPassphrase: 'Test SDF Network ; September 2015',
  rpcUrl: 'https://soroban-testnet.stellar.org',
});
```

This fetches the contract from the live network and auto-generates a class for ergonomic method calls:

```javascript
xlm.balance({ id: 'G123…' });
```

While this works dynamically from browsers, the CLI's `contract bindings typescript` command generates TypeScript libraries with type-ahead and author comments:

```
stellar contract bindings typescript \
  --network testnet \
  --output-dir xlm --overwrite \
  --id CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
```

This creates a fully-typed NPM module for the contract.

## Already the best; just getting started

Soroban's on-chain interface availability from day one eliminates secondary API calls, token management, and signup requirements. "It's a game-changer within the blockchain space."

Stellar CLI and JS SDK demonstrate how foundational tooling can deliver delightful developer experiences. Future developments include adaptive GUIs functioning as interactive documentation, auto-generated code for cross-contract calls, and broader interoperability possibilities when combining smart contracts with WebAssembly across multiple platforms.
