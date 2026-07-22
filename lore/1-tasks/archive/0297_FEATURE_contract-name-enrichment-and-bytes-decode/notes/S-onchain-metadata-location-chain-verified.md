---
prefix: S
title: 'Where do contract name/symbol/decimals actually live? — chain-verified'
status: mature
spawned_from: ['0297', '0283']
date: 2026-06-17
who: karolkow
---

# Synthesis: name/symbol/decimals are ON-LEDGER in the instance-storage `METADATA` struct

> **Method note.** Treating the blockchain as the only ground truth. Every prior
> doc (task 0156, task 0283 open-problem #2, task 0297 body, Staszek's hand-off)
> is one person's _interpretation_ and was checked against live mainnet, not
> trusted. Evidence below is reproducible (`/tmp/sbe-chain-check`, raw RPC).

## TL;DR verdict

1. **Contract `name`, `symbol`, AND `decimals` ARE on-ledger** for the dominant
   token pattern — packed into ONE struct stored in the contract's **instance
   storage** under the key `Symbol("METADATA")`:
   `{"decimal": <u32>, "name": <String>, "symbol": <String>}`.
   Verified on 10/37 live mainnet contracts (7 SAC + 3 WASM).
2. **Our parser throws this away.** `scval.rs:84` (`ScVal::ContractInstance` arm)
   serialises only `executable` and **drops `inst.storage`** — the map that holds
   `METADATA`. Confirmed by reading the code, not just the hand-off.
3. **The on-ledger name-write path looks in the wrong place.** `is_symbol_name_key`
   (`state.rs:199`) matches a _standalone_ persistent `Symbol("name")` ContractData
   entry. **0 of 37 live contracts have one.** Real tokens nest the name inside
   the `METADATA` struct. That is why `soroban_contracts.name` is empty for
   **0 of 424,220** (the "false zero").
4. **"Names are off-ledger" (task 0283 #2 / task 0297 body) is REFUTED.** It was
   generalised from a single unrepresentative sample (Bachini NFT) plus a
   tautological parser self-check. Bachini really does have empty instance
   storage — but it is an outlier, not the rule.
5. **The decimals question (Karol, 2026-06-17) is the SAME fix.** `decimals` lives
   in the same `METADATA` struct (field name is singular: `decimal`). Different
   tokens can carry different values; it is load-bearing for human-readable
   amount display. No separate extraction needed — recover it alongside name/symbol.

## The three prior interpretations, and what the chain says about each

| Source                                         | Claim                                                                                                                                | Chain verdict                                                                                                                                                                                                                                                    |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Task 0156** (Staszek, completed)             | name is a standalone `Symbol("name")` persistent ContractData entry written at deploy/init; extract it inline                        | **Wrong SHAPE.** Name is real and on-ledger, but nested in the `METADATA` struct in _instance_ storage, never as a standalone `Symbol("name")` entry. So the extractor (`is_symbol_name_key`) never matched → empty column.                                      |
| **Task 0283 open-problem #2 / Task 0297 body** | names are OFF-ledger; only readable via the `name()` WASM function; populate via RPC enrichment; "parser is correct, nothing to fix" | **Refuted for the dominant case.** Over-generalised from Bachini (empty storage) + a parser self-check whose fixtures encode the wrong shape. Name IS on-ledger in `METADATA`. The RPC-getter idea survives only as a _fallback_ for the Bachini-style minority. |
| **Staszek's hand-off (2026-06-16)**            | name/symbol/decimals are on-ledger in instance storage `METADATA`; `scval.rs` drops `inst.storage`; extractor looks for wrong shape  | **Confirmed** on both code and chain. Bachini was unrepresentative, exactly as the hand-off argued.                                                                                                                                                              |
| **Karol's intuition**                          | name is there, we just parse it wrong / drop it                                                                                      | **Confirmed.**                                                                                                                                                                                                                                                   |

## Gospel evidence — live mainnet `getLedgerEntries` (read-only)

Probed 53 real mainnet contract IDs (51 pulled from our own research archive
0003/0005/0008 — an independent sample — plus Bachini and the hand-off's two
liquidFi tokens). 37 returned a live instance entry (16 archived/evicted/absent).

**All 10 contracts that carry a `METADATA` struct** (key = `Symbol("METADATA")`
inside `ScContractInstance.storage`):

```
[SAC ] native        {"decimal":7,"name":"native","symbol":"native"}
[SAC ] CCW67TSZ...    {"decimal":7,"name":"USDC:GA5ZSEJYB37...","symbol":"USDC"}
[SAC ] CD25MNVT...    {"decimal":7,"name":"BLND:GDJEHTBE6Z...","symbol":"BLND"}
[SAC ] CDBR4FMY...    {"decimal":7,"name":"FXG:GAVH5ZWAC...","symbol":"FXG"}
[SAC ] CDIKURWH...    {"decimal":7,"name":"USDx:GAVH5ZWAC...","symbol":"USDx"}
[SAC ] (GBPx)         {"decimal":7,"name":"GBPx:GAVH5ZWAC...","symbol":"GBPx"}
[SAC ] (EURx)         {"decimal":7,"name":"EURx:GAVH5ZWAC...","symbol":"EURx"}
[WASM] (Comet)        {"decimal":7,"name":"Comet Pool Token","symbol":"CPAL"}
[WASM] CDKRSOVB...    {"decimal":7,"name":"liquidFi bridge token","symbol":"lUSDC"}
[WASM] CDPLIFG...    {"decimal":7,"name":"liquidFi LP token","symbol":"lUSDC"}
```

Tally: `has METADATA struct: 10` (all 10 have `.name`), `has standalone
Symbol("name"): 0`, wasm 30 / SAC 7.

Note other WASM contracts store metadata under their own ad-hoc keys
(`Name` capitalised in Blend pools `CCCCIQSD…`, `CDVQVKOY…`; `PoolMeta`,
`Config`, etc.), none under lowercase `Symbol("name")`. So even a "look for any
`Name`-ish key" heuristic would miss the struct; `METADATA` is the SEP-41/OZ
convention and the right target.

## Why 0283 saw "off-ledger" — the Bachini trap (getter cross-check)

`simulateTransaction` of `name()` / `symbol()` / `decimals()`:

```
USDC SAC      name()="USDC:GA5Z..."  symbol()="USDC"  decimals()=7      (getter works, matches storage)
liquidFi WASM name()/symbol()/decimals() -> "Bad union switch: 1"       (SDK can't decode the protocol-26
                                                                          simulate response — but the on-ledger
                                                                          METADATA read is clean)
Bachini NFT   name()/symbol() error; decimals() -> HostError MissingValue (NO working getter AND empty storage)
```

So Bachini has **neither** on-ledger `METADATA` **nor** a working `name()` getter.
It is an NFT — per-token metadata lives off-chain via `token_uri` (the path
`crates/enrichment-shared/src/nft_token_uri` already handles). 0283 dumped
Bachini, saw empty storage, and concluded "all names off-ledger via `name()`".
Both halves were unrepresentative: most tokens DO store `METADATA` on-ledger, and
for Bachini specifically `name()` does not even resolve.

Secondary observation: the SDK getter path errored (`Bad union switch: 1`) for
the WASM tokens while the direct storage read was clean. Reading `METADATA` from
the instance entry is both on-ledger AND more robust than the getter for the
contracts we care about — another mark against the RPC-getter enrichment route.

## Current code state (feat/0297 tip, post-0283 merge)

- `scval.rs:84` — `ContractInstance` arm emits `{ "executable": ... }`, **drops
  `inst.storage`**. The `METADATA` map never enters the pipeline. (Production
  code never reads `inst.storage`; only scval unit tests, with `storage: None`.)
- `state.rs:199` `is_symbol_name_key` — matches standalone `Symbol("name")`; never
  fires on real tokens.
- On-ledger name-write path: **disabled on the ClickHouse path + tripwired**
  (commit `38b8d0cc`: name-only RMT row pinned to `wasm_uploaded_at_ledger=0` so a
  real deploy always out-versions it; `tracing::error!` tripwire if
  `contract_name_writes` is ever non-empty). Still live (and harmless, always
  empty) on the Postgres path via `apply_contract_name_writes`.
- ADR 0049 (the "Family-A lossy-extraction" the hand-off cites) was **deleted**
  (commit `9ae270d2`); its framing was inlined into spawned tasks. Cite ADR 0053
  (separate side-table per writer) instead.

## Recommended direction (two-pronged; supersedes the body's RPC-only plan)

1. **Parser fix (primary) — recover `METADATA` from instance storage.**
   - Un-drop `inst.storage` in `scval.rs` _but_ prefer a TARGETED extractor that
     pulls only the `METADATA` struct → typed `name` / `symbol` / `decimals`, not
     the whole storage map (the scval serialiser is shared with operations/events/
     entry-changes; persisting full maps everywhere bloats JSON).
   - Handle the instance entry on BOTH `created` (constructor) AND `updated`
     (later `init()`) ContractData changes — today the deploy path only reads
     `created` (`state.rs:126`). **RESOLVED (Option B, chain-checked — see section
     below): handle BOTH.** Same created/updated asymmetry as the wasm-upgrade
     follow-up (task 0295).
   - This recovers name/symbol/decimals for SACs and OZ-style WASM fungibles
     (liquidFi, Comet, …) — the bulk of the false zero.
2. **Off-chain fallback (secondary) — for NFTs / non-METADATA tokens (Bachini).**
   No on-ledger struct → metadata comes from `token_uri` JSON, already handled by
   the `nft_token_uri` enrichment path (task 0231 family). The RPC `name()`-getter
   enrichment from the original 0297 plan is the weakest option (flaky decode,
   redundant where `METADATA` is on-ledger) — keep only if a real token is found
   that has a working getter but no `METADATA`.

### Target columns — DECISION 2026-06-18: Option D (ADR 0049)

> ⚠️ Supersedes the earlier draft here that split `name → soroban_contracts` vs
> `symbol/decimals → asset_enrichment`. That split was **wrong**: all three are
> on-chain, indexer-derived, on the SAME clock — they must NOT be split, and they
> do NOT belong in the off-chain enrichment tables. Decided after a red/blue
> review (see [[0297]] discussion) → **[ADR 0049](../../../../2-adrs/0049_soroban-contract-metadata-onchain-side-table.md)**.

All three (`name`, `symbol`, `decimals`) go into ONE new per-contract side table
**`soroban_contract_metadata`** (`contract_id`, name, symbol, decimals,
`version` = observed ledger), `RMT(version)`, written by the parser/indexer on
`created` AND `updated`. Read-composed via `LEFT JOIN` on `contract_id`.

- **Why a side table, not `soroban_contracts` columns:** RMT whole-row replace +
  `soroban_contracts`'s multi-writer reality (deploy / `contract_type_rebuild`
  EXCHANGE / stub INSERTs / db-merge) means in-table metadata columns get
  clobbered to NULL by any writer that omits them — the G5 bug class. The deploy
  identity (`wasm_hash`, `deployer`) and metadata are on different clocks; one RMT
  row can't track both. (Full reasoning: ADR 0049.)
- **SACs skipped:** name=`CODE:ISSUER`, symbol=code, decimals=7 already derivable
  from SAC identity — redundant, and would bloat the table.
- **`decimals`:** stored only for Soroban-native; classic/SAC render 7 (protocol
  constant) at read.
- `soroban_contracts.name` / `assets.name` stay as vestigial `COALESCE` fallbacks
  (DROP later).

### The "ScVal::Bytes name-decode bug" in the 0297 body — re-examine, likely moot

The body flags `scval.rs:45` (base64) vs `state.rs:243` (hex) as a producer/
consumer mismatch. These consume **different** JSON representations:
`decode_scval_string` reads the Stellar-XDR-native JSON (hex bytes, per its own
comment + the `name_writes_decodes_bytes_scval_utf8` test using `55534443` hex),
which is NOT the output of `scval_to_typed_json` (base64). They are not wired
producer→consumer. Verify the data flow before "fixing" — the mismatch may be a
false alarm. Even if real, it is moot once name comes from the `METADATA` struct
(a `String`, not `Bytes`).

## Option B — does `METADATA` arrive on a `created` or `updated` change? (chain-checked 2026-06-17)

**Answer: the `created` (constructor) path is confirmed, and the `updated` path
is demonstrably real too → the extractor must handle BOTH.**

- **Direct positive (constructor → `created`).** liquidFi LP (`CDPLIFG…`): its
  instance entry's `lastModifiedLedgerSeq` (60119171) has the SAME close time as
  the contract's creation (`getLedgerEntries` lastModified ledger close ==
  stellar.expert `created`, Δ = 0s), and the entry currently holds `METADATA`. An
  instance untouched since its creation ledger means `METADATA` was written in
  that ledger → it rode in on the deploy's `created` change (the
  `createContractV2` / `__constructor` pattern, protocol 22+, the modern default).
- **`updated` path is real.** USDC SAC (`CCW67…`): instance last modified ~796
  days AFTER creation — instance storage demonstrably mutates post-deploy. A
  deploy-then-`init()` token sets `METADATA` in a later tx → an `updated` change,
  which the current `created`-only deploy pass (`state.rs:126`) would miss.
- **Conclusion:** read the instance-storage map on BOTH `created` and `updated`
  ContractData changes for the `LedgerKeyContractInstance` key.

**Limitation (honest).** A byte-level read of a _specific historical_ deploy's
meta is NOT possible from a dev box: Soroban RPC retains ~7 days (oldest ledger
62946928 at check time), SDF Horizon no longer serves `result_meta_xdr`
(`has result_meta_xdr: false`), and Horizon's per-ledger history is trimmed (old
ledgers return `closed_at: null`). The authoritative direct confirmation —
replay the deploy ledger's `LedgerCloseMeta` and inspect the instance
`created`/`updated` change — needs our own galexie/S3 archive (clean follow-up).
Tooling is ready: `@stellar/stellar-sdk@16` decodes protocol-26 (V4) meta;
`@13` did not (`Bad union switch`).

## Reproduction

`/tmp/sbe-chain-check/` (throwaway, outside repo):

- `dump.mjs` — builds an instance `LedgerKey` per contract (`Contract.getFootprint()`),
  `getLedgerEntries`, decodes `ScContractInstance.storage`, prints `METADATA`.
- `getter.mjs` — `simulateTransaction` of `name()`/`symbol()`/`decimals()`.
- Endpoint: `https://mainnet.sorobanrpc.com`. SDK: `@stellar/stellar-sdk@^13`.
- The repo's own `crates/backfill-runner/src/rpc_snapshot.rs` is the Rust
  equivalent of the read path (currently decodes Account/Trustline only).
