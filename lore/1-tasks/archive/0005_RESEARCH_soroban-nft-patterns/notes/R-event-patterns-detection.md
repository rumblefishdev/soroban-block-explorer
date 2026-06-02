---
prefix: R
title: NFT event patterns and detection via events
status: mature
spawned_from: null
spawns: []
sources:
  - ../sources/sep-0050-nft-standard.md
  - ../sources/stellar-events-structure.md
  - ../sources/stellar-rpc-getevents.md
  - ../sources/bachini-soroban-nft-tutorial.md
---

# Research: NFT Event Patterns and Detection

## SEP-0050 Event Specifications

SEP-0050 defines four event types for NFTs. These are the canonical patterns to detect:

### Transfer Event

```
topics: [Symbol("transfer"), Address(from), Address(to)]
data:   TokenID(token_id)
```

### Mint Event

```
topics: [Symbol("mint"), Address(to)]
data:   TokenID(token_id)
```

### Approve Event

```
topics: [Symbol("approve"), Address(owner), TokenID(token_id)]
data:   (Address(approved), u32(live_until_ledger))
```

### Approve For All Event

```
topics: [Symbol("approve_for_all"), Address(owner)]
data:   (Address(operator), u32(live_until_ledger))
```

### Burn Event

SEP-0050 **does not define a burn function or burn event** in the core `NonFungibleToken` trait. OpenZeppelin's `NonFungibleBurnable` extension adds burn capability as an opt-in.

No standardized burn event format exists in SEP-0050. Community implementations vary:

- Some emit a `transfer` event from owner to zero address (following ERC-721 conventions)
- Some emit a dedicated `burn` event (similar to SEP-0041's `burn: [Symbol("burn"), Address(from)], data: TokenID`)
- Some emit no event at all

**Detection implication:** Burn detection **cannot rely on events** — it must rely on WASM spec (presence of `burn` function in `contractspecv0`). An NFT contract with the `NonFungibleBurnable` extension will have a `burn(token_id: TokenID)` function in its spec.

## Event Structure in Stellar

Events are stored in `TransactionMeta` as `ContractEvent` objects:

```xdr
union ContractEvent switch (int v)
{
case 0:
    struct
    {
        ExtensionPoint ext;
        Hash* contractID;
        ContractEventType type;
        union switch (int v)
        {
        case 0:
            struct
            {
                SCVal topics<>;  // 1-4 SCVal topics
                SCVal data;      // single SCVal payload
            } v0;
        } body;
    } v0;
};
```

Three event types:

- **CONTRACT** — emitted by contracts via `contract_event` host function
- **SYSTEM** — emitted by host (e.g., executable updates)
- **DIAGNOSTIC** — debug only, not in production

Events are in `TransactionMetaV3.sorobanMeta.events` (or V4 equivalent). Only populated on successful transactions.

## Differentiating NFT from Fungible Token Events

Both SEP-0041 and SEP-0050 use `"transfer"` and `"mint"` as event topic symbols. Differentiation requires inspecting the data payload:

| Event           | SEP-0041 (Fungible) | SEP-0050 (NFT)                                                          |
| --------------- | ------------------- | ----------------------------------------------------------------------- |
| `transfer` data | `i128(amount)`      | `TokenID(token_id)`                                                     |
| `mint` data     | `i128(amount)`      | `TokenID(token_id)`                                                     |
| `burn` data     | `i128(amount)`      | _(not in SEP-0050 core; implementation-specific in Burnable extension)_ |

**Detection heuristic:** If a `transfer` event's data is a small integer (sequential ID) rather than an `i128` (large amount), it's likely an NFT event. However, this alone is unreliable — must combine with contract spec analysis.

**Non-standard event symbols:** Community contracts may emit capitalized symbols (e.g., `"Transfer"`, `"Mint"`) instead of lowercase (`"transfer"`, `"mint"`). The known jamesbachini mainnet contract (`CDA5FGE4...`) uses `symbol_short!("Transfer")`. The event-based filter must handle both cases.

## RPC Event Querying

The `getEvents` RPC method supports filtering:

- By `contractIds` (up to 5)
- By `topics` (with wildcard `*` and `**` support)
- By event `type` ("contract" / "system")

**Critical limitation:** RPC retains events for only **7 days** (default 24 hours). The block explorer **must** run its own ingestion pipeline from `TransactionMeta` for historical data. Cannot rely on `getEvents` for historical NFT queries.

## Event-Based Detection Strategy

For the block explorer's indexing pipeline:

1. **At ingestion time:** Parse all `ContractEvent` objects from `TransactionMeta`
2. **First-pass filter:** Look for topic[0] = `Symbol("mint")` or `Symbol("transfer")`
3. **Cross-reference:** Check if the `contractID` is already classified as NFT (from WASM spec analysis)
4. **If unknown contract:** Queue for WASM spec analysis, store event tentatively
5. **Data type check:** Verify data payload type matches expected NFT pattern (integer TokenID, i.e. `u32` in OZ or `i128` in community contracts)

Event-only detection is a secondary signal — WASM spec analysis should be the primary classifier.
