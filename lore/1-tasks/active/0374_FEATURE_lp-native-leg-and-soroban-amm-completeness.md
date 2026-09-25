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
  - date: '2026-09-07'
    status: active
    who: karolkow
    note: >
      Pool write path DEPLOYED to production — release PR 452 merged
      (`098bef9d`), tag `production-2026.09.07-1`, one combined window with
      task 0540. `pool_state_changes` and `pool_instance_state` created and
      verified byte-identical to init.sql; both take live rows. No indexer
      pause was needed: the three `liquidity_pool_snapshots` columns the new
      writer drops were given DEFAULT NULL first (metadata-only), which makes
      old and new writers simultaneously valid, so the DROPs move to after the
      backfills and a rollback stays free. Backfills, closure layers and the
      read half remain.
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

- [x] native XLM leg matches → pools visible — F-B / K2-2 (fixed by 0440/0470 along the way; verified on prod 2026-08-29, see below)
- [ ] Soroswap pools indexed (reserves + volume) and unioned — K3-5 (next protocol, after the Aquarius deploy)
- [x] Aquarius pools indexed and unioned — K3-5 (code complete + verified; ships with the final-phase deploy/backfills)
- [x] Classic / Soroban filter on the pool list — K3-5 (filter[pool_kind] + FE dropdown)
- [x] share_percentage correct (or confirmed already correct) — K4-6 (confirmed correct; the real gap was holder coverage — snapshot-seed-lp built, [K] run pending)

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

6. `CREATE TABLE pool_state_changes` — pool id, ledger, `Array(Int128)`
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
column would collect both spellings. **Reversed 2026-08-27.** The decoder now
keeps only the raw spelling and normalises nothing: three vocabularies are live
at once (`constant` in the event, `standard` in pool state, `ConstantProduct`
in the contract's own enum), and asserting they mean the same thing is an
interpretation, not a decoding. It belongs wherever one vocabulary is actually
needed, not in the path every event crosses. Measured afterwards: `standard`
appears in **zero** of the 497 `add_pool` payloads, so the folding arm was
unreachable code justified by a source it could never see.

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

---

## Step 1b — what a slot is, and why the registry keys on the pool

Settled 2026-08-27 by measurement, because the DDL depends on it.

A registration carries a `subpool_salt`: the key the router addresses a pool by
within one token set. It is tempting to treat `(tokens, salt)` as the pool's
identity. **It is not.**

|                                                             |        |
| ----------------------------------------------------------- | -----: |
| registrations                                               |    497 |
| distinct pool addresses                                     |    497 |
| distinct salts                                              | **81** |
| slots (`tokens` + `salt`) registered more than once         |     18 |
| pools living in those slots                                 |     65 |
| of those, never active                                      |     29 |
| **slots with two pools active in the last million ledgers** |  **3** |

A slot gets re-pointed when a pool is redeployed, and one slot was re-pointed
seven times. The obvious simplification — "the current pool for a slot is its
newest registration" — is **false**: three slots have two pools trading at
once. So the salt is a plain attribute. **The pool contract address is the
identity**, and there it is clean: 497 registrations, 497 distinct addresses,
no pool registered twice.

## Step 1 — the DDL, and the two decisions inside it

### Three tables, not four

Concentrated positions get **no table**. They are 13 801 `position_update`
events across 26 pools, and the contract-id filter is a sort-key prefix, so a
live aggregation is cheap on both the detail page and the list. Recorded as a
decision rather than left as a gap.

### Why `AggregatingMergeTree` and not the usual `ReplacingMergeTree`

The registry has two writers with different authority: `add_pool` writes a full
row, and the orphan arm writes a stub for a contract that trades without a
registration. Under RMT versioned on a ledger the **stub wins**, because it is
written later by construction — and a fully-populated pool silently degrades to
no router, no type, no legs, with the good row deleted on merge. Dormant today
(zero orphans measured) which is the dangerous kind: it fires during a backfill
months from now.

With `SimpleAggregateFunction(max, …)` an empty value loses unconditionally, so
a stub can only ever _add_ information. House precedent is `asset_sac`.

This also settles the empty-vs-null argument for `protocol`. T2 chose an empty
string to assert nothing about an unidentified deployment; the schema review
argued for NULL as the less misleading value. The engine decides it: `max` over
`Nullable` does not give the "empty always loses" property, and an empty string
does. **Empty string, for a mechanical reason rather than a stylistic one.**

### The DDL — operator runs these

```sql
-- Registry. One row per pool contract; the salt is an attribute, not a key.
CREATE TABLE soroban_pools
(
    pool_id              Int64,
    pool_address         SimpleAggregateFunction(max, String),
    protocol             SimpleAggregateFunction(max, LowCardinality(String)),
    deployment_id        SimpleAggregateFunction(max, Int64),
    pool_type_raw        SimpleAggregateFunction(max, LowCardinality(String)),
    token_ids            SimpleAggregateFunction(max, Array(Int64)),
    subpool_salt         SimpleAggregateFunction(max, String),
    init_args            SimpleAggregateFunction(max, Array(String)),
    fee_bps              SimpleAggregateFunction(max, Int32),
    share_token_id       SimpleAggregateFunction(max, Int64),
    plane_id             SimpleAggregateFunction(max, Int64),
    first_seen_ledger    SimpleAggregateFunction(min, Int64),
    last_activity_ledger SimpleAggregateFunction(max, Int64)
)
ENGINE = AggregatingMergeTree
ORDER BY pool_id;

-- Reserves, read from pool-plane state. The key carries the transaction and
-- the intra-transaction index: (pool, ledger) alone collapses 23.5% of rows.
CREATE TABLE pool_state_changes
(
    pool_id          Int64,
    ledger_sequence  Int64,
    transaction_id   Int64,
    event_index      Int16,
    reserves         Array(Int128),
    plane_id         Int64
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 5000000)
ORDER BY (pool_id, ledger_sequence, transaction_id, event_index);

-- Volume. Same key discipline: (pool, ledger) alone collapses 23.7%.
CREATE TABLE soroban_pool_trades
(
    pool_id          Int64,
    ledger_sequence  Int64,
    transaction_id   Int64,
    event_index      Int16,
    token_in_id      Int64,
    token_out_id     Int64,
    amount_in        Int128,
    amount_out       Int128,
    fee              Int128,
    caller_id        Int64
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 5000000)
ORDER BY (pool_id, ledger_sequence, transaction_id, event_index);
```

### Column notes worth keeping

- **`init_args Array(String)`** — raw and unparsed, because the list is three
  vocabularies wearing one shape (`[u32]`, `[u32, i32]`, `[u32, u128]`,
  `[u32, u128, u32]`). Position 1 is a tick spacing in one pool type and an
  amplification factor in another, and `u128` does not fit `i64`.
- **`fee_bps`** is the one safe extraction: position 0 is a `u32` fee in all
  eight measured shapes.
- **`pool_type_raw`** only. No normalised twin: three spellings exist across
  three sources, and deciding they mean the same thing is an interpretation
  that belongs where one vocabulary is actually needed.
- **`caller_id`, not `trader_id`** — topic 4 of `trade` is the caller, which on
  router-mediated swaps is the router.
- **`plane_id`** on both tables — planes are per deployment, so the pool's
  reserve source is not a constant.
- **`last_activity_ledger`** — the classic list has no usable order key
  precisely because this column is missing there; deriving it later would mean
  a full snapshot `GROUP BY`.

## Committed follow-through — the pair columns die (decided 2026-08-28)

Not deferred-as-in-maybe: the end state is **`legs` as the only leg source in
both worlds**, and the six pair-shaped columns
(`asset_a_type/code/issuer_id`, `asset_b_*`) **dropped**. Sequence, each step
with its own verifier:

1. resolve whether our `hash64` surrogate == ClickHouse `cityHash64`
   (one sample comparison decides SQL-mutation vs Rust job),
2. backfill `legs` for the 52,620 classic rows,
3. migrate the ~612 pair-shaped call sites (API queries, classifier,
   frontend) to `legs` — the largest piece, behind a read-time coalesce
   bridge so it can land incrementally,
4. `ALTER TABLE liquidity_pools DROP COLUMN asset_a_type, …` (operator) +
   drop from `init.sql`, the row struct, and the column-order guard together
   — the guard test (`column_order_liquidity_pools`) enforces that these
   three can only move in lockstep.

`init.sql` cannot lose the columns earlier: the driver validates inserts
against the live table (0310), so the struct must keep the fields until the
prod columns are dropped, and the guard pins struct ↔ init.sql.

## Step 18 amended — positions are materialized, not query-time (2026-08-28)

Prior-art sweep (wayfinder `answers/R-position-model-prior-art.md`): the
one-Position-entity-with-nullable-ticks API shape is exactly Messari's
standardized DEX-AMM schema, so it stays. But NO online-serving system folds
positions from raw events at query time — everything materializes. At 13 801
events the fold is cheap today; the design still diverges from every system
that serves this. Amended: a rebuildable projection, fed by a materialized
view over the events already ingested — no new ingest code.

Operator DDL (runs together with the still-pending registry ALTER):

```sql
CREATE TABLE soroban_pool_positions
(
    pool_id   Int64,
    holder_id Int64,
    tick_lo   Int32,
    tick_hi   Int32,
    liquidity SimpleAggregateFunction(sum, Int128),
    last_updated_ledger SimpleAggregateFunction(max, Int64)
)
ENGINE = AggregatingMergeTree
ORDER BY (pool_id, holder_id, tick_lo, tick_hi);
```

(The MV definition over `soroban_events` follows with the step-18
implementation; the table is rebuildable from the events at any time, so a
wrong projection is a re-fill, never a re-parse.)

The participants DTO gains an explicit position-kind discriminator rather
than inferring fungible-vs-ranged from null ticks (research amendment 2).

---

## DEPTH-FIRST — the governing principle from here on (Karol, 2026-08-28)

One protocol at a time, one deployment at a time, built end-to-end. Analysis
may look wide; **building may not**. Nothing lands "because the next protocol
will need it" — the next protocol updates the model when its turn comes, and
the diff then shows exactly what differed. 0516 remains analysis and ground
rules only, never a shared build.

This reversed a same-day drift. The registry had grown to nine columns, a
deployments dimension and a positions projection — cut back to what the first
protocol needs:

**The one DDL that exists now** (replaces every earlier DDL block in this
file; none of the earlier ones ever reached production):

```sql
ALTER TABLE liquidity_pools
    ADD COLUMN pool_kind      UInt8                  DEFAULT 0,
    ADD COLUMN legs           Array(Int64)           DEFAULT [],
    ADD COLUMN deployment_id  Int64                  DEFAULT 0,
    ADD COLUMN pool_type_raw  LowCardinality(String) DEFAULT '',
    ADD COLUMN share_token_id Int64                  DEFAULT 0;
```

Why each of the five is needed by Aquarius alone: `pool_kind` renders the id
honestly (a contract payload otherwise renders as a valid-looking wrong L…
strkey); `legs` because Aquarius stable pools carry 3 and 4 legs today;
`deployment_id` because two live routers share Aquarius's code and only one
is Aquarius (T1) — and with the venue label resolved from this id at read
time (a reviewed constant in the API, not a stored string), a new pool is
labelled the moment it registers, which kills both the editorial-UPDATE drift
and the label column; `pool_type_raw` gates the participants path (fungible
vs concentrated) and the meaning of the tick parameter; `share_token_id`
feeds the fungible participants read (T6).

**Cut, with the reason recorded:**

- `subpool_salt`, `init_args` — registration provenance; the `add_pool` event
  sits complete and forever in `soroban_events`. Extract on demand, never
  copy. (`fee_bps` is the one operational value, already extracted.)
- `protocol` column — replaced by read-time resolution from `deployment_id`;
  a stored label needs a re-run editorial UPDATE for every new pool and
  drifts in between.
- a `soroban_pool_registry` subtype side table (moving
  `deployment_id`/`pool_type_raw`/`share_token_id` out of the wide row) —
  textbook-cleaner, but it pays a JOIN on the hottest path (the union list
  with type/deployment filters), ClickHouse's own guidance is to denormalise,
  and the table already carries classic-only columns the same way (`assets`
  does too: `contract_id`=0 on classic rows, empty code/issuer on soroban —
  the discriminator says which columns apply). **Revisit at the legs
  migration**, when the six classic-only columns drop and the JOIN cost can
  be measured on the real list.
- `amm_deployments` table — a good idea _when a second protocol arrives_;
  today it would hold two rows serving one JOIN. Revisit at the Phoenix
  adapter, not before.
- `soroban_pool_positions` + its MV — concentrated participants are LAST in
  Aquarius scope (Karol). The prior-art verdict (materialize, explicit
  position-kind discriminator) is recorded in T5 and stands for whenever
  that work starts; nothing is built until then.

## Step-4 ordering rule (Karol caught this, 2026-08-28)

**The backfill runs ONLY after the step-3 writer is live on production, and
the corpus is re-harvested at that moment.** Order: deploy writer → harvest
`add_pool` corpus (now covering everything up to the writer's start) →
generate → insert. A pool registering between an early insert and the deploy
would be silently missing; the reverse overlap is harmless because the RMT
key (`pool_id`, version `last_updated_ledger`) makes the insert idempotent.
The generated file is disposable — one `chq` + one `cargo run` recreates it
in seconds, so nothing is "saved" by inserting early.

## Step-15 design constraint — filling `share_token_id` must not clobber (Karol caught this, 2026-08-28)

The registry is RMT: merges keep whole rows, so a later partial row wins whole.
A deposit-path writer that knows ONLY the share token cannot write a registry
row — it would replace legs/type/deployment with defaults. The three honest
options, in house terms:

1. **Side table, the `asset_sac` pattern** (different writer, different
   rhythm → side table with column-merge semantics): a tiny
   `pool_share_tokens (pool_id, share_token_id, derived_at_ledger)` RMT keyed
   on `pool_id`, written by the deposit path which has both values in hand;
   reads join ~500 rows. The `share_token_id` column added by the ALTER then
   stays unwritten and is dropped at the legs migration. **Recommended.**
2. In-DB periodic backfill that reconstructs FULL rows
   (`INSERT … SELECT` joining the current row + the derivation) — no clobber,
   but the token appears with batch delay and the job is a new moving part.
3. Read-modify-write in the deposit path — racy against concurrent
   registration updates and already rejected as a pattern in this project.

Decide at step 15; nothing writes the column until then, so today it is
harmless either way.

## `fee_bps` silent-zero (Karol, 2026-08-28)

`init_args[0].parse().unwrap_or(0)` turns an unparseable fee into a plausible
0 — the misleading-fallback class. Bounded today: the corpus test pins all
eight mainnet shapes (u32 at position 0 in every one), so a silent zero can
only come from a FUTURE shape. Amendment for step 15's commit train: warn in
`pool_registry_row` when position 0 is absent or unparseable, so the zero is
always accompanied by an alarm-visible log line.

## Final-phase step: prove the deploy window is closed (Karol, 2026-08-28)

After the writer deploys and the catch-up backfill re-runs, one read-only
check proves no registration slipped between the working backfill and the
writer's start — the gap the batched-deploy plan makes possible:

```sql
SELECT
  (SELECT uniqExact(JSONExtractString(data_xdr,'value',1,'value'))
     FROM soroban_events WHERE signature = 'add_pool')      AS zdarzen,
  (SELECT uniqExact(pool_id) FROM liquidity_pools
     WHERE pool_kind = 1)                                   AS w_rejestrze,
  zdarzen - w_rejestrze                                     AS brakuje  -- MUSI byc 0
```

Non-zero `brakuje` = re-run the generator + insert (idempotent) and re-check.

**Amendment (three-lens review, 2026-09-01):** the generator CANNOT perform
the router corroboration the live writer does (instance storage is not in
`soroban_events`), so before its insert the backfill session must (a) re-run
the "no pool registered twice" measurement over `add_pool` — a duplicate
registration naming an already-registered pool would beat the genuine row on
RMT merge and the closure check above (uniqExact) is blind to it — and
(b) refuse duplicates in the generator (one GROUP BY). Zero duplicates
measured 2026-09-01; this guards the window between that measurement and the
writer's start.
The generator itself is deliberately NOT in the tree (a one-off is not a
maintained surface — Karol, 2026-08-28); resurrect it verbatim for the final
phase with:
`git show 082ee364:crates/db-clickhouse/src/bin/gen_pool_registry_backfill.rs`
This lands in the T7 production checklist, not only here.

## Step-15 fundamental correction — the chain stores the relation as STATE (2026-08-28)

Karol pushed past "which table" to "where does the truth live". Probed on raw
ledger meta (registration ledger 63 893 403, pool `CBMWU357…`):

**The pool's own instance storage, written IN THE SAME TRANSACTION as
`add_pool`, carries the whole registration as state:**

```
TokenShare  = CC5PU23M…   ← the share token, at birth
Plane       = CCABO2IQ…      TokenA/TokenB, FeeFraction,
Router      = CBQDHNBF…      ReserveA/ReserveB, TotalShares, …
```

And a concentrated pool's instance (probed at 64 134 576) has **no
`TokenShare` key at all** — instead `Slot0`, `TickSpacing`, `Liquidity`,
`FeeGrowthGlobal…`. The absence is structural, matching `share_id()`
returning the pool itself.

### Revised design — T4's rule applied to its own conclusion

- **Source: instance-storage `TokenShare`** (state), not the deposit⇄mint
  correlation. Available at the creation ledger and, for the 13 measured
  migrations, as later instance-update entries. "State over inference" — the
  same reasoning that decided reserves.
- **Store: the `pool_share_tokens` side table stands** — the relation is
  mutable and the registry is whole-row RMT, so the structural argument is
  untouched; only the WRITER changes.
- **The T6 deposit-mint detector is demoted to a monitored cross-check** —
  exactly `update_reserves`' fate in T4. Its 16/16 on-chain verification is
  what makes it a trustworthy alarm.
- **Implementation folds into step 7**: the instance-entry arm is the same
  ContractData extraction family as the plane read, so share tokens, plane_id
  and reserve corroboration ride one pathway. Before implementing, probe one
  EARLY-era pool (52.7M) for storage-layout drift across contract versions.

### Anchor upgrade (2026-08-28): shares-first is the vendor's own word

The 0008 capture of `liquidity_pool_events` carries the impl comment for
`deposit_liquidity`: body `[stake_amount, amountA, amountB, amountC]` —
shares FIRST, in the vendor's own source. The claim previously stood on
measurement alone (75 200/75 200); it now stands on source + measurement.

## Step-7 deep verification (2026-08-29) — and one layout discovery

79 raw mainnet ledgers through the compiled pipeline (`pool_state_real_corpus`):
both T3 pilot slices plus 11 registration ledgers picked across every era and
type. Results:

- **267 plane writes, 269 pool instances, zero PoolData-shaped entries the
  parser refused** — the silent-loss invariant held everywhere.
- **The recorded early-era probe PASSES**: dead-deployment registrations
  (50.6M) and the first Aquarius pools (52.7M) parse with the same storage
  keys as today's. No layout drift across contract versions.
- **TokenShare presence matches pool type 269/269**: fungible (constant 220,
  stable 44, elastic 1) → present; concentrated (4) → absent. Elastic carries
  a share token — consistent with its deposits minting.
- **Dual-source oracle** (plane state vs `update_reserves` events, the
  event-era overlap): every comparable row agrees on the reserve values.

**Discovery: a concentrated pool's plane `reserves` vector is reserves AS A
PREFIX plus per-tick state in the tail** (measured: `[r0, r1, 0…]` and
`[r0, r1, tick liquidity…]` vs the event's exact `[r0, r1]`). The snapshot
writer stores the vector RAW (state verbatim); reads slice by the pool's leg
count. Nothing may treat vector length as leg count.

## T4 refined by the anti-test (2026-08-29): concentrated reserves ride the INSTANCE

The bidirectional set-equality anti-test (extracted state vs `update_reserves`
events, per ledger, both directions) caught a real architecture gap: **a
concentrated pool does NOT update the plane per operation.** Probed raw on a
hot ledger: 8 instance rewrites (`Reserve0`/`Reserve1`, `TickChunk`s), zero
plane writes for the busiest concentrated pool. The plane gets their
`PoolData` at registration only. T3's 80/80 pilot could not see this — the
first concentrated pool registered AFTER the gap era the pilot sampled; the
one-source claim was a generalization beyond its sample.

Refinement (implemented): reserves source is per pool layout —
**fungible → plane `PoolData`; concentrated → instance `Reserve0/Reserve1`**
(same extraction family, same snapshot table, `plane_id` still carried). The
fungible instance's `ReserveA/B` mirror is deliberately NOT staged (the plane
is their source; staging both would duplicate rows) — it remains a
corroborator.

Final verification state for step 7:

- 90 raw ledgers through the compiled pipeline; zero shaped-but-unparseable;
- raw registration ledger → full STAGING → exact rows (values, change_index,
  plane_id, share relation);
- anti-test: **zero foreign contracts captured** (instances outside the
  registry: 0; the only set-differences are registrations, where the plane is
  created before any event exists);
- **zero pools missed** after the refinement;
- last-write-per-ledger reserves equal the last `update_reserves` event
  **17/17** in the event-era overlap;
- TokenShare-vs-type: 285/285 instances consistent (fungible → present,
  concentrated → absent).

## Why snapshots stay TWO tables while pools became one (Karol challenged, 2026-08-29)

Dimensional rule: unify by entity when the GRAIN matches; separate facts when
it differs. The pool dimension shared its grain (one row = one pool). The
snapshot facts do not: classic = one deterministic row per (pool, ledger)
(the 0356 LIMIT-1/no-FINAL invariant depends on it); soroban = one row per
state write, up to 12/ledger (collapsing loses 23.5%). Value models are
disjoint too (Decimal(38,7) + write-time analytics vs verbatim Array(Int128)).
Unifying would need a sort-key change = a 322M-row rebuild, and — the
clincher — **no cross-kind snapshot read exists**: charts and cross-checks
are per pool, and the pool's kind is known from the registry first. The
union cost that justified one dimension table does not exist for these facts.
Considered and rejected: write-side collapse to classic grain (loses real
intermediate states; adds a kind-conditional invariant to a versionless RMT).
Revisit trigger: a query that genuinely needs a cross-kind snapshot union.

### Reframed after the greenfield pass (2026-08-29): target + parallel legacy model

Corrected after a devil's-advocate pass (senior-principal framing) — the
first draft of this section had three real errors:

**Greenfield is TWO fact tables, not one.** A pure state table cannot
reconstruct volume: a reserve delta does not distinguish a trade from a
deposit/withdrawal — only the indexer, seeing the operation, can attribute
it. That is exactly what `gross_volume_a` encodes today (read-time tvl /
volume / fee_revenue in `queries.rs` are all derived from indexer-written
inputs). So `gross_volume_a` is a FACT living in the wrong table, not an
accretion. From scratch:

- `pool_state_changes(pool_id, ledger, tx, change_index, reserves Array(Int128), total_shares)`
  — verbatim chain state at chain grain, both families. `total_shares`
  BELONGS here for classic: it is a field of the same `LiquidityPoolEntry`
  (same source, same write, same grain); for Soroban the column is absent /
  from its own source. The first draft wrongly exiled it.
- `pool_trades(pool_id, ledger, tx, …, gross_amounts)` — trade attribution
  fact (Soroban already has this via events; classic's lives fused into the
  snapshot row as `gross_volume_a`).

Real accretions remain: write-time Decimal(38,7) scaling and the collapsed
per-ledger grain (an 0356 write-side choice, not chain shape).

**`pool_state_changes` matches the target state-fact shape**, so the two
current tables are the target model + a PARALLEL legacy-shaped model — not
peers-forever. Deliberately NOT labelled "legacy to migrate" and no
migration task filed (no-speculative-backlog rule): unification is
trigger-gated. Triggers: first consumer needing one cross-family history
read; or repair-tier1 cost outweighing a rebuild (its 12 MIN-semantics
columns are mostly the fused accretions and would retire). Until then every
pool-history endpoint carries a per-kind branch — a real, named, ongoing
cost, accepted.

**No in-band sentinels on import.** If classic history ever imports at
ledger grain, it must carry an explicit source discriminator (nullable
tx/change or a `source` column), never `tx=0/change=0` — magic zeros are the
same bug class as the native empty-string convention that hid 21.7% of
pools.

Step 7 code is unaffected by all of the above.

**Rename (2026-08-29, pre-DDL so free):** `soroban_pool_snapshots` →
`pool_state_changes` (struct `PoolStateChangeRow`). The only decision from the
greenfield pass that would have forced later churn was the NAME — after a
unification the family prefix lies. Everything else (classic migration,
`pool_trades` split, `total_shares`/`source` columns) is additive later, so
deferred. Historical mentions above read as the new name.

## Steps 8/9 — formally closed by the deep-testing record (2026-08-29)

The step-7 verification above IS the reserve verification: 90-ledger raw
corpus through the compiled pipeline (0 shaped-unparseable), raw-ledger →
staged-rows e2e with exact values, and the bidirectional anti-test against
`update_reserves` (missing 0, foreign 0, TokenShare-vs-type 285/285,
last-write-per-ledger values 17/17). The standing cross-check monitor query
(extraction vs events per ledger) lives in that section; run it after any
parser change touching pool state.

## Steps 16-20 — read path, frontend, docs (2026-08-29, one session)

- **API** (steps 16-17-19): list + detail publish `pool_kind` / `protocol`
  (verified-operator label, read-time resolve from `deployment_id`) /
  `pool_type` / `legs[]` (resolver live-verified on prod: both arms, 8/8
  legs incl. an 18-decimals bespoke token); pair fields null on soroban
  rows; `participant_count` null (≠ 0) where the population is wrong.
  Participants endpoint branches: soroban = share-token holders from
  `balances` (asset-direction full scan measured 121.5M rows / 71 ms; skip
  index on `asset_id` is the [K] upgrade if hot), shares scaled by metadata
  decimals or null, percentage scale-free, cursor rides the raw value.
  Chart + activity REFUSE soroban pools explicitly (400 with reason) —
  never a confidently-empty series. Pool routes accept `L...` AND `C...`
  (a soroban pool's id bytes are a contract payload; an `L...` render would
  be well-formed and WRONG). `pool_exists` retired — every gate needs the
  kind now. OpenAPI + `@rumblefish/api-types` regenerated.
- **Frontend** (step 18): one `poolLegViews` model renders both worlds
  (classic pair expands, soroban `legs[]` scale raw reserves by on-chain
  decimals — unknown scale renders as absent, never raw). List + detail
  render 2-4 legs, protocol chip (verified only), classic-only sections
  (chart/activity) not mounted for soroban. web tests 305/305, typecheck
  and lint clean (3 pre-existing warnings elsewhere).
- **Docs** (step 20): ADR 0058 (discovery shape-first + label-as-attribution;
  one dimension two id worlds; state facts at chain grain; share relation as
  side table; explicit refusals) + database-schema / indexing-pipeline /
  xdr-parsing overviews updated per ADR 0032.

Deferred within scope, still ahead: [K] DDLs, deploy, backfills, gap
re-parse, deploy-window closure check, registry post-backfill audit (old
step 6), concentrated positions (step 23, LAST). Local full-stack visual
verification of the FE against prod CH belongs to the final phase.

## K4-6 measured (2026-08-29): share_percentage is NOT stale — lp_positions coverage is the real gap

Method: per pool, sum of deduped positive `lp_positions` shares vs the
latest snapshot `total_shares` (323.7M rows read, 0.7 s), buckets on the
mismatches, then RAW-XDR chain validation via `getLedgerEntries` (the
arbiter — Horizon untouched).

Findings:

1. **The percentage's denominator is chain-exact.** Two live sampled pools:
   snapshot `total_shares` == the on-chain `LiquidityPoolEntry` value to the
   stroop. Listed holders' percentages are CORRECT. K4-6's "stale
   share_percentage" is refuted as stated.
2. **Zero overcounts.** No pool has positions summing above the chain total
   — no stale-high rows anywhere.
3. **The real defect: missing holders.** 24,920/26,271 pools agree exactly;
   the rest under-sum. 2,681 pools know <50% of the shares' owners (1,164 of
   them LIVE — fresh snapshots), 597 miss 1-50%, 141 drift <1%.
   Chain-validated: pool `0d8a4b61…` has 2 trustlines on chain, we know 0;
   `8e08ca7a…` has 3, we know 2.
4. **Root cause: the ingest floor.** The worst pools' first snapshot is
   exactly L50,458,12x (the frozen backfill floor); a holder whose pool-share
   trustline predates the floor and was never touched since has no
   `lp_positions` row. Same class as every other pre-floor state gap.
   `pos_newer = 0` everywhere — recency is not a factor.

User-visible harm: participants lists are incomplete and
`participant_count` undercounts (2 shown where the chain says 3);
percentages shown are right but do not sum to 100 on affected pools.

Fix direction (not yet decided): pool-share trustlines are ordinary ledger
entries, so a checkpoint snapshot carries ALL of them — a one-shot seed of
the missing `lp_positions` rows from a current checkpoint (the 0457
snapshot toolchain) closes the gap completely, with `first_deposit_ledger`
explicitly unknown for seeded rows (never a fabricated value).

## K4-6 fix MOVED OFF THIS BRANCH (decision karolkow 2026-08-29)

The seed was built here first (commits `a3bc8e63` + `cee04d5a`, reverted in
place — resurrect from those SHAs), then pulled out: it touches ONLY the
CLASSIC world (`LiquidityPoolEntry` + pool-share trustlines are classic
entry types; the soroban-AMM era starts AFTER the ingest floor — first
`add_pool` at L50,638,875 vs floor 50,457,424 — so soroban pools need a
re-parse, never a checkpoint), and this branch ships Aquarius. The work —
folded into the ONE `snapshot-seed` flow, not a side subcommand (same
decision) — now lives in task 0523 (re-scoped), together with the 0468
"Since ledger 0" display bug it half-fixes (94.8% of positions already
carry the 0 sentinel in prod). The K4-6 MEASUREMENT above stands.

## F-B / K2-2 closed by verification (2026-08-29)

The native-leg filter defect was FIXED ALONG THE WAY by tasks 0440/0470:
`asset_codes_predicate` matches native legs by `if(asset_*_type = 0, 'XLM',
asset_*_code)` (one shared definition for the pools list and global search,
unit-tested as load-bearing). Verified on prod, deduped: 11,734 classic
pools carry a native leg (22.3% of 52,663) and EVERY one matches the `XLM`
filter — the formerly invisible set in full. The filter also substring-hits
3,270 pools whose credit code merely contains "XLM" (`yXLM`, `XLMFISH`, …)
— the documented substring semantics every code shares (`USD` matches USDC),
called out on the DTO. Deliberately NOT built: an exact-native-only hatch
(the 0359 "surrogate or type=native" suggestion) — no requester since, and
the per-leg exact mode plus the L/C-id point-select cover the precise cases.
The scope line "in LP snapshots" in this task's summary was loose wording:
the defect lived in the list/search FILTER, and snapshots join by pool_id,
never by leg code.

## Full-stack e2e on real data (2026-08-30) — two shipping-blockers caught, then everything exact

Environment: raw mainnet ledgers (public bucket) → the REAL backfill-runner
binary → local dockerised ClickHouse (full init.sql) → the real API binary
(`bin/local`, new plain-CH branch) → the real SPA in a real browser.
Windows: hot era 64,132,000-200, concentrated-registration era
64,134,500-699, near-tip 64,190,000-64,191,999 (2,401 ledgers, 174k events).

### Bugs the pipeline caught (both invisible to every unit test)

1. **Writer dropped BOTH new tables' rows silently.**
   `PartitionWriter::commit()` hand-lists the inserts to `end()` —
   `pool_share_tokens` and `pool_state_changes` were streamed by
   `write_ledger` but never ended, so their buffered rows vanished on drop
   while the `ledgers` marker still landed. Live indexer shares the path: a
   deploy would have shipped two silently empty tables. Fix: exhaustive
   destructure of `TableInserts` in `commit()` — a future field refuses to
   compile until someone decides where it drains.
2. **The sort key had no intra-ledger order.** `transaction_id` is a hash
   surrogate and sorts randomly; "latest reserves" picked an INTERMEDIATE
   write on 127 of 1,410 real (pool, ledger) pairs (192 pairs carry >1 row).
   Fix (free — prod DDL not yet run): new column `application_order` (tx
   position in its ledger), key becomes
   `(pool, ledger, application_order, change_index)`; `transaction_id`
   stays as a join attribute. API `argMax` follows. Corollary found during
   verification: `soroban_events.event_index` is per-TRANSACTION, so any
   "last event in ledger" comparison must order by
   `(tx application_order, event_index)` — the earlier 17/17 prod check
   passed only because its pairs were single-tx.

### Results after the fixes

- Registry vs vendor API: 248/248 catalogued pools present, pool_type 0
  mismatches under the vocab map, fee 0/248; our 92 extra = dead pools
  (91/92 event-silent since L63M) — full history vs their live subset.
- Full-pipeline bidirectional anti-test vs `update_reserves`:
  **missing 0, values 1,414/1,414 exact** (extras: 1 non-trade write).
- Reserves vs LIVE chain (`getLedgerEntries` at each entry's own
  lastModified): **26/26 exact** (88 pools moved past the windows —
  untestable by construction).
- Share tokens vs live chain `TokenShare`: **92/92** surrogate-exact.
- Live-chain architecture confirmation: concentrated instances carry
  `Reserve0/1` and no `TokenShare`; fungible the reverse, reserves in plane
  `PoolData` (`pool_type: "standard"` — third vocabulary observed live).
- API battery on real data: union list + legs + protocol label + verbatim
  pool_type; explicit 400s (participants-concentrated, chart, activity, bad
  filter value, each with reasons); C-id and L-id routing; absent-valid ids
  404; malformed 400; cursor walk 3 pages no dups and back-page equality;
  XLM filter behaves per F-B.
- Playwright (real data, no mocks; `POOLS_REAL`-gated spec):
  **3/3** — union list row with legs+chip, soroban detail with per-leg
  reserves and classic-only sections unmounted, classic detail intact.

Not covered locally (honest gaps): fungible-pool participants through the
API (no fungible pool REGISTERS inside the windows, so no registry row to
gate on — the SQL path is measured on prod and unit-covered); prices/TVL
(null by design in the local stack).

## Correction (2026-08-30): the classic-legs backfill is a RUST pass, not SQL

The legs-step-2 commit message promised "a cheap in-DB INSERT-SELECT" for
the 52,620 existing classic rows. Wrong — and the legs plan's own step 1
already asked the deciding question: the leg surrogate is our
`cityhash_102_128` low half, and ClickHouse's builtin `cityHash64` is a
DIFFERENT algorithm (ids.rs header), so SQL cannot compute it. The backfill
is a small one-shot Rust pass (read the pair columns, emit rows with legs
filled — versioned on each row's own last_updated_ledger), [K] in the
deploy window alongside the other catch-ups.

## Grain reversal (2026-08-30, decision karolkow): one row per (pool, ledger)

Karol challenged the per-write grain ("czy nie da się jak w klasyce?") and
the re-audit agreed: every consumer that mattered (anti-tests, API latest,
charts) works at ledger grain, and intra-ledger history is permanently
reconstructible from `soroban_events` — the stored intermediates duplicated
it. Collapsed at parse time in apply order (`dedup_final_plane_writes` /
`_pool_instances`, twins of the classic dedup); schema simplified to
`(pool_id, ledger_sequence, reserves, plane_id)`, `application_order` and
the tx columns gone from the table (the ordering-bug class dies with them);
API argMax by ledger alone. Free change — production DDL still did not
exist. Bonus: both worlds now share grain AND mechanism, so the distillation
unification becomes a plain union.

Re-verified end-to-end on the real windows after the collapse: rows==pairs
1,415/1,415 (structural one-per-pair), anti-test values 1,414/1,414,
missing 0, live-chain spot checks 10/10 (the rest moved past the windows),
Playwright 3/3. ADR 0058 §3 rewritten to record the reversal.

## Chart + activity for soroban pools (2026-08-30, decision karolkow: "sprawdź lepiej" → build)

The §5 refusals for the chart and activity feeds did not survive a re-audit:
both are buildable from data already indexed, no new tables.

- **Activity**: the pool's own `trade` / `deposit_liquidity` /
  `withdraw_liquidity` events in `soroban_events` — the table is keyed
  `(contract_id, ledger, transaction_id, event_index)`, so the per-pool page
  is a leading-PK seek. New `leg_amounts[]` on the wire (per-leg signed RAW
  units + `leg_index`; FE scales by each leg's on-chain decimals through the
  unified leg views). Every read `LIMIT 1 BY` (RMT dedup). The actor is the
  TRANSACTION source, never the event's trader topic — measured: routed
  trades put the ROUTER contract there. Cursor gains `event_index`
  (`#[serde(default)]` keeps old classic cursors valid).
- **Chart**: reserves from `pool_state_changes` + the pool's own trade
  events + the SAME prices series the classic chart joins (SAC legs price
  via their classic identities; bespoke tokens under `asset_kind =
'contract'`). Folded in Rust: reserve carry unbounded, price carry ≤ 48 h,
  a bucket with an unpriceable trade reports null volume (house rule — no
  partial sums). Leg ↔ token matching is BY SURROGATE against the registry's
  `legs`, never via the contracts dimension (may lack rows locally).
- Hand-verified exact on the local real-data stack with a stubbed prices DB:
  bucket 08:00 tvl 87,711.33 / volume 204.59 / fee 0.61 (fee_bps = 30) —
  each recomputed by hand from raw reserves × stub closes. Playwright 3/3
  (list, soroban detail with chart tab + Trade rows + linked leg labels,
  classic regression).
- FE: `AmountLegPart` refactored to label/href parts (classic arm keeps
  `assetLegLabel`/`legHref`; soroban arm reads `leg_amounts` through
  `poolLegViews`); the detail-page unmount gates for charts + activity are
  gone. ADR 0058 §5 rewritten (one refusal remains: participants on a pool
  with no known share token).

Finding, out of scope here: for that remaining participants 400 the FE
renders the GENERIC "Something went wrong" retry state — misleading for a
deliberate refusal; it should surface the explanatory message.

## Simplify pass on the soroban feeds (2026-08-31, /simplify — 4 review agents)

Karol challenged the volume of new API code; the four-angle review agreed
and one reuse miss was PROD-BREAKING: the soroban close-series read named
`prices.price_usd_series_1d`, a view that exists only in the local stub —
prod's daily view is `prices.price_usd_series` (verified via chq). The 1d/1w
chart would have 500'd in production. Fixed by extracting the classic
chart's interval→view mapping into `chart_price_series()` shared by both.

Applied (all verified value-identical on the local stack after the rewrite —
bucket 08:00 still tvl 87,711.33 / volume 204.59 / fee 0.61 exact):

- Chart inputs pre-aggregated in CH: reserves argMax per bucket + one seed
  row (pre-window history never crosses the wire), trades summed per
  (bucket, token) with a `bad` parse-failure count so the null-poison rule
  survives aggregation. The Rust fold now only prices and carries — the
  O(buckets × trades) rescan, the per-call linear close scans, and the
  dense-null bucket grid are gone. Output is SPARSE calendar buckets via
  the classic `toStartOfHour/Day/toMonday` grain (the epoch-aligned weeks
  landed on Thursdays; both arms of the endpoint now share density,
  alignment, and the in-progress-price-bucket guard).
- Three chart inputs fetched with `futures::try_join!` (were sequential).
- Chart handler: `PoolPriceContext` now carries `legs`, dropping the second
  `liquidity_pools` seek; both arms fall through one response tail.
- Activity: 3-way OR keyset → house tuple comparison; tx enrichment
  extracted to `fetch_activity_txs()` shared with the classic feed (whose
  copy had also lost the BTreeSet dedup); dead always-`None` `actor` slot
  removed; deposit arm's mut-flag loop → zip + `collect::<Option<_>>`.
- `fetch_pool_asset_ids` → `fetch_pool_feed` returning
  `enum PoolFeed { Classic, Soroban }` — the `unreachable!` in the handler
  and the garbage-surrogates-on-soroban footgun are unrepresentable now.
- `SorobanChartLeg` reuses `PriceLeg` via `enum ChartPriceId`; interval
  tables deduped.
- Wire: `application_order` is now NULLABLE on activity items — the `0`
  sentinel built dangling `#op-0` links and collided FE row keys when one
  tx emitted several events. Soroban rows link plain `/transactions/<hash>`.
- FE: `poolAmountLegs` collapsed to ONE arm for both worlds — rows
  normalize to `(leg index, signed raw amount)` and `PoolLegView` (now
  carrying `decimals`; classic = 7) supplies label/href/scale uniformly.
  `tradeRate` divides SCALED values (raw units of different-decimals legs
  were not comparable). Local `scaleRawAmount` deleted for the lib's
  validated `scaleByDecimals`. Pinned formatPoolAmount strings unchanged.
- queries.rs tests extracted to `queries_tests.rs` +
  `queries_decode_smoke.rs` (file 3,804 → 3,140 lines; per-file test
  extraction limited to PR-touched god files, per Karol).

Cross-validation (scratch harness, zero shared code with the API): activity
20/20 rows field-identical to an independent Python decode of raw
`soroban_events` (leg mapping confirmed against PROD `asset_sac` — the SHX
SAC has no local contracts row, which is exactly why the API matches legs
by surrogate); chart 1h recomputed from raw tables matches every point to
the cent; 1w bucket lands on Monday; three 400-anti-tests pass; Playwright
3/3. Rig note: Playwright needs vite started with
`VITE_API_BASE_URL=http://localhost:4280` (the `.env.development.local`
default of 4200 silently starves the app) and the stub prices DB needs
`prices.price_usd_series` (prod's daily name), not `_1d`.

## Decisions 2026-08-31 (owner) — plane_id stays, chunking into the row contract, duplicate codes stay

- **27A: `plane_id` STAYS on `pool_state_changes`** — cheap provenance
  insurance: the plane contract's identity is stored nowhere else, and
  recovering "which deployment's scoreboard wrote this state" after the
  fact would take a full-history re-parse. 8 bytes/row buys that never
  being necessary. (A full removal was built and reverted the same day on
  the owner's correction — recorded so the exercise isn't repeated: the
  fields are write-only today, and the removal is mechanical if ever
  re-decided.)
- **29: `reserveRows` chunking deleted** — a new `SummaryRows` sibling in
  `SummaryRow.tsx` lays a flat cell list out two-per-row, so the pairing
  lives beside the 2-cell row contract that causes it and any future
  variable-cell caller reuses it. Rendering identical (verified on the
  3-leg demo pool: 2+1 rows).
- **32C: duplicate leg codes stay as-is** (EURC × two issuers renders two
  `EURC` labels; the asset link disambiguates). Options A (collision-only
  issuer suffix) and B (issuer domain) recorded here if it ever bites.

## Review of PR #438 closed (2026-09-01) — every finding fixed in-branch

Seven-agent review (correctness, simplify, devil, prod-readiness, security, UX,
architect) over the whole PR and its foundations, then a judge pass and an
adversarial re-verification of each P1 in code. Verdict was REQUEST CHANGES:
the architecture is better and the branch is a net reduction, but it carried a
class of merge-blockers. Owner's call was to fix everything here rather than
spawn per-finding tasks. Done.

### The root class, and how far it reached

One root: **an entity's identity taken from a payload the emitting contract
chooses freely, instead of from the authenticated owner/emitter, with no
downstream check.** A three-agent sweep graded every entity producer in
`xdr-parser` and `db-clickhouse/persist` against it.

- **Confined to the new 0374 code.** Every pre-existing producer anchors
  identity to an authenticated source — ledger-entry owner, event
  `contract_id`, tx source, or a crypto derivation corroborated against the
  emitter (`nft.rs`'s `derived_sac == emitter` is the reference).
- **One pre-existing relative, out of scope here**: `operation_asset_appearances`
  takes an asset identity from an event topic with no emitter check. Presence
  only — the value path has read authenticated ledger deltas since 0393 — and
  it already has a task. Re-verified and recorded in **0410**, which is the
  only other member of the class.
- The schema comment that licensed the RMT choice claimed orphan registrations
  "go to a monitored counter, never into rows". No such counter existed; the
  comment now describes the guard that does.

### What changed structurally

`pool_share_tokens` became **`pool_instance_state(pool_id, plane_id,
share_token_id, derived_at_ledger)`** — one table, because both facts come from
one authenticated source (the pool's own instance storage, where the pool
contract is the ledger-authenticated owner), read in one pass, on one version
clock. A second side table was considered and rejected. Free to do: nothing had
deployed. **ADR 0058 decision 4 amended.**

`plane_id` is the authority the read path checks: all three reserve reads (KPI,
chart, list activity) keep only `pool_state_changes` rows whose plane matches
what the pool declares. Symmetrically at write time, a registration becomes a
row only when the named pool declares that emitter as its `Router`.

### Decisions worth carrying

- **No version column on `pool_state_changes`** — the fold is the fix. A
  `write_order` column was built and REMOVED: it was keyed on the fold position
  within a batch, so a narrow re-parse could stamp a lower version than the
  original wide parse and lose to the stale row, inverting the guarantee
  `backfills.md` rule 4 rests on. The classic twin `liquidity_pool_snapshots`
  is version-less for exactly this reason.
- **`pool_kind = 0` stays in `asset_codes_predicate`** — it reads only the
  legacy pair columns, which only classic rows fill, so the guard states the
  function's domain rather than patching a symptom. The more fundamental fix
  (soroban rows carrying NULL instead of a placeholder `0`) would touch the
  ~612 call sites bound to that legacy shape; the guard disappears with them.
- **Search no longer invents a label.** A soroban row's label is `NULL` in SQL
  and built in Rust from the pool's resolved legs — the same identities the
  list and detail render — rather than a hardcoded string.

### Verified against sources outside our own code

- **Failed transactions cannot deliver a contract event.** Stellar's docs:
  consensus contract events are "only populated if the transaction succeeds",
  while diagnostic events "include events from failed contract calls".
  Measured on production over 500 ledgers: 2,199 failed soroban transactions
  carry **only `fee` events** (4,398 = 2 × 2,199), zero contract events. So a
  failed `add_pool` can only reach us as a diagnostic event, and filtering that
  container is what excludes it. Note the converse is NOT true — failed
  transactions do emit consensus `fee` events, correctly, since the fee applied.
- **Cardinality, measured 2026-09-01** from `add_pool` in `soroban_events`:
  10 routers, 500 pools, 500 registrations, **no pool registered twice** — so a
  pool has exactly one router, and `deployment_id` is properly a column rather
  than a join table. Heavily skewed: one router holds 343 of 500. By type:
  constant 375, stable 83, concentrated 39, elastic 3 — so **461 pools mint a
  share token and 39 structurally never do**, which is why `share_token_id = 0`
  is an answer, not a gap. 438 of 500 have ever emitted a flow event.
  (Plane count stays at 2 from earlier work — planes emit no events, so it
  cannot be measured this way until `pool_state_changes` exists on prod.)

### Found while verifying, fixed on develop

`nx typecheck` replayed a cached success while the tree had real type errors.
Not the known stale-`.tsbuildinfo` trap: the target is inferred with
`production` inputs, which EXCLUDE `*.test.ts`, while `tsc --build` typechecks
them — so it checked files its cache key did not track. Reproduced both ways
and fixed in `nx.json` (commit on develop, since it affects every project and
the pre-commit gate, not this branch).

### Still open

Production verification, in the order now written into the runbooks: DDL →
indexer → three catch-up backfills → window-closure check → only then the read
surfaces. Nothing here was verified on production when this was written —
the write path since is: see "Production verification of the write path
(2026-09-13)" at the end of this file.

## Soroban ↔ classic read-surface inconsistencies — ranked (karolkow, 2026-09-01)

Ranked by "data exists and is merely unwired" (high) vs "structural or costly"
(low). Recorded verbatim from the owner's audit; each is a fact about the
CURRENT branch, not a new commitment.

1. 🔴 **`total_shares` empty for every soroban pool.** The Summary card reads
   from `liquidity_pool_snapshots`, which soroban never writes. The value
   exists TWICE already: the chain keeps `TotalShares` in the fungible pool's
   instance storage (observed on mainnet; `PoolInstanceState` does not parse
   the key), and the participants endpoint already computes
   `sum(amount)` over the share token's holders. Same class as TVL/volume.
   Highest: visible on every soroban pool page, cheap to close — and the
   WRITE half (parse `TokenShare`-sibling `TotalShares` into the instance
   arm) belongs with the indexer, before any backfill runs.
2. 🟠 **`participant_count` KPI says "—" while the section below lists the
   participants.** Two surfaces of one page contradict each other. Cost is
   real — EXPLAIN ESTIMATE: the `asset_id` filter on `balances` reads
   120,554,700 rows (table is sorted by `holder_id`) — but the participants
   endpoint pays that scan anyway for the percentage denominator. Cheap exit:
   the participants response carries the count, the KPI reads it. Expensive:
   a second scan on detail.
3. 🟠 **Asset-code filter blind to soroban.** `asset_codes_predicate` is
   gated `pool_kind = 0` (the #438 review fix), so filtering `USDC` finds no
   soroban pool despite many carrying a USDC leg. Closure IS step 3 of 0530
   (legs migration): match through `legs` + asset dimensions, uniform for
   both kinds; the guard dies there.
4. 🟡 **`last_updated_ledger` means a different thing per kind.** Classic:
   last trade. Soroban: registration. List ORDER is fixed (activity_ledger),
   but the wire FIELD still carries two semantics under one name. Target:
   a separate `last_activity_ledger` on the wire, or one semantic.
5. 🟡 **No `#op-N` anchor on the soroban activity feed.** `op_index` exists
   in the parser but `soroban_events` does not store it, and the schema
   records why (key inexpressible for tx-level and pre-protocol-23 events).
   Closure = column + backfill. Real limitation, not an oversight.
6. 🟡 **Concentrated positions (39 pools) not rendered.** Decided ("three
   tables, not four": live aggregation over 13,801 events is cheap) but the
   render is step 23, deliberately LAST. Today those pools show the
   deliberate participants 400 and nothing else.
7. 🟢 **`share_percentage` has a different denominator per kind.** Classic
   divides by snapshot `total_shares`; soroban by the sum of positive
   balances. Both correct, but two definitions under one field name. Unify
   only after item 1.
8. 🟢 **`pool_type` / `protocol` null on classic.** Correct (a protocol pool
   has neither an AMM type nor a vendor) — the Summary card just renders two
   fewer rows. Visual asymmetry, not a data defect.

## DECIDED (karolkow, 2026-09-01) — split the PR: write path ships, read path waits

Executed the same day: write half extracted to branch
`feat/0374-aquarius-write-path` (95 files off develop, byte-identical to the
#438 states on those paths; `cargo check --workspace` + all four crates' tests
green; `extract_openapi` output JSON-identical to the committed spec, so the
api-types gate passes untouched). PR #438 and its branch stay exactly as they
are — the committed archive of the read half. Excluded deliberately: this task
file (its history lives on the #438 branch), the `POOLS_REAL` Playwright spec,
`crates/api`, `web`, `libs/api-types`. Motivation: the API + frontend in PR
#438 read a schema this task's
own follow-through (0530 legs migration, registry-normalization re-open,
snapshot unification triggers, items 1/2/4/7 above) intends to change — so the
read surface would be verified twice.

Sketch of the split (see the wayfinder map for the current state):

- **Write half → new PR**: `crates/xdr-parser`, `crates/db-clickhouse`,
  `crates/indexer`, `crates/backfill-runner`, the `audit-harness` deletion,
  ADR 0058, write-side docs. Parser-level verification (90-ledger raw corpus,
  bidirectional anti-tests) rides with it. Compiles alone — the api crate at
  develop does not reference the new tables.
- **Read half stays on this branch as the archive**: `crates/api`, `web`,
  `libs/api-types`. NOT `git stash` — the work is committed; the branch is
  the preservation. Playwright + API-battery live proof stays with it.
- **Consolidation gate must be narrow**: the read revival waits for
  wire-shape changes (items 1, 2, 4, 7 + the registry-normalization
  re-open), NOT for 0530's ~612 classic call-site migration — soroban's read
  path already sits on the target model (`legs`, `pool_kind`), so gating on
  0530 in full would hold issue #405 hostage to classic-side refactoring.
- **Concrete win**: schema changes are free while no reader exists — every
  "free change, production DDL did not exist yet" note in this file is that
  principle. The writer deploy also starts the catch-up clock earlier.

## Three-lens review of the write half + greenfield schema audit (2026-09-01)

Owner smelled the schema ("two plane_id columns") and asked for
devils-advocate + ponytail-review + /simplify over init.sql / stage.rs /
pool_router.rs / pool_state.rs, then a first-principles redesign. Results,
each verified by producer→consumer trace:

**The dual plane_id is NOT the defect** — it is claim (who wrote the reserve
row) vs authority (what the pool's own instance declares), and reads compare
one against the other; one column cannot be compared with itself. The defect
found instead:

1. **CONFIRMED, must land before prod DDL: forged plane writes can EVICT
   genuine reserve rows.** The stage fold and the table sort key are
   `(pool_id, ledger_sequence)` — blind to plane — while the plane arm
   accepts any `[PoolData, Address(pool)]` shape. A forged entry applying
   later in the same ledger evicts the genuine row at parse time; the
   read-side plane filter then hides the forgery but serves the PREVIOUS
   ledger's reserves as current. Fix is free pre-DDL: `plane_id` into the
   fold key and `ORDER BY (pool_id, plane_id, ledger_sequence)` — forged
   rows land in their own key space and die at the read filter.
2. Reads apply the pool's CURRENT plane to ALL history (argMax) and nothing
   warns when an instance write re-points a pool's plane — the share-token
   column of the same table measured 13 such re-pointings, so the "planes
   never migrate" assumption must fail loudly: add a warn.
3. The resurrected registry-backfill generator cannot corroborate routers
   (instance storage is not in soroban_events): it must refuse duplicate
   registrations and the deploy window re-runs the "no pool registered
   twice" measurement. → final-phase checklist.
4. `commit()`'s exhaustive destructure guards half the silent-drop
   invariant; `write_ledger` needs the same over `StagedLedger`.
5. init.sql's "a third party cannot register — or overwrite" overclaims
   (the UNVERIFIED router-less arm is the documented exception); one clause.

**Simplify/ponytail (converged independently, net ~−95 lines):** four
hand-copied last-wins folds → one generic `keep_last_by_key` (also gives
finding 1's key change a single home); `dedup_final_plane_writes` is
redundant with the stage fold (two independent traces); `pool_state.rs`
hand-rolls scval readers incl. a local `map_get` (the exact 0393 recurrence
— `pool_router.rs` imports them correctly); `subpool_salt` is decoded then
dropped against the schema's own "extract on demand, never copy" rule; plus
a minor batch (per-event String clone on the hot path, two reserve-parse
idioms in one function, duplicate bytes32 reader, comment triplication,
soroban-arm helper extraction).

**The fundamental alternative to the dual plane_id — measured and refuted
(2026-09-01).** Owner challenged the claim-vs-authority check as a patch and
asked whether the pool's OWN instance storage (self-authenticated: entry
owner == pool identity) could be the reserve source, dissolving the forgery
surface entirely. Measured on the 90-ledger raw corpus (all eras, all
types): every one of 239 plane writes has a same-ledger instance write
(coverage 100%); the mirror exists under three spellings (constant
`ReserveA/B`, stable `Reserves` vec, concentrated `Reserve0/1`) and NEVER
contradicts the plane (233 equal, 0 differ, 6 apparent shorts of which 5
are the concentrated prefix-vector false alarm) — **but the ELASTIC type
mirrors nothing**: its instance carries `TokenA/B`, `Oracle`,
`FeeFraction`… and no reserve key in any spelling; elastic reserves live
ONLY in the plane. So the self-authenticated source cannot be universal,
the plane machinery must stay for at least one type, and switching the
others would trade one mechanism + one check for three source arms + the
same check. Also re-measured pre-dedup collisions: 29 of 259 real
(pool, ledger) instance keys carry >1 image (max 5 in one ledger) —
`dedup_final_pool_instances` is load-bearing, not theoretical; and 0 pools
in the corpus are written by more than one distinct plane — the forged-write
class remains unobserved in the wild, and the plane-in-key fix is purely
preventive. Write-time filtering (prefetched pool→plane map) was rejected
for a fundamental reason, not a stylistic one: under a PARALLEL backfill a
pool's activity partition can stage before its registration partition
commits, so the map is incomplete and genuine rows would be refused
permanently and silently; the stored-provenance + read-filter form is
order-independent and reversible.

**Greenfield audit (from requirements R1-R8, not from the code): the
3-table shape is the fixed point.** One RMT row = one writer = one clock
forces exactly one table per (writer, clock, grain), and there are exactly
three: registration events / reserve stream / pool self-declarations.
M1 (fold instance state into the registry) fails on migration clobber and
on AggregatingMergeTree's max() keeping the WRONG token when a migration
re-points to a lower hash surrogate; M2 (fold into the time series) lets a
forger's row collide with the authority row that exists to reject him;
M4 (read-time derivation) is impossible — instance writes are ledger-entry
changes, absent from soroban_events. Zero tables to save. The audit adds
one thing: `total_shares Int128` on `pool_instance_state` (same entry, same
pass, same clock) — the write half of ranked-inconsistency item 1, belongs
in the same pre-backfill DDL.

## Correction (2026-09-02) — concentrated positions are NOT NFTs

ADR 0058 (and a code comment on the feature branch) said concentrated
positions "are NFTs". Vendor docs and this task's own step-17 decision
(2026-08-26) refute it: a position is the storage entry keyed
`(owner, tick_lower, tick_upper)` — no share token, no NFT, no numeric
position id; `position_update` is the indexing source. The phrase entered on
2026-08-29 as an unsourced aside in the read-path commit (a Uniswap-v3
pattern carried over) and was copied into the ADR the same day. ADR fixed;
the branch comment corrected in place. Indexing stays deferred to 0516.

## Production verification of the write path (2026-09-13)

State after the backfill (one targeted re-parse over the full ingest range,
`--only` carrying `liquidity_pools`, `pool_state_changes`,
`pool_instance_state` — commit `e1d97e0b`) plus the live writer: **769
soroban pools** — router family 514, pair-factory family 235, config-factory
family 20. Every oracle below is independent of our decoder unless stated.
Scripts and raw outputs were scratch artefacts; the query shapes are given
inline so each figure can be reproduced.

### Registry closure — every layer passes

| Layer                              | Method                                                                                                                                                          | Result                                                                                                                                                                                                                                                                                                      |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Router, chain events               | `signature = 'add_pool'`, per partition 100-128 (`intDiv(ledger_sequence, 500000)`), pool address from `data_xdr` value 1, both directions against the registry | 514 announced ↔ 514 registered; 0 missing, 0 extra, 0 announced twice, 0 emitter ≠ `deployment_id`, 0 event `pool_type` ≠ `pool_type_raw`; 10 emitting routers                                                                                                                                              |
| Router, vendor catalogue           | the vendor's external pools API, paginated, against the pools of the documented router (`CBQDHNBF…`)                                                            | stated count 353 = ours 353. Its paging exposed 343 distinct addresses over four passes; the other 10 are in the `add_pool` set and answer RPC. On the 343: 0 not ours, 0 `pool_type` mismatches, 0 fee mismatches, 0 leg mismatches (order included; legs mapped id → address through `soroban_contracts`) |
| Pair-factory, contract enumeration | RPC `all_pairs_length` + `all_pairs(n)` on each of the 4 deploying factories                                                                                    | set-equal per factory: 214 / 11 / 6 / 4                                                                                                                                                                                                                                                                     |
| Config-factory, contract listing   | RPC `query_pools` on each of the 6 factory deployments                                                                                                          | listed ⊆ ours on all 6; the only unlisted pool of ours is `CAZ6W4WH…` (delisted — see below)                                                                                                                                                                                                                |
| Sibling registry                   | `prices.pool_registry` set-compare                                                                                                                              | only-theirs = 0 for all three venues (488 / 221 / 19)                                                                                                                                                                                                                                                       |
| Coverage                           | registry pool ∈ `pool_instance_state` and ∈ `pool_state_changes`                                                                                                | 514/514, 235/235, 20/20                                                                                                                                                                                                                                                                                     |

`pool_type_raw` is correct per family, verified four ways: the writer
(`stage.rs` — pair family writes it empty because the pair contract has no
type; config family writes the `PairType` discriminant verbatim); the
contract's own on-chain spec (`PairType { Xyk = 0 }` on the config pool; no
type in the pair spec); a set-level mapping of the sibling registry's venues
onto our buckets with no cross-over (router types 488, `"0"` 19, empty 221);
and the event/API agreement in the table above.

Residual, stated: unknown pair- or config-factory deployments cannot be
swept by event name yet — the label-convention registration events still
carry `signature = NULL` (the 0517 in-DB backfill has not run; a probe for
`new_pair` returned 0 rows in all 29 partitions). The registry itself is not
factory-scoped (the re-parse detects by shape), so this is a gap in the
check, not a known gap in the data.

### Current reserves — RPC simulation of every pool (the contract answers from its own storage)

Router `get_reserves`, pair `get_reserves`, config `query_pool_info`,
compared exactly against the newest `pool_state_changes` row on the pool's
declared plane. Sweep ran over ledgers 64,410,119 → 64,410,190; the 18 pools
that changed state inside that window and 1 transient RPC failure were
re-simulated ledger-aware and all matched.

| Outcome                                                      | Pools   |
| ------------------------------------------------------------ | ------- |
| exact                                                        | **758** |
| mixed-decimal stable pool, one leg scaled by 10^k (defect 1) | 6       |
| unsynced balance — our row equals the pool's STORED reserves | 4       |
| code replaced by non-pool code (defect 2)                    | 1       |

`get_reserves()` is a computed view, not a storage read, on the upgraded
router WASMs: it returns token balance minus unclaimed protocol fee (exact on
8 of 8 legs checked). The "exact" count therefore compares against the
contract's live view; the storage comparison below is the stricter one.

**Defect 1 — mixed-decimal stable pools store NORMALISED reserves.** Every
non-empty router-family stable pool whose tokens differ in decimals (6 of 6)
has one leg in our table equal to the chain value × 10^(max_decimals −
token_decimals): ×10 for 6/7-decimal pairs, ×10^11 for 18/7. The other two
mixed-decimal stable pools match only because both are empty. All share one
WASM (`f1077e0b…`). The plane carries the stable math's normalised figures;
`get_reserves()` returns raw token units. This refines the T4 finding
("plane state == reserves"), which holds for equal-decimal pools only. A read
applying each token's own decimals to these rows overstates one leg by 10× or
10^11×. Pools: `CA262ONR…`, `CA27UTMX…`, `CCI5UGNC…`, `CCYMZTOJ…`,
`CDCSXULB…`, `CD5WJYPF…`.

**Not a defect — four pools whose live balance runs ahead of their stored
reserves.** `CBI5I254…`, `CB6GYGGZ…`, `CCRULRY3…` hold a time-rebasing token
(`yUSDT` "Tether USD with yield from AAVE", `yTIME` "Time Rebased Token") whose
balance grows with no transaction at all — one leg was re-read a day apart
and had grown with zero events in between; three of the four pools are in
emergency mode and do not trade, so nothing re-syncs them. `CDDLTOOD…`
received 19 direct XLM transfers after its last trade summing to exactly the
gap (2,005,041). In both cases the pool's OWN instance storage
(`ReserveA`/`ReserveB`, read via `getLedgerEntries`) equals our row to the
unit, so the table matches what the ledger stores; the difference is balance
the pool has not yet absorbed (`ReservesSyncLedger`). No indexer can record a
time-rebasing balance continuously — it is computed at read time, never
written. The read may say "as of ledger X".

**Defect 2 — a registered pool whose code was replaced by non-pool code.**
`CAZ6W4WH…` (config family, documented factory). Timeline from
`executable_update` topics, which carry the old and new code hash:

| Ledger (UTC)                  | Event                                                                                                                                                                                                                                              |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 51,572,101 (2024-05-07)       | registered                                                                                                                                                                                                                                         |
| 54,514,504 (2024-11-22 13:29) | last pool activity; our newest reserve row                                                                                                                                                                                                         |
| 54,515,539 (2024-11-22 15:14) | pool code replaced by code exposing `bond`, `unbond`, `distribute_rewards`, `query_staked` (a staking interface), submitted by the account behind 61 upgrades of this factory's pools                                                              |
| 54,515,539 → 63,767,534       | no activity; the address holds exactly our last reserves                                                                                                                                                                                           |
| 63,767,534 (2026-08-02 17:10) | code replaced again (`install`, `mint_redeem_sweep`, `redeem_held_sweep`, `sweep`) by a single-use account; the same transaction moves both balances out, plus the liquidity of a second registered pool on the same pair (`CD5XNKK3…`, now 0 / 0) |

The balances moved out equal our last row to the unit (263,512,715,771 /
131,948,815,702). So the NUMBERS were true until 2026-08-02; the IDENTITY
("this is a pool") has been false since 2024-11-22. No row was written after
the first replacement — the writer decoded nothing wrong; the registry simply
outlived the code that justified it. Authorisation of either replacement is
not determinable from our data (a transaction's source need not be its
authoriser). Our rows for `CD5XNKK3…` are correct (0 / 0 matches the chain).

Upgrades are common, so this class is live: 403 of 514 router pools (1,537
`executable_update` events) and 14 of 20 config pools (63) have been
upgraded; pair-family pools none. The other 19 config pools still answer the
pool interface and match exactly.

### Total shares

| Family                                    | Result                                                                                                                                                                                        |
| ----------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pair (`total_supply` on the pair)         | 235 / 235 exact                                                                                                                                                                               |
| router, current WASM (`get_total_shares`) | 451 / 451 exact (constant, stable, elastic)                                                                                                                                                   |
| router, oldest WASM                       | 17 pools lack `get_total_shares`; their share tokens lack `total_supply` — **no RPC oracle**; only a checkpoint snapshot can check them. 11 of the 17 hold exactly `10000000000` in our table |
| router, concentrated                      | ours `0` (modelled as structural) vs chain `get_total_shares` non-zero on 45 of 46; `share_id()` returns the pool itself                                                                      |
| config                                    | ours `0` (structural — supply lives on the separate share token) vs chain `asset_lp_share.amount` non-zero on 17; 2 zero on both; 1 not callable (`CAZ6W4WH…`)                                |

### What this settles and what it leaves

Settled: the registry is complete and correctly attributed for all three
families against independent sources, and current reserves are exact for 758
of 769 pools.

Open, in order of user impact:

1. Defect 1 — **DECIDED (karolkow, 2026-09-15): C′, read router-family
   reserves from the pool's own storage** (see "Root cause of defect 1"
   below). Before changing the parser, measure the reserve-key layouts of
   every historical router-pool code version, not just the current ones.
2. Defect 2 — a registered pool whose current code lacks its family's pool
   interface must not render current reserves; history stays. **Carried by
   task 0325** (widened on 2026-09-15 to "every code-derived row must match
   the contract's current code"), where the measurement over all upgrades
   lives.
3. The read half must not render the structural `0` total shares as a value
   for concentrated and config-family pools; the chain has a figure for both.
4. Historical reserves remain checked only by the decoder-parity test of the
   backfill session (same code on both sides); checkpoint snapshots are the
   only independent history oracle and have not been run for these tables.
5. Read-half labels: `protocol_labels.rs` on the read-half branch predates
   the pair and config families. Brand only deployments the vendor documents;
   a shape-matching contract from any other deployment stays unlabelled.

### Root cause of defect 1 — the plane is a quote-input sheet, not a balance sheet (2026-09-14)

Three sources read for all **514** router-family pools at the same moment:
the pool's own instance storage (`getLedgerEntries`), its plane row
(`plane.get([pool])`, simulated), and our newest row.

| Pool type                                 | Pools | Pool storage vs plane row            | Our table                           |
| ----------------------------------------- | ----- | ------------------------------------ | ----------------------------------- |
| constant                                  | 380   | identical                            | matches                             |
| elastic                                   | 3     | identical                            | matches                             |
| stable, no `PrecisionMul` (older code)    | 38    | identical                            | matches                             |
| stable, `PrecisionMul` all 1              | 39    | identical                            | matches                             |
| **stable, `PrecisionMul` ≠ 1, non-empty** | **6** | **plane = storage × `PrecisionMul`** | **scaled**                          |
| stable, `PrecisionMul` ≠ 1, empty         | 2     | identical (zero)                     | matches                             |
| concentrated                              | 46    | pool does not write the plane        | matches (already read from storage) |

Four pools first looked off; re-read ledger-aware, three matched and one had
traded after the storage read. So our table equals the pool's own storage in
**508 of 514**; the six are exactly the non-empty pools whose stable math
carries a multiplier. A stable pool's instance holds `Reserves` (raw units),
`Decimals` and `PrecisionMul`; its plane row holds `pool_type`, the
stableswap parameters (fee, amplification ramp) and `Reserves ×
PrecisionMul` — precisely the inputs a swap quote needs, in the units the
invariant is computed in. No standard governs the plane; the vendor's source
is not public, so "normalised on purpose" is inferred from that structure. The
error was ours: T4's "plane state == reserves" was generalised from a sample
that did not include these six.

Why storage is the fundamental source, not a read-time divide: the pool is
the ledger-authenticated owner of its instance (ADR 0058's authority rule);
it holds raw units, so `reserves` keeps one meaning; it matches our table in
508 of 514 pools, so the switch changes no correct value; and it unifies the
family — concentrated pools already use storage because they never write the
plane. Current code uses three layouts: `ReserveA`/`ReserveB` (383 pools),
`Reserves` (85), `Reserve0`/`Reserve1` (46).

### Decision C′ measured across every code version (2026-09-15)

Before changing the parser: which storage keys hold reserves in EVERY code
version router-family pools have run, not only the current ones. Code
versions come from `executable_update` topics (old and new hash per upgrade)
plus the original code of never-upgraded pools.

- **58** code versions ran on router pools in the ingested range (1,537
  upgrades, 403 pools upgraded).
- **55** versions: one raw archive ledger each in which a pool on that
  version changed reserves, decoded — every one wrote its reserves into its
  own instance storage. **3** versions (`D3A1C7B6`, `419DD5F0`, `45435508`)
  saw only administrative events while pools ran them: no reserve change, so
  nothing to read.
- Exactly three layouts in all history, no fourth: `ReserveA` + `ReserveB`
  (constant, elastic; later versions add `ReservesSyncLedger`), `Reserves`
  (stable, a vector, with or without `Decimals` / `Precision` /
  `PrecisionMul`), `Reserve0` + `Reserve1` (concentrated).
- **Raw units in every version — the decisive check.** The 8 mixed-decimal
  stable pools, every version each wrote reserves on: 23 samples; storage raw
  and plane = storage × `PrecisionMul` in 21, both zero in 2, exceptions 0.
  For equal-decimal pools plane = storage in every sample.
- A suspected gap (trades on version `3ECB29BB` with no reserve rows) was a
  sampling artefact: all six rows exist with exact values.

Parser rules this fixes:

1. Read `ReserveA`+`ReserveB` or `Reserves` from the pool's instance write. A
   missing key emits no row — never a zero: administrative operations rewrite
   the instance too.
2. Emit only when the reserves changed between the instance pre-image and the
   post-image (both are in ledger meta) or the instance was created. Otherwise
   every reward or config operation adds a row with unchanged numbers.
3. `plane_id` = the plane the pool declares in its own instance (the
   concentrated arm already does this), so a corrected row REPLACES the old
   one under the same key instead of standing beside it.
4. The plane stays as a cross-check, not a source: plane = storage ×
   `PrecisionMul` (1 where absent) held in every sample, so a mismatch is a
   signal of changed contract semantics.

Rollout: parser first (live), then the history of the 8 affected pools — 821
rows in 726 ledgers across 60 archive partitions. A range re-parse would fetch
~60 partitions (~800 GB) for ~1 GB of ledgers, so the backfill is a list pass
over those 726 ledgers through the same parse+stage path (option A, decided
2026-09-15), never an in-DB division. No DDL, no indexer pause. Verify by
re-running the three-source comparison: expected 514 / 514.

### Decision C′ implemented — PR #459 (2026-09-15)

Branch `fix/0374_router-reserves-from-pool-storage`, commit `bbfe3ca5`, PR
https://github.com/rumblefishdev/soroban-block-explorer/pull/459. Built
test-first; every new test failed on the old code for the intended reason.

**What changed**

- `pool_state.rs`: `parse_pool_instance` reads reserves from all three
  layouts in raw units, plus `PrecisionMul`; absent keys give no reserves,
  never zeros. `extract_pool_instances` pairs each post-image with its `state`
  pre-image and flags `reserves_changed`.
- `stage.rs`: the plane arm no longer stages rows; the instance arm stages a
  row when `reserves_changed`, with `plane_id` = the declared plane. The plane
  row is compared with storage × `PrecisionMul` on the legs only (a plane
  vector may carry a per-tick tail) and a mismatch logs a warning. **That
  cross-check was removed later the same day** — it was wrong 17 times out of
  17 on a wider corpus; see the two decision sections below.
- Harness `redecode_pool_state_changes_from_list` (ledger list from a file) —
  the differential tool and the backfill generator.
- ADR 0058 amended; indexing-pipeline, xdr-parsing and database-schema
  overviews, `init.sql` comments and `docs/backfills.md` (list-pass procedure)
  updated. No schema change.

**Verification**

| Check                                                              | Result                                                                                                      |
| ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| old code over 874 real ledgers vs production rows on those ledgers | 2,627 = 2,627 (the baseline reproduces production)                                                          |
| new code, pools other than the 8 affected                          | 1,862 / 1,862 identical to production                                                                       |
| new code, the 8 mixed-decimal pools                                | 754 rows = old ÷ `PrecisionMul`, all; 18 zero rows unchanged                                                |
| new rows vs pool storage decoded from raw ledgers                  | 23 / 23 exact                                                                                               |
| rows no longer produced                                            | 11, each an exact repeat of the pool's previous row (plane re-publication, 10 of them in ledger 62,234,220) |
| rows produced that did not exist before                            | 0                                                                                                           |
| reserve-moving activity of the 8 pools outside the backfill list   | 10 ledgers (`claim_protocol_fee`, gauge reward claims): 0 rows from the new code                            |
| stored rows off the pool's declared plane (production)             | 0 of 773 pool/plane keys — re-derived rows replace old ones                                                 |
| final code vs the verified run                                     | byte-identical output                                                                                       |
| tests                                                              | xdr-parser 439/439, db-clickhouse 144/144, integration suites green; clippy `-D warnings` and fmt clean     |

Two CH-gated tests fail identically on clean `develop` against the local
container, which lacks the `executable_owner_id` column
(`repair_tier1::columns_tests::soroban_contracts_rebuild_keeps_the_executable_reference`,
`g9_cross_ledger_verdict_routes_nft_events`); `bootstrap::…writes_rows` is
order-dependent in a full local run and passes on its own.

**Rollout (writer first)**

1. Merge PR #459, deploy Compute.
2. Insert the re-derived history of the 8 pools — 772 `(pool, plane, ledger)`
   keys in 726 ledgers, produced by the harness from the merged code
   (payload SHA-256 `e4bff4c864902749e7a5618f759fa397e80c5bb942327fe5391bd75dbd1b1832`);
   production holds 821 raw rows for those pools = the same 772 keys plus
   unmerged duplicates. Then `OPTIMIZE TABLE pool_state_changes FINAL`.
3. Re-run `cargo test -p backfill-runner --test pool_reserves_reconciliation`
   — expected afterwards: every pool equals its own storage except
   `CAZ6W4WH…`, whose code is no longer a pool (task 0325).

### Wide differential of decision C′ + the two sibling families (2026-09-15)

**Differential, 1,565 ledgers.** The previous 874 plus one ledger per router
pool (all 514) and one per (code version × set of events the pool emitted in
one transaction) — 457 combinations across the 50 versions that emitted
anything in range. Function names are not stored, so the event set stands in
for the called function; calls that emit nothing are not targeted. Base
`c2fc620c` and the PR, same test harness, same archive files:

| Outcome       | Rows  | Check against production                                                                            |
| ------------- | ----- | --------------------------------------------------------------------------------------------------- |
| identical     | 3,116 | —                                                                                                   |
| value changed | 754   | all in the 8 `PrecisionMul` ≠ 1 pools, each = base ÷ multiplier                                     |
| dropped       | 160   | all equal the pool's previous production row (true repeats): 156 concentrated, 2 stable, 2 constant |
| added         | 0     | —                                                                                                   |

**Soroswap-shaped (235) and Phoenix-shaped (20) pools** already read raw
reserves from their own storage (task 0518). RPC on 2026-09-15: 235/235 and
19/20 equal our newest row. Repeat rows in production are 0.03% and 0.006%
(router: 0.4%), so the unchanged-reserves rule adds nothing there.

The twentieth, Phoenix pool `CAZ6W4WH…`, is the silent-freeze failure mode in
the wild: pool code until 54,514,504; replaced by a staking contract at
54,515,539 and by a sweep tool at 63,767,534, which wrote `u32(0)` (now an
address) and `u32(1)` = 0 but not `u32(2)`. The parser drops a half reserve
pair without `CONFIG` silently (`pool_config_factory.rs` `_ => continue`), so
our row still shows the pool-era reserves. Phoenix pools: 63 upgrades on 14 of
20 pools, 7 current code versions; Soroswap: one version, no upgrades.

### Decision (karolkow, 2026-09-15) — an unread reserve write is logged, not alarmed

A storage layout we do not read makes a pool's snapshots stop without a
trace. Considered: a CloudWatch alarm, a hard error, and a plain `error!`.
Chosen: the plain `error!`, like every other decoder refusal. No other
decoding gap pages anyone — alarms cover whether the system is alive — and an
alarm keyed on these conditions would still miss the silent shapes (a
concentrated pool has no plane to compare against; a renamed identity key
makes the write look foreign). A hard error would stop ingestion of every
ledger for one pool's layout change. Logged now:

- router: the instance was written with no known reserve key while the plane
  row of the same (pool, ledger) shows non-zero reserves;
- pair-factory: an instance carrying the identity triple but half a reserve
  pair (a snapshot carries every key, so this is a layout change).

The config-factory half pair stays loud only with `CONFIG` in the same
transaction: without the registry the parser cannot tell a pool's lone
`u32(1)` write from any other contract's u32-keyed entry. Pool `CAZ6W4WH…`
(above) is exactly that case, so a data-level check against the chain is the
only thing that sees it.

**Measured on the 1,565-ledger corpus after the change:** rows identical to the
previous PR run; the two new `error!` lines fired **0** times. The existing
plane cross-check (`plane row disagrees …`) fired **17** times, all false:

- 13 on mixed-decimal stable pools at early ledgers (58.2M–63.4M) whose
  instance write carried no `PrecisionMul` key while the plane already held
  the multiplied figure — the check reads a missing key as × 1. Likely an older
  code version derived the multiplier without storing it (not verified).
- 4 on concentrated pools, which DO write plane rows (36 values against the
  instance's 2) — refuting "concentrated pools do not write the plane" above.

### Decisions (karolkow, 2026-09-15) — drop the plane cross-check; reconcile against the chain on demand

- **Plane × `PrecisionMul` cross-check removed** (the 17 false warnings above).
  The plane is kept only for the unread-layout `error!`.
- **`pool_reserves_reconciliation`** (backfill-runner integration test, same
  shape as `account_reconciliation`): reads every registered Soroban pool's
  own reserve entries with `getLedgerEntries`, decodes them from XDR itself,
  bounds ClickHouse at the ledger the RPC answered at, and fails on any pool
  whose newest `pool_state_changes` row differs. Skips without
  `POOL_CH_CERT`/`POOL_CH_KEY`; run on demand — after a pool backfill and per
  release. First run, production, ledger 64,442,966, 1.5 s: **762 of 769
  equal**; the 7 failures are the 6 non-empty mixed-decimal stable pools
  (production still holds the plane figure until the C′ backfill) and
  `CAZ6W4WH…`.

**`CAZ6W4WH…` leaves this task.** The pool whose code was replaced is carried
by task 0325 ("a code-derived row must follow the contract's CURRENT code"),
where the decision is now to write the verdict at the code change instead of
deriving it per read. Until that lands, `pool_reserves_reconciliation` lists
the pool on every run and its stale reserves stay visible.

### Decision C′ shipped to production (2026-09-16)

- **Deploy:** Compute from `develop` at `9b0f05b6` (PR #459 merge); the indexer
  Lambda updated 10:49:22 UTC; 0 WARN/ERROR lines in the following minutes.
  A laptop deploy needs `zig` for `cargo lambda build --arm64` (it had been
  removed from the machine; `brew install zig` restored it).
- **Backfill:** ledger list taken 10 min after the deploy (in-flight runs of
  the old code drained) — unchanged from the morning: 728 ledgers, 774 keys of
  the 8 pools. Re-derived with the merged parser; checked against production
  key by key: 758 = production ÷ `PrecisionMul`, 16 zero in both, 0 other, 0
  keys on either side only. Payload sha256
  `fb14d1b9e4ed51dc258f46ad0eb95f42bd27ea780ed90d9a2a000b13aefb7ba3` (contains
  the 772-row payload recorded above plus 2 later rows of `CCYMZTOJ…`).
  Loaded into a Memory staging table, read back identical, then
  `INSERT … SELECT`, `OPTIMIZE … PARTITION 11 FINAL` and `… PARTITION 12 FINAL`
  (the survivor of an unversioned `ReplacingMergeTree` merge is the last
  inserted row), staging dropped.
- **Verified after the write:** partitions 11 and 12 merged; 774 raw rows =
  774 keys for the 8 pools; production rows equal the payload exactly.
  `pool_reserves_reconciliation` at ledger 64,454,699: **769 of 770 pools equal
  their own storage**; the one failure is `CAZ6W4WH…` (task 0325).

### SAC legs keyed on the asset — shipped and repaired (2026-09-22)

The first PR split out of #455: #474 (`fix/0374-sac-leg-rekey`). A soroban
pool leg keys on the asset it is (`assets.id`), not on the SAC contract
surrogate; the SAC → classic map loads on every ledger that registers a pool,
live and under `--only`; one fn keys legs and contract-held balances; runbook
`docs/runbooks/0374_lp_legs_sac_rekey_repair.md`.

- **Deploy:** Compute from `develop` at `9a89d35c` (the #474 merge), which also
  carried task 0381's `536dbcdb` and `c346117c`. Indexer Lambda updated
  15:26:12 UTC, API 15:26:14; the index at the chain's tip afterwards, DLQ
  empty.
- **Repair A** (runbook step 3, operator): `mutation_1995468`, created
  15:33:01, done, no failure. No registry backfill was running.
- **Verified after the mutation**, reading `legs` directly:

  |                                         | before (dry run) | after          |
  | --------------------------------------- | ---------------- | -------------- |
  | soroban legs keyed on a SAC, latest row | 1,454 of 1,553   | **0**          |
  | same, every physical row (duplicates)   | —                | **0**          |
  | soroban legs with no `assets` row       | 1,455            | **1**          |
  | soroban pools carrying native XLM's id  | 0 of 770         | **260 of 770** |
  | classic legs with no `assets` row       | —                | 0 of 107,056   |

  The one unresolved leg is pool `8FE06922…`, the inert config-family pool
  the runbook names.

- **Still open, task 0571:** the live map keeps only facets with
  `max(sac_deployed) = 1`, and 138 SACs are marked not deployed. The repair's
  map has no such filter, so their legs are fixed now, but a new registration
  naming one of them orphans its leg again until 0571's root is found. The same
  gap keys 286 contract-held balance pairs on the SAC; that predates #474 and
  is not this defect.
- **Not closable yet:** the read side does not show soroban legs until the
  read PRs of the split land; issue #405 stays open.

### Pool as legs — PR 3 of the split (2026-09-23)

PR #479 (`feat/0374-pool-legs`): the list and detail endpoints read `legs`,
`filter[pool_kind]` replaces the four positional leg filters, a Soroban pool is
addressed by its `C…` contract, and the frontend renders every leg with a
kind chip row and badge.

- **Measured before it:** 770 Soroban pools list on production as `XLM / XLM`
  under a wrong `L…` id (positions 21,329 onward in the default order) — their
  pair columns hold placeholders and develop's list has no kind filter.
- **No data step:** `legs` is filled for every row (0 empty of 54,303; 11
  Soroban pools with more than two legs).
- **Found while rebuilding it:** a leg nothing identifies made
  `assetLegLabel` throw outside any section boundary, which blanks the app.
  One live pool has one (`CCH6A2JC…`, the inert pool whose leg has no `assets`
  row). It now reads `Unregistered token`, one label shared with the
  balance-change cell.
- **DECIDED (karolkow, 2026-09-23): deploy PR 3 together with PR 4 and PR 5.**
  Until those land a Soroban pool shows `—` for reserves, TVL and shares, and
  the participant and activity sections show zeros that are not measurements.
  The zeros are on production today as well, under the wrong pair, but the
  kind filter would make them easy to reach.

## 2026-09-17 (karolkow) — can a Soroban pool delete a key we read? No deployed one can

Spawned from task 0210's pool-removal fix: the classic extractor stored a
`state` image as a value and skipped `removed`, which left 1,671 stale
snapshots. The same question for the six Soroban-side extractors that skip
`removed`.

- **Safe by construction:** `extract_pool_instances` (router family) and
  `extract_factory_pairs` (Soroswap) read the contract instance entry. A
  contract cannot name that key (`soroban-env-common` `val.rs:622-626`,
  `convert.rs:596-600`) and deleting a key inside instance storage rewrites the
  entry as `updated` (`host.rs:2167-2173`). `extract_executable_ref_targets`
  reads executable-tag entries, whose delete the host refuses
  (`host/data_helper.rs:668-684`).
- **Safe because they write no row:** `extract_plane_pool_data` feeds a
  mismatch log (`stage.rs:1425`), `extract_address_list_writes` a same-ledger
  registration check.
- **Phoenix-family `extract_config_pools` reads deletable persistent keys**
  (`CONFIG`, `u32` 0/1/2). Measured 2026-09-17:
  - 4,731 ledgers decoded (every admin, creation, upgrade and no-event ledger
    of the 20 pools and 6 factories, plus sampled swap/provide traffic):
    **0 removals and 0 evictions** of those keys, 0 instance updates dropping a
    storage key.
  - Current chain state over RPC at 64,474,775: all four keys present for all
    20 pools (44 archived, 57 live — archived is not deleted).
  - **Bytecode, not the vendor's `main`:** all 31 wasm versions these pools ever
    ran, fetched by hash over RPC. None matches a published Phoenix checksum, so
    each was read directly: the delete host function is imported by 9 of them
    and called only on the instance key `p_admin` (an `updated`) and, for the
    blended pool, on `u32(5)`, which we do not read.
  - **A removal cannot mean zero:** the deployed contracts read these keys with
    `unwrap()`, so a pool missing one is broken, not empty.
- **Landed:** `extract_config_pools` now recognises a family-shaped
  `state` + `removed` pair and logs `error!` instead of silently skipping it;
  it writes no row, because no honest value exists. Unit test on a constructed
  pair.
- **Residual risk:** an upgrade could add a delete. `soroban_contracts` carries
  the wasm hash per contract, so that is where a watch belongs.
- **Two findings for elsewhere:** pool `CAZ6W4…` was upgraded at 63,767,534 to a
  four-function sweep contract and is still in the pool registry;
  `soroban_contracts` shows `f74d87d7…` for `CBENABXP…`, which runs `6fe099b6…`
  (task 0320's stale-hash symptom). Both under audit.

## 2026-09-22 — joining or leaving a classic pool is not a pool operation

Open, found in an audit of values we could derive instead of storing, looking
up or omitting.

- **`change_trust` on a pool share carries no pool id.** For
  `ChangeTrustAsset::PoolShare` the parser writes only the variant name —
  `{"type": "liquidityPool", "params": "LiquidityPoolConstantProduct"}`
  (`crates/xdr-parser/src/operation.rs`, `format_change_trust_asset`). The
  pool id is derivable from the parameters the operation carries: CAP-38
  defines it as `SHA256(LiquidityPoolParameters)` (asset pair + fee), the same
  identity `extract_liquidity_pools` already relies on. `operation_pools`
  takes a pool id only from `liquidityPoolId` or `poolIds`
  (`crates/db-clickhouse/src/persist/stage.rs`, `OpTyped::from_details`), so
  opening or closing a pool-share trustline is missing from the pool's
  activity, and the transaction detail shows a generic label instead of the
  pool. Fix: derive the id in the parser, emit it as `liquidityPoolId`; history
  needs a re-parse. Scale not measured — operation details are not in
  ClickHouse.
- **No oracle pins the derivation.** Nothing recomputes
  `SHA256(LiquidityPoolParameters)` and compares it with the
  `liquidity_pool_id` of a real `LiquidityPoolEntry`. A corpus test with no
  network would pin the function the fix above adds.

### One-legged activity rows are round trips, not lost data (2026-09-23)

Found while reviewing PR 3 (#479): `PoolActivityItem.event` was documented as
null only in an "unreachable" malformed case. Measured in the 100k ledgers to
64,576,995: **350 of 6.09M** operations carry one leg only in
`lp_operation_amounts`.

- **Every one is a multi-pool path payment** (types 2 and 13, routes through
  2–5 pools), and the stored leg is always positive — the asset ENTERED the
  pool.
- **Cause, verified on one transaction** (`0d22447e…`, strict receive through
  5 pools): the route crosses pool `1746987b…` (EURC/yXLM) twice in a row, there
  and back — hop 3 takes 415,847 yXLM and pays 37,745 EURC, hop 4 takes the same
  37,745 EURC and pays 413,354 yXLM. EURC nets to exactly zero, and
  `pool_fill_amounts` drops a leg that nets to zero (`stage.rs:185`); what is
  left is +2,493 yXLM, the pool's take on the round trip.
- **Not an indexer defect:** the row is the op's true net effect on the pool.
- **Open for the activity PR (split PR 7):** such a row has `event = null` and
  renders `—`. It is a trade in substance; whether to classify a round trip as
  one (and how to show a leg that moved and came back) is a display decision.
  PR 3 corrects the DTO comment to say the case is real and keeps the handling.

### Pool as legs merged (2026-09-24)

PR #479 merged (`3cebd913`), ten commits: the test moves and the base change, plus
the SAC field dropped from legs, activity amounts and reserves keyed by leg,
dead guards removed on both sides, a plain rewrite of the pool code filter
(SQL byte-identical) and the unnamed-leg avatar fixed. **Not deployed — held
for PR 4 and PR 5 (decision 2026-09-23).** Follow-ups recorded elsewhere: one
display name from API to frontend (0546), rank pool results so `XLM` lists
native pools first (0485), filter pools by asset identity (0470 stage 3,
undecided), round-trip activity rows (this task, for PR 7).

### PR 4 split into 4a–4d; 4a and 4b (2026-09-24)

PR 4 lands as four PRs: 4a split the pool queries by topic, 4b order the list
by activity, 4c Soroban reserves / shares / TVL, 4d Soroban chart.

- **4a, #494 (`refactor/0374-split-pool-queries`):** `queries.rs` (1,928
  lines) into one file per handler (`list_pools`, `get_pool`,
  `get_pool_chart`, `list_participants`, `list_pool_activity`) plus the shared
  `usd_analytics`. Pure move, SQL byte-identical. The naming rule it
  prompted is now in `CLAUDE.md` (`6226e9a7`).
- **4b (`fix/0374-pool-list-activity-order`, stacked on 4a):** the list orders
  by `greatest(last_updated_ledger, max(pool_state_changes.ledger_sequence))`.
  Measured on production: 699 of 770 Soroban pools are active later than
  their row says (250 days on average, 801 at worst), and no Soroban pool
  reached the first 5,000 rows of the full list; with the key 49 are in the
  first 1,000 and 127 in the first 5,000. 310 were active in the last 7 days.
- **Found while measuring 4b:** as one query the key cost 37–45M read_rows
  and ~350 ms per page against 8–10M / ~180 ms before. ClickHouse
  re-evaluates a `WITH` subquery at every reference and the list references
  its page CTE six times, so the 5.1M-row aggregate ran six times; the #455
  branch's "+3.5M" had measured one run. Decision (2026-09-24): two queries —
  pick the page (5.1M rows / 30–60 ms), then enrich those pool ids (8–14M /
  ~180 ms). Paging forward and back, the filters and the id lookup verified
  on the local API against production.

### 4b reworked: the activity key is stored, not derived per request (2026-09-24)

Decision 90 A replaced the two-query list: `pool_activity` (table) and
`pool_activity_mv` (refreshable MV, every 2 minutes, the `accounts_recent_mv`
pattern) keep each Soroban pool's last reserve change; the list is one query
again with `LEFT JOIN pool_activity`, ordered by
`greatest(last_updated_ledger, last_activity_ledger)`.

- **Plane filter added.** The MV keeps only `pool_state_changes` rows of the
  plane the pool declares in `pool_instance_state`, as the schema requires:
  a plane entry names its pool in an attacker-writable key. The first cut of
  4b took the max over every row. No foreign row exists today (0 of 5.02M;
  the order differed for 0 of 774 pools).
- **DDL run on production by the operator, 2026-09-24 ~11:01 UTC**; first
  refresh 11:02 UTC, 774 rows, latest activity 14 ledgers behind the tip, no
  exception in `system.view_refreshes`. The MV's SELECT reads 5.0M rows in
  ~0.1 s per refresh.
- **Cost per list page** (local API against production, `query_log`):
  7–15M rows, 170–490 ms — against 37–45M for the single query that derived
  the key inline and 5.1M + 8–14M for the two-query version.
- **Deploy order:** the API reads `pool_activity`, so the table must exist
  before the API deploy — done.
- **PR 3 is live after all.** The hold (decision 2026-09-23) was overtaken:
  the production API Lambda was updated from `develop` 2026-09-24 09:33:49
  UTC and the SPA redeployed at 11:01:57 UTC (its bundle carries
  `pool_kind`). Between the two the pool list crashed on the old SPA — task 0582. Until 4b–4d and PR 5 ship, a Soroban pool on production shows `—` for
  reserves, TVL and shares, zeros for participants and activity, and sits
  deep in the default order (reachable through the kind filter).

### 4c — Soroban reserves, shares and TVL (2026-09-24)

Branch `feat/0374-soroban-pool-reserves`. Measured through the local API
against production, all 770 Soroban pools:

|                         | before 4c | after 4c                         |
| ----------------------- | --------- | -------------------------------- |
| legs with a reserve     | 0 / 1,553 | 1,549 (269 of them `0`)          |
| pools with total shares | 0 / 770   | 705 (130 of them a measured `0`) |
| pools with a TVL        | 0 / 770   | 527                              |

- **Reserves** come from `pool_state_changes` on the plane the pool declares
  (95 C: every reader carries the filter, with a comment saying why), scaled
  only by decimals that are a fact. Unscalable: 4 legs of Soroban tokens with
  no published metadata and 1 unknown asset. Verified against on-chain
  `get_reserves` for 4 pools (constant, stable 3-leg, concentrated) — exact.
- **Total shares** from `pool_instance_state`, scaled by the share token's
  decimals (NULL when unpublished — 0 today). Decision 96 A: a stored 0 reads
  `0` for a pair-factory pool or a pool whose every reserve is 0, else `null`.
  Measured: 83 of 84 constant and 39 of 39 stable router pools with a 0 hold
  nothing; two sampled on chain answer `get_total_shares() = 0`; the one
  constant pool holding reserves (`CALL3ZZS…`) has no `get_total_shares`.
- **Volume** on a Soroban detail is `null` instead of `$0.00` — nothing
  records it, and an empty window used to read as a zero-volume day.
- **Cost:** a 20-row Soroban list page reads 8.1M rows (4b shape 7.5M); the
  reserve read is bounded by the oldest `pool_activity` ledger among the
  page's Soroban pools (0.25M rows instead of 2.66M). Detail 0.6M rows /
  ~130 ms.
- **Tests:** the CH-gated list test now also pins the reserve plane filter
  (red without it: the foreign row's `99999` / `0.0000001`) and the reserve
  bound: a pool registered after its last change, and one the refresh has
  not reached (red under the old activity-key bound, and under a bound that
  skips a missing `pool_activity` entry).
- **Known gap, accepted (review of the 4c follow-ups):** when a pool
  declares a NEW plane, `pool_activity` still holds the old plane's maximum
  until the next refresh. If the new plane's latest row sits below that
  value, the pool's reserves read `null` for up to 2 minutes. Reasoned, not
  reproduced: 0 of 813 pools in `pool_instance_state` have ever declared a
  second plane (production, 2026-09-24). Empty, never wrong, so no code
  change.
- **Simplify pass (2026-09-24).** An asset's decimals are one `Option<u32>`
  (`None` = not a fact) instead of a number plus a trust flag; a share
  token's decimals come from the shared identity resolution, dropping a
  per-request whole-table read of `soroban_contracts` and
  `soroban_contract_metadata`.
- **For 4d (decision 109 A):** the Soroban volume guard sits in the detail
  handler, but the cause is `usd_analytics`, which reads "no snapshot rows"
  as a zero-volume day and still runs the volume query for a Soroban pool.
  Move the rule into `PoolPriceContext` when the chart needs it.
- **Known limitation (decision 110 A):** the writer stores `total_shares = 0`
  when the pool's key is absent, so the API infers from the pool type and
  reserves whether a 0 was measured (96 A). The root fix (a nullable
  column, an indexer change and a backfill) is not worth a task while the
  rule covers every measured case; pools that keep their supply on the
  share token would read 0 even then.

### The plane filter guarded nothing since C′ — dropped from the API reads (2026-09-25)

Decision 113 E. The review asked why the Soroban reads were so involved;
the main cause was the declared-plane filter (95 C), and it is obsolete:

- Before C′ reserve rows came from a plane's `[PoolData, pool]` entry, a key
  any contract could publish under another pool's id. Since C′ (deployed
  2026-09-16) every row is decoded from the pool's own instance, keyed on the
  entry's owner (`pool_state.rs`, `stage.rs`), so a contract writes only under
  its own id.
- Production, 2026-09-25: 0 of 5,040,494 `pool_state_changes` rows come from
  a plane the pool does not declare; 0 rows without a declaration; 0 of 775
  pools with more than one plane.
- The filter also hid a re-pointed pool's reserves until its next move (gap
  104). The CH-gated test now pins that case (red with the filter restored:
  `[None, None]`), replacing the foreign-row scenario C′ made impossible.
- The `minIf` ledger bound went with it: the pool endpoints see 36 production
  requests a day (`query_log`, 24 h), and the unbounded read costs 2.4M rows /
  57 ms for a page of the busiest pools. A plain view (113 D) was measured and
  dropped: its reason was to keep the filter in one place.
- `pool_activity_mv` still filters in `init.sql` (DDL to change); task 0581
  rebuilds that view and drops it there.

**Other per-request rebuilds (114 A, sweep of `crates/api`):** the NFT list
derives its sort key (mint ledger) from all of `nft_ownership` per page
(23k rows today, cheap); contract list/detail count 7-day invocations per
request (99.5M rows for the hottest contract). Neither is worth a task at
current traffic. The one large background cost found on the way,
`balance_aggregates_mv` at 45% of database CPU, is task 0583.

### The 7-day "freshness window" is a leftover — removed from 4c, one left for PR 5 (2026-09-24)

The PG design treated a pool with no snapshot in 7 days as stale and blanked
its dynamic fields. The ClickHouse port (0243) dropped that for list and
detail but kept it in the participants endpoint and in the frontend's
`isPoolStale` caption. It no longer describes anything: a classic pool
writes a snapshot on every change of its ledger entry (52,284 of 52,284
pools have their latest snapshot at their last change), so an old snapshot
is a quiet pool's CURRENT state. On 2026-09-24 the caption "no recent
snapshot" sat under correct values on 29,284 of 53,554 classic pools (55%).

- **Removed in 4c (decision 99 A):** `isPoolStale` and its caption; the DTO,
  handler, table and module comments that still described the window.
- **For PR 5:** `list_participants.rs` still reads `total_shares` only from a
  snapshot inside `FRESHNESS_WINDOW_LEDGERS`, so a quiet pool's participants
  lose their share percentage. Drop the window there too.

### Soroban pools are half-indexed — the band-aid map and the new split (2026-09-25)

A sweep of the pool read path and the rest of the API (two review agents,
every finding read in code) traced most pool band-aids to one root: the
indexer stores a Soroban pool's STATE but not its operations or holders,
while the read path presents both kinds alike. Fundamentally the indexer
should write the same four facts for every pool, classic or Soroban:

| Fact                                     | Classic                            | Soroban today                                         | Band-aid it causes                                                           |
| ---------------------------------------- | ---------------------------------- | ----------------------------------------------------- | ---------------------------------------------------------------------------- |
| Registry (family, fee, legs)             | yes                                | family inferred from `pool_type_raw = ''`; fee frozen | family guessing in the zero-shares rule                                      |
| State per ledger (reserves, shares)      | yes                                | reserves yes; shares `0` stored for "key absent"      | `Reserves` Pair/Raw; `zero_shares_is_measured`; concentrated shares read "—" |
| Operations (swap/deposit/withdraw, amts) | `lp_operation_amounts`, volume col | none                                                  | volume guard in `handlers.rs`; activity and chart said "none"                |
| Holders                                  | `lp_positions`                     | none (they are the share token's holders)             | participants said 0                                                          |

**Volume was planned and lost in the split (decision 122 B).** Step C above
(`soroban_pool_trades`, ~4.16M-row backfill, Δreserve check) and a
read-time version (`0ff56188`, 2026-09-01, from `soroban_events`) existed;
the PR 1–7 split of #455 carried neither. It returns as W1 below.

**Interim honesty, in #496 (decision 123 A):** `participant_count` is
`null` for a Soroban pool, and its chart, participants and activity
sections say "Not indexed yet" instead of their empty states.

**Remaining split (decision 126, supersedes the list under "PR 4 split"):**

| PR  | Scope                                                                                              | Write/read |
| --- | -------------------------------------------------------------------------------------------------- | ---------- |
| 5   | Soroban participants = share-token holders from `balances`; drop the 7-day window                  | read       |
| W1  | Stage Soroban pool operations (swap, deposit, withdraw) with amounts + backfill; per-ledger volume | write      |
| W2  | `total_shares` nullable, concentrated pools' shares key, stored `pool_family`, instance backfill   | write      |
| 4d  | Soroban chart: reserves + W1 volume; volume rule moves into `PoolPriceContext` (109 A)             | read       |
| 7   | Soroban activity from W1                                                                           | read       |
| 6   | Drop the pair columns (deployment window)                                                          | write      |

W2 removes the zero-shares inference (110) and the family guess; W1 removes
the volume guard (109). Classic/Soroban storage unification (ADR 0058) and
the activity column (0581) stay where they are.

**Outside the pools** (same sweep): guessed 7 decimals in account balances,
balance changes and asset supply show wrong numbers today (USST 10^11 too
large); task 0473 covers only the parser, not the rendering — decision 119
pending. Merged accounts shown open: 0321. `resolves_on_asset_page`: 0542.
Smaller defaults that render a plausible wrong value: task 0584 (127 A).
The read-only user's refused `join_use_nulls`, behind most `nullIf` /
`toNullable` tricks, is an infra setting left to the operator.
