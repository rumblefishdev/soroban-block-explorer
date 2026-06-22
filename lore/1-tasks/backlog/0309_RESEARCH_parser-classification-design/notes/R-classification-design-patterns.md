---
title: 'Research: design patterns for exhaustive contract/event classification'
type: research
status: mature
spawned_from: ../README.md
spawns: []
tags: [parser, classifier, architecture, completeness, sep-48, deep-research]
links:
  - 'https://github.com/orgs/stellar/discussions/1724'
  - 'https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0048.md'
history:
  - date: 2026-06-19
    status: mature
    who: karolkow
    note: 'Serialized from /deep-research run (101 agents, 25 claims, 21 confirmed / 4 killed).'
---

# Research: design patterns for exhaustive contract/event classification

> Goal: how SHOULD a blockchain indexer classify contracts and decode events so it never
> silently misses a category? Is keyword/shape-sniffing (our current approach) right, and
> what is the state of the art? Method: deep-research harness, 3-vote adversarial verify.

## Headline

On a **permissionless** chain, absolute "never miss" is **NOT achievable** — arbitrary WASM is
deployable and formal interface guarantees are insufficient. The state of the art is
**"never SILENTLY miss"**: a **total function** that maps every contract/event to exactly one
known class OR routes it to an **explicit, monitored UNKNOWN** bucket. Our keyword-classifier +
fixed event-shape allow-list is the **textbook anti-pattern**.

## Key findings (all 3-0 unless noted)

1. **Production decoders are schema/codec-first** ("parse, don't validate"): Dune, Goldsky, The
   Graph all decode against a DECLARED ABI/spec, not heuristic name/shape sniffing. This is the
   direct antidote to our failure mode.
2. **Contract-identity fingerprinting = EXACT bytecode/wasm-hash** (registry pattern), NOT
   interface-shape similarity — and exact-match **silently misses near-identical variants** (Dune
   won't decode near-identical-but-not-exact bytecode). → a hash-keyed allow-list has inherent gaps;
   use it only as a CACHE, never source of truth.
3. **Factory/registry discovery is event-driven + developer-authored** (The Graph): the author must
   anticipate every deployment path → same silent-miss class, relocated to a hand-written handler set.
4. **Soroban root cause (admitted by core devs):** events historically had NO machine-readable spec
   (only comments/SEP prose) → forces index/name/shape guessing. Exactly our NFT failure.
5. **Soroban's fix = typed spec-driven discriminator stack:**
   - **SEP-48** (Active): XDR-typed event schema (`SC_SPEC_ENTRY_EVENT_V0`) — per-param TYPE +
     TOPIC/DATA location + `prefixTopics` discriminators, in the Wasm `contractspecv0` section.
     A structured decision-tree, not name heuristics. _Caveat:_ `prefixTopics` not unique (SAC emits
     two `transfer` schemas) → it's a cascade needing a typed-param tiebreaker.
   - **SEP-46/47**: STATIC Wasm-meta capability/interface declaration (`contractmeta!`), chosen OVER
     ERC-165 runtime calls because stream indexers can't invoke contracts. _Caveat:_ SEP-47 still
     Draft, adoption ~0 today.
6. **Capability/interface detection alone is INSUFFICIENT:** a contract can implement an interface
   yet not emit its events (a Rust trait can't enforce the function body) → must backstop with an
   UNKNOWN/anomaly path.
7. **Specs are descriptive NOT authoritative, unverified-by-design:** a contract may declare
   nonexistent functions, omit real ones, or lie about SEPs (SEP-47 "informative only", verification
   "out-of-scope"). → validate against actual Wasm exports; treat spec as trust-but-verify input.
8. **STATE OF THE ART = never-SILENTLY-miss** (3-0): the unmatched case is an EXPLICIT consumer
   decision (discard / flag / route to `unknown_params`) — a total function whose residual is a
   measured, alarmed UNKNOWN bucket. Cross-ecosystem too (web3.py → 'unknown'/raw bytes).

## Pattern scorecard

| pattern                                                    | verdict                                             |
| ---------------------------------------------------------- | --------------------------------------------------- |
| Schema/codec-first ("parse don't validate")                | ✅ dominant correct pattern                         |
| Discriminator cascade (SEP-48 prefixTopics + typed params) | ✅ core, but needs tiebreaker                       |
| Capability/interface detect (SEP-46/47)                    | ✅ but insufficient alone                           |
| Unknown-quarantine + promotion + monitoring                | ✅ mandatory backstop                               |
| Registry + wasm-hash fingerprint                           | ⚠️ cache only; exact-match silently misses variants |
| Type-lattice / subsumption (formal)                        | ❌ insufficient on permissionless WASM              |

## Refuted (do NOT believe)

- Stellar SDKs do NOT auto-generate typed event decoders today (0-3) — would implement ourselves.
- `AppProtocolCatalog` is NOT the adopted solution (0-3) — the adopted direction is SEP-46/47/48.
- "i128 = fungible" event discriminator (0-3, from the NFT research) — real NFTs use i128.

## Caveats

- Production survey effectively = Dune + Goldsky + The Graph (+ Sourcify); Allium/Subsquid/Etherscan/
  Blockscout/stellar.expert/SDF Hubble/Nansen produced no surviving verified claims.
- EVM-specific primitives (4-byte selectors, CBOR metadata hash) have no Soroban analog; Soroban's
  static-metadata interface discovery has no EVM analog. Patterns transfer; primitives don't.
- Formal-methods pillar answered NEGATIVELY (formal guarantees insufficient), not positively.

## Open questions (research could not close)

- **Upgrade / type-mutation over time:** how to re-classify + re-decode history when a contract's
  Wasm is swapped (`update_current_contract_wasm`) and "object X becomes Y"? No published answer —
  our exact case (NFT families with 4 wasm versions). Novel ground.
- **Completeness-monitoring SLOs:** no published %UNKNOWN thresholds / promotion mechanics.
- **How Soroban-native indexers classify TODAY** (stellar.expert, SDF Hubble, Subsquid Soroban) —
  likely the same heuristics; unconfirmed.
- **Type recovery when spec is absent/lying:** wasm-hash clustering vs labeled corpus, behavioral
  fingerprinting — residual silent-miss rate unknown.

## Sources (primary)

Dune / Goldsky / The Graph decoding docs; Sourcify metadata-hash blog; SEP-41/46/47/48 +
stellar-xdr `Stellar-contract-spec.x`; Stellar discussions #1596/#1659/#1724; EIP-165;
"parse don't validate" (lexi-lambda); arXiv 2501.00965 / 1811.11645 / 2502.13513. Full machine
output: `tasks/w5c40x3ia.output` (session artifact). See also auto-memory
`reference-soroban-classification-seps`.
