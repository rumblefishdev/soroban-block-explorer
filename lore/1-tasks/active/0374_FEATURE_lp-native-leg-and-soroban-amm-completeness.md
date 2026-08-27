---
id: '0374'
title: 'LP completeness: native XLM leg match + Soroban-AMM union + share% recompute'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-medium, layer-api, liquidity-pools]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/405'
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker. Bundles F-B/K2-2, K3-5, K4-6.'
  - date: 2026-08-14
    status: backlog
    who: karolkow
    note: >
      Linked issue 405 (add Soroban AMM protocols). Rewrote K3-5: the union is
      the last step, not the work — no Soroban pool state is indexed today.
  - date: '2026-08-21'
    status: active
    who: karolkow
    note: >
      Activated. First-protocol scope confirmed reachable from data already in
      `soroban_events`; the backfill is an in-DB INSERT ... SELECT, not an
      S3 re-parse.
---

# LP completeness

## Summary

Make liquidity-pool activity complete: match the native XLM leg (currently
unmatchable → 21.7% of pools invisible), union Soroban-AMM pools into
`/liquidity-pools`, and recompute stale `share_percentage`.

## Context

Spawned from 0359. Mostly read/query-side: the native leg fails to match because
of the two-conventions native representation (see memory: native two
conventions); Soroban AMMs live outside the classic pool table.

## Implementation

- **F-B / K2-2** — match the native XLM leg in LP snapshots (16 552 pools /
  21.7% currently invisible).
- **K3-5** — surface Soroban-AMM pools. The union into `/liquidity-pools` is the
  final step; the actual work is extracting pool state we do not index at all
  today (no reserves, no swap volume, no pool row — only the LP token contract).
  One adapter per protocol:
  - **Soroswap first** — its LP tokens already carry on-chain `METADATA` we
    read (248 `…Soroswap…` names in `soroban_contract_metadata`), so pool
    discovery is a lookup.
  - **Aquarius second** — only 19 metadata hits, so its pool contracts must be
    discovered via factory/registry and decoded from swap events. The harder
    half, despite being the more-requested one in issue 405.
  - then union with the classic pools + a Classic/Soroban filter on the list
    (cheap once both live in one list).
  - `ContractType` has no `Dex` variant — 131 740 contracts sit in `Other`.
    Splitting it is anticipated in `crates/domain/src/enums/contract_type.rs`.
- **K4-6** — recompute stale LP `share_percentage` (unconfirmed; verify first).

## Acceptance Criteria

- [ ] native XLM leg matches → pools visible — F-B / K2-2
- [ ] Soroswap pools indexed (reserves + volume) and unioned — K3-5
- [ ] Aquarius pools indexed and unioned — K3-5
- [ ] Classic / Soroban filter on the pool list — K3-5
- [ ] share_percentage correct (or confirmed already correct) — K4-6

---

## Aquarius first — on-chain research, 2026-08-21

Decision: Aquarius is the first Soroban AMM adapter. Everything below was
measured against production ClickHouse and cross-checked against mainnet via
`stellar contract invoke --send=no` (read-only simulation, RPC
`mainnet.sorobanrpc.com`). Nothing here is inferred from our own code.

### What the store already holds

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

### Three findings that change the plan

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

### Reserves are exact — verified against chain, three pool types

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

### Traps to design against

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

## Atomic steps

Each step is independently landable and carries its own check.

**A. Registry**

> Superseded in part by the schema review of 2026-08-27 at the end of this
> file: four tables, not two; every fact table keys on
> `(pool_id, ledger_sequence, transaction_id, event_index)`; the registry is an
> `AggregatingMergeTree`, not a plain RMT.

1. `CREATE TABLE soroban_pools` — pool contract id, protocol, pool type,
   registering router id (0 = unregistered), token ids array, fee params,
   share token id, first-seen ledger, version column. Raw `Int128` policy
   applies to nothing here; this table is identity only. _(Karol runs the DDL.)_
2. Parser arm: any `add_pool` event → pool row, router taken from the emitter.
   No address allowlist.
3. Parser arm: `update_reserves` / `trade` from a contract with no pool row →
   write a pool row with `router = 0`, tokens from the trade topics. Orphans are
   never silently dropped.
4. Backfill: `INSERT … SELECT` over `soroban_events` for `add_pool`. 410 rows.
5. **Check:** enumerate every discovered router on chain via
   `get_pools_for_tokens_range` and diff against the table. Zero pools live but
   missing. (Script exists in scratch form from this research.)

**B. Reserves**

6. `CREATE TABLE soroban_pool_snapshots` — pool id, ledger, `Array(Int128)`
   reserves, source event index. _(Karol runs the DDL.)_
7. Parser arm: `update_reserves` → snapshot row.
8. Backfill: `INSERT … SELECT`, ~3.3 M rows.
9. **Check:** for a sample across all three pool types, latest stored reserves
   equal live `get_reserves()`. Three pools already pass; widen the sample.

**C. Volume**

10. Parser arm: `trade` → per-trade row (pool, ledger, token_in/out ids,
    amount_in, amount_out, fee).
11. Backfill, ~4.16 M rows.
12. **Check:** between consecutive snapshots, `Δreserve_in == amount_in - fee`
    and `Δreserve_out == -amount_out`, within the documented rounding tolerance.

**D. Identity and units**

13. Leg resolver: contract address → asset identity via `asset_sac`
    (`sac_contract_id`) with fallback to `assets.contract_id`. Unit tests for
    native, classic-credit, soroban-native.
14. Decimals resolver + the missing-metadata case. A leg whose decimals are
    unknown renders raw with an explicit marker — never a plausible wrong number.

**E. Participants**

15. Derive share token per pool from events: the token contract emitting `mint`
    in the same transaction as the pool's `deposit_liquidity`.
    **Check:** matches `share_id()` on a sample.
16. Participants read = `balances` on that asset, deduped by
    `argMax(amount, last_updated_ledger)`.
    **Check:** summed shares equal chain `get_total_shares()` per pool.
17. Concentrated pools: **decided 2026-08-26 — index the positions.**
    `position_update` carries holder + tick range + liquidity delta; state is
    one GROUP BY (712 open positions, 273 holders, 26 pools). List returns
    positions, header counts holders; constant pools are the degenerate
    one-position case. Open positions only; raw L + price range (no
    token-amount conversion until it passes an on-chain check).
    `ParticipantItem` gains optional range fields; shares become optional.

**F. API**

18. `PoolItem.pool_id` widens from the SEP-23 `L…` strkey to also carry a `C…`
    contract address; add protocol + pool type. **api-types regen.**
19. Legs become a list, not `asset_a`/`asset_b` — 3-token stable pools exist.
20. List endpoint unions classic + Aquarius; `filter[protocol]`.
21. Detail, participants and activity endpoints routed per protocol.

**G. Frontend**

22. Pool route accepts a `C…` id.
23. Classic / Soroban filter in `PoolsFilterBar`.
24. `PoolAssetPair` renders N legs; pool-type badge.
25. Participants empty state per E17.

**H. History gap**

26. Attempt the backwards reconstruction on the 79 gap pools; accept only pools
    whose walk lands on the first known snapshot exactly. The rest render null
    reserves before their first snapshot ledger, labelled.

**I. Records**

27. ADR for the two new tables and the shape-driven discovery rule.
28. `docs/architecture/**` — schema, read path, frontend contract.
29. `docs/backfills.md` — the three in-DB backfills, flavour A, no re-parse.

---

## Reserves come from ledger state, not from event arithmetic — 2026-08-21

Supersedes the reconstruction approach sketched earlier in this file. The
earlier design tried to predict how each event moved the reserves. That is now
unnecessary, and the measurements below are why.

### The finding

A pool's reserves are **stored on ledger**. Decoding the `TransactionMeta` of a
real swap (`a46f2c7f…4980`, ledger 64 052 779) shows a persistent
`ContractData` entry owned by `CCABO2IQ…JROY` — the Aquarius "pools plane" —
keyed `[Symbol("PoolData"), Address(pool)]`:

```
reserves  -> [1044176401956, 353830778]   identical to the announced values
pool_type -> "standard"
init_args -> [10]                          fee, basis points
```

The plane contract was **deployed at ledger 52 728 369**, before the first
Aquarius trade (52 728 694) and ~4.85 M ledgers before the first
`update_reserves` event (57 573 730). Its documented job is to be updated on
every pool action.

### Why this replaces the reconstruction

| Reconstruct from events                                                | Read the state                        |
| ---------------------------------------------------------------------- | ------------------------------------- |
| predict each event's effect on reserves                                | read the reserves                     |
| fee semantics per pool type **and per contract version**               | none                                  |
| ±1 per-event error compounding over 921 119 steps                      | independent snapshots                 |
| partial by nature — pools failing the zero-landing test keep "no data" | every pool, whole history             |
| indirect proof                                                         | the value the contract itself reports |

It also yields `pool_type` and the fee parameter from the same entry, so pool
metadata stops depending on event archaeology.

### What the abandoned path had already established

Kept because it is the evidence that the arithmetic route was a dead end, and
because two of the results stay useful:

- **Trade rule fitted per pool type against 91 181 clean single-event intervals:**
  constant `Δin = amount_in − fee`; stable `− ceil(fee/2)`; concentrated
  `− floor(fee/2)`. Out-leg is exactly `−amount_out` in **100 %** of cases,
  all types. Stable matched 20 688/20 688 exactly; concentrated
  31 362/31 510; constant only 23 436/38 983 exactly (rest off by 1).
- **The residual has a name.** Balance derived purely from CAP-67 token
  transfers minus the announced reserves equalled `get_protocol_fees()` **to
  the unit** (32 902 811) on the sampled pool. So
  `reserves = transfer-derived balance − accrued protocol fee`. Retained as a
  **cross-check**, not as a mechanism.
- **The oracle test that killed the approach.** Predicting each pool's accrued
  protocol fee from its whole event history and comparing with on-chain
  `get_protocol_fees()`: **6 of 49 pool-token cases exact**. Small misses are
  rounding (3, 11, 136 against balances in the billions); large ones are 12×
  and 30× and are **not** explained by fee claims — only one of the diverging
  pools has any `claim_protocol_fee` event at all. Most likely the `fee`
  field's meaning changed across contract versions, and pools were upgraded
  many times. A rule per contract version, with no published source, is not a
  foundation.

### Open question — must be settled by a pilot, not assumed

The pool interface exposes **`backfill_plane_data()`**. That function exists
for a reason: plane data was probably not populated for every pool from the
start. So "the plane was deployed early" does **not** prove "the plane carried
every pool's reserves from the start".

Settle it with a **pilot re-parse of a small slice** inside the gap window
(~10 k ledgers) and check whether `PoolData` changes appear for pools trading
in that slice. Cheap, and it decides whether the full re-parse is worth
running. Do not run the full pass first.

### Revised order

1. Parser extracts the plane's `ContractData` changes from the ledger entry
   change list — an extension of the existing `ContractData` handling that
   already reads token balances, not a new mechanism.
2. Verify on the live path: indexer-captured plane state vs `get_reserves()`
   on chain. Same comparison that already matched to the unit on three pools
   across all three pool types.
3. **Pilot re-parse** of a ~10 k-ledger slice inside the gap window; confirm
   `PoolData` entries are present there.
4. Only then the full re-parse of 52 728 369 → 57 573 730 (~4.85 M ledgers).
   Operator task, not an agent task. `repair-tier1` after any `--reindex` run
   is mandatory (`docs/backfills.md`), indexer stopped.

**Decided 2026-08-27: plane state is the single reserve source for the whole
timeline** — live and historical, one decode, no stitch at 57 573 730.
`update_reserves` events become a monitored cross-check (same announced
values; alarm on divergence; coverage from 57 573 730 onward — before that,
checkpoint snapshots are the only oracle). Events stay the source for volume,
where the amounts are read rather than inferred. The pilot behind this: 80/80
router-A pools had `PoolData` in their first trade ledger in both sampled gap
slices, and router B's pools sit in a second, deployment-own plane
(`CDWVENDO…WN5C`, 8/8) — so plane discovery keys on the
`[Symbol("PoolData"), Address(pool)]` shape, never a hard-coded address.

---

## Verification pass — 2026-08-26

Independent re-measurement of the findings above, prompted by an adversarial
review that assumed they were wrong. Each figure is a single-pass query over
`soroban_events` (10.3 G rows) with explicit deduplication. Two of the three
challenges were refuted by the data; the router counts in the first block do
need correcting.

### Correction: an earlier join was inflated by unmerged RMT rows

`soroban_contracts` carries duplicate rows — merges are healthy, the parts are
simply never collapsed to one. An `INNER JOIN` on it multiplies event rows by
roughly 4. Any count reached through that join is wrong by that factor unless
taken with `uniqExact` / `DISTINCT`. Everything below is deduplicated.

### Routers: ten, not two — but eight are dead

| Router          | Pools | Types registered               | First ledger | Last ledger |
| --------------- | ----: | ------------------------------ | -----------: | ----------: |
| `CBQDHNBF…6QUK` |   339 | constant, stable, concentrated |   52 728 530 |  64 119 240 |
| `CA7RQDMM…UOJQ` |    73 | constant, stable, concentrated |   52 902 613 |  63 997 027 |
| `CAZREK5U…IXVE` |    41 | constant, stable               |   52 085 052 |  52 699 385 |
| `CC2B3GFL…UQF7` |    13 | constant, stable               |   51 288 881 |  51 551 155 |
| `CANMWW5D…TTOD` |     8 | constant, stable               |   50 667 251 |  50 857 364 |
| `CCPHUHQY…I7SE` |     7 | constant, **elastic**          |   59 502 171 |  59 651 517 |
| `CDT6GQYR…57KM` |     6 | constant, stable               |   51 103 194 |  51 204 578 |
| `CDVTDAUA…T2VI` |     3 | constant                       |   50 667 038 |  50 667 042 |
| `CBVSLUYH…PWL3` |     3 | constant                       |   50 638 875 |  50 638 879 |
| `CALJOHJU…KLDN` |     3 | constant                       |   50 772 154 |  50 772 158 |

**496 registered pools, not 410.** The documented router holds **339, not 337**.

Of the 84 pools registered by the eight undocumented routers: 5 ever emitted
`update_reserves`, newest activity at ledger 60 697 845, **none** active in the
last million ledgers, 210 reserve events in total. Those eight are historically
dead, so a two-router scope loses nothing live — and loses those 5 pools and
210 events from a complete history, which is the standard this project holds
itself to.

**A fourth pool type exists.** `CCPHUHQY…I7SE` registers `elastic` alongside
`constant`. The earlier claim that no other variants exist across all history
is false as written. Any match on pool type must be total; `elastic` must not
land in a default arm.

### The registry is complete — zero orphans

|                                          |       |
| ---------------------------------------- | ----: |
| registered pools, all ten routers        |   496 |
| pools emitting `update_reserves`         |   373 |
| **emitting but registered by no router** | **0** |
| registered but never traded              |   123 |

Registry-driven discovery is sufficient, provided every router is found. The
shape-driven rule stated earlier is still the right rule — it is what surfaces
the ten — but the orphan arm (step A3) has no known work to do today. Keep it
as a monitored path, not as a load-bearing assumption.

Step A5 cannot establish this on its own: it diffs the table against the same
registries that filled it, so a pool no router registered would be invisible to
both sides. The zero above comes from the independent shape side
(`update_reserves` emitters). That is the comparison A5 should make.

### Concentrated pools: an adoption curve, not a sampling artefact

Trades per ~500 k-ledger window, by pool type:

| Window |  Trades | constant | stable | concentrated | % conc. |
| -----: | ------: | -------: | -----: | -----------: | ------: |
|    116 | 265 570 |  237 647 | 27 923 |            0 |       0 |
|    120 | 204 822 |  168 709 | 36 108 |            0 |       0 |
|    122 | 259 589 |  208 208 | 51 381 |            0 |       0 |
|    123 | 229 956 |  188 359 | 41 578 |           19 |       0 |
|    124 | 241 793 |  184 498 | 47 962 |        9 333 |     3.9 |
|    125 | 436 390 |  268 643 | 32 439 |      135 308 |    31.0 |
|    126 | 417 163 |  199 210 | 49 645 |      168 308 |    40.3 |
|    127 | 226 021 |   91 183 | 65 752 |       69 086 |    30.6 |
|    128 | 181 375 |   89 165 | 21 690 |       70 520 |    38.9 |

Zero to ~39 % in five windows, then a plateau at 30–40 %. `constant` falls in
absolute terms across the same span (237 k → 89 k), so concentrated is taking
flow rather than adding it. The "constant-only is not defensible" conclusion
holds, and if anything understates the case.

### Still unverified — settle before the participants work

Share-token coverage. The participants finding rests on a single pool matching
`get_total_shares()` exactly. How many constant pools have a share token
actually present in `assets` was not measured — the hourly read quota ran out.
Until it is measured, an unresolvable share token must render "not indexed";
an empty holder list is indistinguishable from a pool that genuinely has none.

---

## Schema + API review — 2026-08-27

An adversarial review of the two proposed table shapes and the API contract,
run before any DDL was written. Verdict: **ship with changes**. Three findings
were severe enough to invalidate the atomic steps as written; all were
re-verified independently before being accepted.

### 1. The proposed sort key would have deleted ~a quarter of every fact table

`ORDER BY (pool_id, ledger_sequence)` on a ReplacingMergeTree collapses rows
sharing that key, and without a version column the survivor is arbitrary.
Measured over the newest million ledgers:

| Event             |    Rows | Distinct `(pool, ledger)` |       Lost |
| ----------------- | ------: | ------------------------: | ---------: |
| `trade`           | 192 399 |                   146 768 | **23.7 %** |
| `update_reserves` | 194 230 |                   148 491 | **23.5 %** |

Up to 12 reserve updates land in one ledger for one pool. **Every fact table
here keys on `(pool_id, ledger_sequence, transaction_id, event_index)`** — the
shape `soroban_events` already uses. This is the same silent-loss class the
classic snapshots table has carried unnoticed.

### 2. Four tables, not two

Steps A1 and B6 named DDL for the registry and for reserves. Volume (step C)
and concentrated positions (step E) had none — they were written as if they
would insert into tables nobody had defined. Both need DDL in the same
operator session, with the key from finding 1.

### 3. Widening `pool_id` yields a valid-looking wrong address, not an error

`pool_id_hex_to_strkey` (`crates/api/src/common/strkey.rs:74`) wraps any
32-byte payload as a `LiquidityPool` strkey. A contract id is also 32 bytes, so
it passes the length assert and renders a **well-formed `L…` address for a pool
that does not exist** — no panic, no error. A `pool_kind` discriminator is
required, and `is_hex_pool_id` / `pool_id_from_text` need the same branch.

### 4. `pool_type` has two vocabularies in our own evidence

The router's `add_pool` says `constant`; the pool-state entry for the same pool
says `standard`. Since plane state is now the single reserve source (T4), one
column would collect both spellings. Resolved: a normalised enum plus the raw
string, with an uncatalogued spelling normalising to `None` and being counted.
**Implemented** — `domain::PoolType`.

### 5. The orphan arm can clobber a registry row

Under RMT, a stub written by the orphan arm (step A3) at a later ledger wins
over the real registration. Dormant today (zero orphans measured) but the
schema must not permit it: use `AggregatingMergeTree` with
`SimpleAggregateFunction(max/min)` per column — house precedent is `asset_sac`.
Add `last_activity_ledger` while there: the classic pool list has no usable
order key precisely because that column is missing.

### 6. The two-leg assumption is wider than the DTO, and `i64` overflows

Beyond `PoolItem`: `PoolEvent::from_signs(i64, i64)` is the whole
deposit/withdraw/trade classifier and would mislabel an imbalanced three-leg
deposit as a trade. `PoolActivityItem.amount_a: i64` cannot hold the
18-decimal leg already on chain (1.28e24 against an `i64::MAX` of 9.22e18).
Also `reserve_a`/`reserve_b`, the chart TVL formula, `fetch_pool_asset_ids ->
(i64, i64)`, and five frontend files. Step 21 covers only the DTO and must be
widened to the classifier and the amount types.

### 7. Empty-string `protocol` is a misleading fallback

Decided in T2 as "assert nothing", but an empty string is indistinguishable
from a missing filter value and renders as a blank rather than an absence. Use
`Option<String>` / SQL NULL, which says the same thing without pretending to be
a value. Same for `shares` becoming optional: `PoolParticipants.tsx:40` calls
`formatAmount(row.shares)` unconditionally and needs the null branch.

### Also noted

The registry moved from 496 to 497 pools during the review — the same live
drift that took router A from 339 to 340 earlier. Not a discrepancy; every
count in this file carries its measurement time.

`clickhouse` 0.15 round-trips `Array(Int128)` correctly, but no such column
exists in this database yet, so it warrants an integration test rather than an
assumption.
