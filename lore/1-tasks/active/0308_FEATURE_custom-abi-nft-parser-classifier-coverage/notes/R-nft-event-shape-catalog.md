---
title: 'Research: exhaustive Soroban NFT event-shape catalog'
type: research
status: mature
spawned_from: ../README.md
spawns: []
tags: [nft, sep-50, openzeppelin, event-shapes, deep-research]
links:
  - 'https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0050.md'
  - 'https://github.com/OpenZeppelin/stellar-contracts'
history:
  - date: 2026-06-19
    status: mature
    who: karolkow
    note: 'Serialized from /deep-research run (97 agents, 25 claims, 23 confirmed / 2 killed). Cross-checked vs CH census.'
---

# Research: exhaustive Soroban NFT event-shape catalog

> Goal: every (event symbol, topic layout, data layout, token_id type) a Soroban indexer
> must parse to never silently skip an NFT, and how to tell NFT from SEP-41 fungible.
> Method: deep-research harness (fan-out web search → fetch → 3-vote adversarial verify →
> synthesize), cross-checked against our ClickHouse census + on-chain RPC.

## Headline

No single doc enumerates this. The only fully-specified standard is **SEP-50** (4 events).
Everything else (burn, consecutive_mint, and all custom/launchpad ABIs) is convention or
**undocumented — discoverable only empirically on-chain**. token_id placement and width are
NOT fixed. NFT vs fungible is NOT decidable from the event alone — needs the WASM interface.

## The documented catalog (SEP-50 + OpenZeppelin)

**SEP-50 — exactly 4 events, NO burn:**

| symbol            | topics                      | data                            | token_id                              |
| ----------------- | --------------------------- | ------------------------------- | ------------------------------------- |
| `transfer`        | `[transfer, from, to]`      | `[TokenID]`                     | in DATA                               |
| `mint`            | `[mint, to]`                | `[TokenID]`                     | in DATA (single recipient, no `from`) |
| `approve`         | `[approve, owner, TokenID]` | `[approved, live_until_ledger]` | in TOPIC                              |
| `approve_for_all` | `[approve_for_all, owner]`  | `[operator, live_until_ledger]` | none (operator-level)                 |

- **token_id placement is INCONSISTENT** (data for transfer/mint, topic for approve) → a parser
  must branch per-symbol, never assume a fixed slot.
- **SEP-50 is internally contradictory on `approve`**: the prose says token_id is a topic; the Rust
  trait docstring says token_id is in data. A parser must accept either. (SEP-50 still Draft, 2025.)
- **token_id is a generic unsigned int, NO fixed width** — mainnet uses u32 (OZ default), u64, i128
  (SEP-39 ports), even bytes/string. **Store token_id as a string.**

**OpenZeppelin `stellar-contracts` (de-facto library):** variants Base / Consecutive / Enumerable +
extensions Burnable / Royalties. Emits transfer/mint/approve/approve_for_all; Burnable adds
`burn`/`burn_from`; Royalties `set_token_royalty` puts token_id in a TOPIC. token_id = u32.

**`consecutive_mint` (Consecutive ext / EIP-2309):** ONE batch event, `[consecutive_mint, to]` +
data = RANGE `from_token_id..to_token_id` (map OR vec). A per-token parser silently misses interior
ids → must expand `[from,to]` to N mints (with a sanity cap).

## NFT vs SEP-41 fungible — the critical discriminator

NFT and fungible **share the identical symbols AND topic layout AND can both carry i128 OR a map**.
The naive rule "i128 = fungible, token_id = NFT" is **FALSE** (refuted 0-3): real SEP-39 NFTs use
i128 for token_id. Reliable discrimination needs the **WASM interface** (`owner_of`/`token_uri`/
`approve_for_all`). The only safe parse-time heuristic is the **data-map KEY**: `map{token_id}` =
NFT candidate; `map{amount}` or `map{to_muxed_id}` (no token_id) = fungible/SAC. Magnitude on
mainnet: ~147.9M fungible `map{amount,to_muxed_id}` mints vs ~890 NFT `map{token_id}` mints — without
the key guard a map handler mis-ingests ~5,580 fungible contracts as NFTs.

## The four real on-chain DATA encodings (verified, task 0296)

| shape             | topics                   | data                   | note                                               |
| ----------------- | ------------------------ | ---------------------- | -------------------------------------------------- |
| A scalar          | `[Symbol, addr…]`        | bare `u32`/`u64`       | 36-37 contracts                                    |
| C map             | `[Symbol, addr…]`        | `map{"token_id": uN}`  | canonical OZ default; 14-16 contracts              |
| B packed-vec      | `[Symbol]` only          | `vec[addr…, token_id]` | hand-rolled ERC-721 ports (Bachini); ~20 contracts |
| consecutive-range | `[consecutive_mint, to]` | range (map/vec)        | expand to N                                        |

`nft.rs` (post-0296) already handles all four. A scalar-only parser dropped 34 of 72 NFT-minting
contracts chain-wide — that gap is closed.

## Not applicable / absent

- **SEP-39** (classic-asset NFT) emits NO Soroban contract events — irrelevant to the event parser
  unless SAC-wrapped.
- **No ERC-1155 / semi-fungible standard** exists on Soroban (only named in SEP-50 motivation prose).

## Custom / undocumented ABIs (the empirical-only tail)

The research found **NO documented Soroban contract** emitting `bulk_mint`, `collection_updated`,
`token_updated`, `freeze_collection`, `approve_all`, or `revoke` as NFT events. These are NOT in any
SEP / OZ / library. **They exist only on-chain (our census found them).** The correct architecture is
therefore never-silently-drop: parse the known tuples, and tripwire any candidate symbol/shape that
doesn't match — so novel ABIs surface instead of vanishing. A WASM-interface classifier is the
authoritative NFT gate downstream.

## Caveats

- SEP-50 + SEP-48 are Draft/in-flight; the `approve` layout is unresolved (prose vs trait docstring).
- The map-key heuristic has ≥1 known non-NFT false-positive — safe only because the WASM classifier is
  the authoritative gate (rows quarantine in `nfts_pending` until the verdict promotes/drops them).
- RPC `getEvents` is ~7-day retention → the live shape census under-counts the historical tail; the
  all-time numbers come from our captured `soroban_events`.
- `token_id`-as-4th-topic and fully-packed map were considered, deliberately NOT parsed (no SEP, no
  on-chain instance) — they tripwire, not parse.

## Sources (primary)

SEP-50, SEP-41, SEP-39, SEP-48 (stellar-protocol repo); OpenZeppelin stellar-contracts source
(`packages/tokens/src/non_fungible` + `extensions/consecutive`); Stellar discussions #1674, #1724;
project `crates/xdr-parser/src/nft.rs` + task 0296 notes. Full machine output:
`tasks/wf145phmk.output` (session artifact).
