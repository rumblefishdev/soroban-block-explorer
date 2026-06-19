---
id: '0049'
title: 'On-chain Soroban token metadata (name/symbol/decimals) in a per-contract side table, not on soroban_contracts'
status: accepted
deciders: [karolkow]
related_tasks: ['0297', '0283', '0231']
related_adrs: ['0048', '0042']
tags: [clickhouse, soroban, schema, write-strategy, side-table, metadata]
links:
  - lore/1-tasks/active/0297_FEATURE_contract-name-enrichment-and-bytes-decode/notes/S-onchain-metadata-location-chain-verified.md
history:
  - date: '2026-06-18'
    status: accepted
    who: karolkow
    note: >
      Accepted after chain verification (mainnet getLedgerEntries) + an
      independent red-team/blue-team review. Realizes the "route metadata to a
      side table" intent the 0283 G5 tripwire already references as "ADR 0049"
      (the earlier 0049 stub was dropped; this is its concrete realization).
---

# ADR 0049: On-chain Soroban token metadata in a per-contract side table

**Related:**

- [Task 0297](../1-tasks/active/0297_FEATURE_contract-name-enrichment-and-bytes-decode/README.md) — implementation.
- [Chain-verified synthesis](../1-tasks/active/0297_FEATURE_contract-name-enrichment-and-bytes-decode/notes/S-onchain-metadata-location-chain-verified.md) — evidence + red/blue.
- [ADR 0048](./0048_clickhouse-separate-tables-for-two-writer-columns.md) — the side-table-per-writer precedent this generalizes.
- [ADR 0042](./0042_soroban-contracts-typed-name-column.md) — added `soroban_contracts.name` (now superseded as the metadata home).

## Context

Soroban token `name` / `symbol` / `decimals` are **on-ledger**, packed into one
struct in each contract's **instance storage** under `Symbol("METADATA")` =
`{decimal: u32, name: String, symbol: String}`. Chain-verified 2026-06-17 on
mainnet (10/37 live contracts carry it: every SAC + WASM tokens liquidFi, Comet;
0/37 carry a standalone `Symbol("name")`). This **refutes** the earlier 0283/0297
"names are off-ledger" verdict, which had over-generalized from one sample
(Bachini NFT, empty instance storage) and a parser self-check whose fixtures
encoded the wrong key shape. The parser (task 0297) now extracts the METADATA
struct from every contract-instance ledger-entry change (`created` AND `updated`).

The question this ADR answers: **where do we persist it on ClickHouse?**

Constraints:

- CH tables are `ReplacingMergeTree` (RMT): merges keep the **whole** row with the
  highest version column; **no cheap per-column UPDATE**; mutations avoided
  project-wide.
- `soroban_contracts` is `RMT(wasm_uploaded_at_ledger)` keyed by `contract_id`. Its
  identity columns (`wasm_hash`, `deployer_id`, `deployed_at_ledger`) come from the
  **deploy transaction context** — which is **not** in the instance entry. So a
  metadata write derived from a (possibly later) instance change cannot
  reconstruct those columns.
- Ledger entry changes are **full snapshots**: an `updated` instance carries
  `executable` (→ wasm_hash) + ALL instance storage — but still not the
  tx-context deploy fields above.
- `soroban_contracts` is written by **multiple** paths today: the indexer deploy
  write, the `contract_type_rebuild` full-table `INSERT…SELECT` + `EXCHANGE
TABLES`, two stub-row INSERTs (enrichment-worker / nft_token_uri), and the
  db-merge step.
- History: the "G5" bug — a partial `name`-only row clobbered `wasm_hash` to NULL
  via RMT whole-row replace. It was disabled + tripwired; the tripwire says
  _"route contract names to an enrichment side table … do not rely on
  soroban_contracts.name."_

## Decision

**Persist on-chain Soroban token metadata in a dedicated per-contract side
table, written only by the indexer/parser, composed at read time. Do NOT add
metadata columns to `soroban_contracts`.**

- New table **`soroban_contract_metadata`**:
  `(contract_id, name Nullable(String), symbol Nullable(String), decimals
Nullable(UInt32), version)`, `ENGINE = ReplacingMergeTree(version) ORDER BY
(contract_id)`. **`version` = the ledger sequence the metadata was observed**
  (deterministic / replay-safe; not wall-clock).
- The parser/indexer writes a row whenever it observes a `METADATA` struct on a
  contract-instance `created` OR `updated` change. **SACs are skipped** — their
  name (`CODE:ISSUER`), symbol (=asset code) and decimals (=7) are already
  derivable from the SAC identity (ADR 0038 / task 0160), so storing them here is
  redundant and would bloat the table to ~all contracts.
- **`decimals`** is rendered as the protocol constant **7** at read time for
  classic / SAC assets (Stellar classic precision is fixed at 7); the column is
  stored only for Soroban-native tokens, where it can differ.
- The API read-composes: `LEFT JOIN soroban_contract_metadata ON contract_id`,
  `COALESCE(meta.name, …)` for name/symbol; decimals from the table for Soroban,
  constant 7 otherwise.

## Rationale

This is ADR 0048's rule, generalized: **when disjoint columns of one entity have
different update clocks, give them their own RMT table keyed identically and
compose at read.** The trigger is not "two writer processes" but "writes on
different clocks into a whole-row-replace table." Here:

- **Different clocks.** Deploy identity is fixed at the deploy ledger; metadata
  can be set/changed on a different ledger (constructor, later `init()`, rename).
  One RMT row has one version column — it cannot track both timelines without
  losing one.
- **No clobber, by construction.** The metadata table is never touched by the
  deploy write, the rebuild `EXCHANGE`, the stub INSERTs, or db-merge — so none of
  them can null metadata, and the parser need not read-before-write to preserve
  deploy fields it does not have.
- **Reuses a shipped pattern.** `asset_enrichment` / `nft_enrichment` already do
  exactly this (RMT(version) side table, read-compose); the `nft_enrichment`
  schema comment states the same rationale verbatim.
- **The parser already extracts `updated` metadata** (task 0297) — this design
  uses it; the rejected in-table option would discard it.

## Alternatives Considered

### A — add `symbol`/`decimals` columns to `soroban_contracts`, write at `created`

**Rejected.** Re-arms the G5 clobber class. It makes the metadata columns inherit
`soroban_contracts`'s entire multi-writer set: the `contract_type_rebuild`
full-table `INSERT…SELECT` + `EXCHANGE` would null metadata for **every** contract
at once if its explicit column list ever omits them; the stub-row INSERTs are
partial by design; db-merge already hand-writes per-column `COALESCE` to avoid
clobber on PG — and **ClickHouse RMT offers no per-column defense at all**.
It also discards the parser's `updated`-path metadata (the deploy pass is
`created`-only) and couples two update clocks into one row+version. "Simpler, no
JOIN" is true only until the next writer forgets the invariant.

### C — hybrid (A at deploy + side-table top-up for updates)

**Rejected.** The side table alone already handles `created` + `updated`
uniformly; the hybrid pays both costs to cover a case B already covers.

### Compute-at-read (RPC `name()` / instance read on demand)

**Rejected** as the primary store. List views need name/symbol/decimals fast and
in bulk (e.g. amount rendering); per-row RPC is too slow and hits the
archived/evicted liveness ceiling. (Remains a possible fallback only for the rare
contract with no on-ledger metadata, e.g. Bachini-style NFTs, which are already
served by the off-chain `nft_enrichment` / `token_uri` path.)

### Put metadata on `assets`

**Rejected.** `assets` is re-written on every supply change (higher write churn
than `soroban_contracts`), so it is a _worse_ clobber surface, and it is keyed by
the asset tuple, not `contract_id`.

### Reuse `asset_enrichment` for symbol/decimals

**Rejected.** That table is owned by the off-chain enrichment worker. Having the
parser also write it re-introduces the exact two-writer mixing ADR 0048 forbids,
and conflates on-chain vs off-chain provenance.

## Consequences

- **New table + read JOIN.** Bounded: the table holds only Soroban-native tokens
  (~thousands), and the read path already does multi-LEFT-JOIN composition.
  **Action: validate JOIN/`FINAL`/`argMax` cost on the contracts + assets list
  endpoints against the CH snapshot before flipping the read flag** (the codebase
  has a `read_rows`-quota history).
- **Backfill required.** The table starts empty; existing contracts need a
  one-shot fill (re-parse historical instance changes, or RPC `getLedgerEntries`
  where instances are still live — archived instances need re-parse). Tracked as
  task 0297 follow-up.
- **`decimals` default.** Classic/SAC render 7 at read; only Soroban-native store
  the column.
- **`soroban_contracts.name` (ADR 0042) + `assets.name` become vestigial**
  fallbacks (empty in practice). Kept in the read `COALESCE` for now; DROP is a
  later cleanup.
- **API change** (read-compose + new symbol/decimals response fields) regenerates
  `libs/api-types` (codegen gate).
- **G5 tripwire** in `stage.rs` is replaced by this real write path; its "ADR
  0049 side table" reference is now realized by this ADR.
- **Docs** (ADR 0032 evergreen): schema, xdr-parsing, and API docs updated in the
  implementing PR.
