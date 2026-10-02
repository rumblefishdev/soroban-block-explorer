# R — Aquarius first: on-chain research (2026-08-21)

Moved out of the task README unchanged; the decisions it led to are in the
README's later sections.

Decision: Aquarius is the first Soroban AMM adapter. Everything below was
measured against production ClickHouse and cross-checked against mainnet via
`stellar contract invoke --send=no` (read-only simulation, RPC
`mainnet.sorobanrpc.com`). Nothing here is inferred from our own code.

## What the store already holds

`soroban_events.topics_xdr` / `data_xdr` are decoded scval JSON, so the whole
protocol is already queryable without touching XDR again:

| Event                                      | Emitter | Shape                                                                                                                                                                | Rows            |
| ------------------------------------------ | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------- |
| `add_pool`                                 | router  | topics `[sym, vec<token addresses>]`, data `[pool address, sym pool_type, bytes pool_hash, vec params]`                                                              | 410             |
| `update_reserves`                          | pool    | topics `[sym]`, data `vec<i128>` — one entry per token, **same order as the pool's `get_tokens()`**                                                                  | 3 302 989       |
| `trade`                                    | pool    | topics `[sym, token_in, token_out, caller]`, data `[amount_in, amount_out, fee]`                                                                                     | 4 158 845       |
| `deposit_liquidity` / `withdraw_liquidity` | pool    | topics `[sym, …tokens]`, data `[shares, …amounts]` _(corrected 2026-08-26 — recorded backwards here originally; shares is element 0, verified on ledger 61 777 648)_ | 75 276 / 27 145 |

Event arity tracks token count: 2-token pools give 3 topics / 3 data, the
3-token stable pools give 4 / 4. No other variants exist across all history.

## Three findings that change the plan

**1. There are TWO routers, not one — and pools exist outside both.**
_(Counts corrected 2026-08-26 — there are ten routers and 496 pools; see
the verification pass at the end of this file.)_

`CBQDHNBF…6QUK` (the address Aquarius documents) holds 304 token sets / **337
pools**. A second contract, `CA7RQDMM…UOJQ`, reports `contract_name() =
"AMMRouter"`, `version() = 200` — same as the first — and holds 57 token sets /
**73 pools**, disjoint from the first. Total **410 registered pools**:
`constant` 319, `stable` 57, `concentrated` 34.

Each router's `add_pool` events reproduce its live registry **exactly** (337 and
73, verified by enumerating `get_pools_for_tokens_range` on chain and diffing
the address sets — zero difference either way). But of the 177 pools active in
the newest partition, **10 are absent from the first router's registry**; all 10
sit in the second. Building discovery on one documented router address silently
drops ~6 % of live pools.

→ Discovery must be shape-driven, not address-driven: any contract emitting
`add_pool` IS a router; any contract emitting `update_reserves` + `trade` IS a
pool, registered or not. No hard-coded addresses.

**2. Aquarius DOES have share tokens — participants come free after all.**

An earlier read of this said otherwise; it was wrong, and it was wrong because
it checked the POOL contract against `assets` instead of the pool's share
token. `share_id()` on a `constant` pool returns a separate token contract,
which is already indexed:

- pool `CD3XIX65…UKWL` → share token `CAM3JVJL…3ZYY`
- our `balances`, deduped by `argMax(amount, last_updated_ledger)`: **5 holders,
  8 810 229 081 shares**
- chain `get_total_shares()`: **8 810 229 081** — exact match

The dedup is not optional: the raw `sum(amount)` on the same asset returns
19 412 769 722 (10 rows for 5 holders) — unmerged RMT duplicates.

`concentrated` pools are the exception: `share_id()` returns the pool itself,
the pool is not in `assets`, and positions are tick-ranged (`position_update`,
`pool_state`). Those need their own treatment or an explicit "not indexed".

**3. Concentrated pools are not a rounding error — they are a third of the flow.**

Newest partition, by pool type: `constant` 25 079 trades over 122 pools,
`concentrated` 21 356 over 21, `stable` 8 010 over 24. The single busiest
Aquarius pool on the network is concentrated (XLM/AQUA, fee 10, tick spacing
20). Shipping "constant only" would omit ~39 % of recent trades **and** the top
pool — that is not a defensible first cut.

## Reserves are exact — verified against chain, three pool types

Latest `update_reserves` from our events vs live `get_reserves()`:

| Pool            | Type         | Ours                                       | Chain     |
| --------------- | ------------ | ------------------------------------------ | --------- |
| `CBBMQBNH…BUCV` | concentrated | `40196052765563, 5748484968000`            | identical |
| `CBMWU357…2LSH` | constant     | `1044176401956, 353830778`                 | identical |
| `CCYMZTOJ…JX25` | stable       | `1282501540990846914271528, 7176974914804` | identical |

`get_tokens()` order matched the `add_pool` token vector on every pool checked,
so the reserve vector needs no reordering.

Trade arithmetic reconciles too: between two consecutive snapshots on
`CBMWU357…2LSH`, `reserve_out` moved by exactly `-amount_out` and `reserve_in`
by `amount_in - fee` (exact on one sample, 1 unit off on another — rounding, to
be pinned as a tolerance, not assumed away).

## Traps to design against

- **`trade` topic 4 is the CALLER, not the end user.** On router-mediated swaps
  it is the router address. Sampled counts (244 654 trades over 30 days through
  113 distinct addresses) are a symptom of this, not of 113 real traders. Do not
  render it as "trader".
- **Reserves are not Decimal128(7).** A stable-pool token carries 18 decimals
  (`CBZ4DCE7…N2PJ`, `decimals() = 18`, reserve 1.28e24 raw). Store raw `Int128`
  plus decimals; the classic `liquidity_pool_snapshots` scale would corrupt it.
  That same token has **no row** in `soroban_contract_metadata`, so decimals
  cannot always be resolved from our store today.
- **Leg identity needs two lookups.** `asset_sac` by `sac_contract_id` resolves
  native and classic-credit legs (verified: XLM SAC → native, `CCW67TSZ…MI75` →
  USDC); soroban-native legs resolve directly on `assets.contract_id`.
- **`soroban_contracts.wasm_hash` is stale for upgraded contracts** (task 0320),
  so it is NOT a usable discovery key — the four pools sampled showed three
  different hashes.
- **22 % of trade history predates its pool's reserve stream.** 921 119 of
  4 158 845 trades, across 79 pools, occur before that pool's first
  `update_reserves`. Reconstruction backwards from the first known snapshot is
  arithmetically possible (deltas above) but must be proven per pool, not
  assumed.
