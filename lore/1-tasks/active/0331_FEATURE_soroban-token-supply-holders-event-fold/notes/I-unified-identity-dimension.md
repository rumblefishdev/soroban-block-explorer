---
title: 'Unified identity dimension (addresses + assets) → generic balances'
type: idea
status: seed
spawned_from: README.md
spawns: []
tags: [architecture, data-model, north-star, balances, identity, option-c]
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/api/src/accounts/queries_ch.rs
history:
  - date: '2026-06-29'
    status: seed
    who: karolkow
    note: >
      Emerged from the 0331 storage-state deep dive. Karol's idea: a shared
      intermediary table for "entities used in the same places" (asset ids,
      balance holders). Parked as the option-C north-star; NOT now (option A first).
---

# Unified identity dimension → generic balances (option C)

> **Status: idea / north-star. Not now.** Decision 2026-06-29: ship **option A**
> (parallel `soroban_token_balances`) first; evolve toward this later. Each step
> stands alone — no big-bang required.

## The idea (Karol)

A shared "intermediary" table for the polymorphic-identity bytes that get used in the
same places across the schema. Two such bytes:

1. **Asset** — identified differently per type (native / `code+issuer` / `contract_id`)
   but used uniformly (balances, operations, supply). **`assets` already IS this** — one
   surrogate id per row, disjoint natural key inside; consumers join by surrogate.
2. **Holder / address** — a balance (and an op, event, invocation) can belong to a
   **G-account OR a C-contract**. This dimension does NOT exist yet: `accounts` holds G,
   `soroban_contracts` holds C, separately. This is the missing "intermediary table".

Not arbitrary: the holder dimension is literally the protocol type
`ScAddress = Account(G) | Contract(C)` (+ muxed). One table = "an addressable holder".

## Why it's strong (not just for balances)

The address dimension is **cross-cutting** — that's the main value, not balances alone:

- Operations (source/dest), events (from/to), invocations (caller) all reference a "who"
  that can be account OR contract. Today each picks one table → **INNER joins silently
  drop the other kind** (exactly the latent bug 0323 fixed: 3× INNER→LEFT, an event
  referencing a row-less contract vanished). One address dimension = total reference,
  no per-type branching, no silent drops.
- Generic `balances(holder_id → addresses, asset_id → assets, balance, version)` replaces
  `account_balances_current` + `soroban_token_balances`. supply = `sum WHERE asset_id`;
  portfolio = `WHERE holder_id`; top-holders = `ORDER BY balance`. Classic == soroban.

Measured driver (2026-06-29): **34% of type-3 token holders are contracts** (201,133 C vs
390,701 G addresses) — holders genuinely span two namespaces.

## Costs / why not now (YAGNI discipline)

1. **Hot-path migration.** `account_balances_current` drives account detail + the supply
   aggregate + canonical 06 SQL, and task 0198 is mid-surgery on its partial indexes.
   Re-keying to a generic `balances` touches everything.
2. **Surrogate collision.** `accounts.id` = hash(G-strkey), `soroban_contracts.id` =
   hash(C-strkey) — two namespaces in one Int64. The dimension MUST carry a `kind`
   discriminator (and one hash space, or a `(kind, id)` key).
3. **Index locality.** Typed partial indexes (what 0198 optimizes) can regress on a
   generic polymorphic table without careful indexing.
4. **EAV trap.** A "table of various entities" slides into entity-attribute-value
   (anti-pattern). Keep it a **typed identity dimension** (address kinds; asset kinds) with
   natural columns + surrogate + `kind`. The win is uniform REFERENCE, not schemaless
   storage. Don't model muxed / claimable / LP until they actually hold balances.

## Sequencing

```
A (now)  soroban_token_balances (parallel, ContractData Balance entries) → fills the gap
   ↓
addresses dimension (account | contract | muxed)  → pays off in ops/events/invocations too
   ↓
C        generic balances(holder_id, asset_id, balance, version)  → falls out once both
         dimensions stand
```

## Reference — asset identifier per type (prod, 2026-06-29)

| type | name | identifier |
|------|------|------------|
| 0 native | XLM | none — singleton (`native`) |
| 1 classic | USDC | `USDC:GA5ZLINQWRY2DRORJWXMQESAVT3C2KXX5KWXH36S7QBX2734DIAZQXRP` (code:issuer) |
| 2 sac | USDC | `USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN` + contract `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75` |
| 3 soroban | MERU | `CCA2ZJP5BVRXYTQH4FAGHCAUMRYCXVC4CRYC2NXHWMR7TIVX36U7F5HR` (contract only) |
