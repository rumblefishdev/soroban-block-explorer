---
id: '0340'
title: 'NFT collection_name from ledger instance-storage (parser-first; name() RPC demoted to fallback)'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0231', '0306', '0301', '0212', '0297']
tags: ['nft', 'enrichment', 'xdr-parser', 'effort-small']
links: []
history:
  - date: '2026-06-30'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from a 2026-06-30 investigation into why nft_enrichment.collection_name
      is 0% populated. Measured on prod CH (sorban-prod): of 68 hot NFT collections,
      0 have a name in soroban_contract_metadata (Symbol("METADATA")) and 0 in
      soroban_contracts.name (Symbol("name")). Concluded (WRONGLY — see 2026-07-13)
      the name is not in any parsed storage slot and needs a name() RPC simulate.
  - date: '2026-07-02'
    status: active
    who: stkrolikiewicz
    note: >
      Promoted to active. Shipped the name()-RPC fetcher + per-contract backfill
      subcommand (PR #301). AC-1 prod drain deferred behind the enrichment-worker
      conc=0 hold. (Superseded as PRIMARY by the 2026-07-13 redirect; retained as
      the fallback path for WASM-baked names.)
  - date: '2026-07-13'
    status: active
    who: stkrolikiewicz
    note: >
      REDIRECTED to parser-first. The 2026-06-30 "storage-parse is 0%" premise was
      a matcher bug, not ground truth: token_metadata::extract_token_metadata
      matched only Symbol("METADATA"); OpenZeppelin NFTs store the name under
      NFTStorageKey::Metadata → ScVal::Vec([Symbol("Metadata")]) (different variant
      AND casing) and were silently dropped. Live-verified via getLedgerEntries the
      name IS in instance storage (CARTUL5A "SushiSwap V3 Positions NFT-V1",
      CAKSC7JH "Minah"). Fix A (parser, PR #330, MERGED) + Fix B (serving, PR #331)
      source it from the ledger. name() RPC demoted to fallback.
---

# NFT collection_name from ledger instance-storage (parser-first)

## Summary

The NFT collection name is captured **from the ledger** — the contract
instance-storage metadata struct — into `soroban_contract_metadata.name` by the
indexer, and served as `COALESCE(soroban_contract_metadata.name,
nft_enrichment.collection_name)`. No RPC on the primary path. The contract-level
SEP-50 `name()` RPC simulate (the original approach) is retained only as a
**fallback** for hand-rolled contracts that bake the name into WASM and leave
instance storage empty.

## Context — the premise was a matcher bug

The 2026-06-30 measurement (`soroban_contract_metadata.name` 0/68 for NFT
collections) was read as "the name is not in on-chain storage → use `name()`
RPC". That 0% was an **extraction bug**, not ground truth:

- `token_metadata::extract_token_metadata` matched only the storage key
  `Symbol("METADATA")` (SEP-41 / OZ **fungible** tokens and SACs).
- OpenZeppelin **NFTs** store metadata under `NFTStorageKey::Metadata`, which
  serializes as `ScVal::Vec([Symbol("Metadata")])` — a different `ScVal`
  variant AND casing. The extractor never looked at it → silently dropped.

Live-verified 2026-07-13 via mainnet RPC `getLedgerEntries` that the name is in
instance storage: `CARTUL5A…` → `["Metadata"] = {name: "SushiSwap V3 Positions
NFT-V1", …}`, `CAKSC7JH…` → `{name: "Minah", …}`. `name()` via RPC just executes
a function that reads this same slot. The instance ContractData already flows
through the indexer parse+persist path — only the key matcher dropped it.

## Implementation

### Fix A — parser (PR #330, MERGED to develop)

`is_metadata_key` in `crates/xdr-parser/src/token_metadata.rs` now matches BOTH
`Symbol("METADATA")` and the OZ NFT `Vec([Symbol("Metadata")])`. The indexer
writes the NFT collection name into `soroban_contract_metadata.name` straight
from ledger meta (no RPC). Real-XDR regression
`extracts_real_mainnet_oz_nft_instance_from_ledger` decodes the actual CARTUL5A
instance bytes. `docs/architecture/xdr-parsing/xdr-parsing-overview.md` updated.

### Fix B — serving (PR #331, open, ClickHouse-verified)

`collection_name` served as `COALESCE(soroban_contract_metadata.name,
nft_enrichment.collection_name)` — ledger authoritative, enrichment fallback —
across all four surfaces: list, detail, search NFT-bucket label, and
`filter[collection]` (with ledger precedence, so a ledger-shadowed enrichment
name cannot match). Joined by contract StrKey via the existing surrogate↔StrKey
mapping. No schema/DTO/openapi change. Verified on CH 26.3 over a 4-case fixture.

### Backfill — free on the Phase-3 archive re-parse

`backfill-runner Run` (the lore-0359 archive re-parse) writes
`soroban_contract_metadata` (table #4 of 14 in `db-clickhouse persist/writer.rs`).
With Fix A in the deploy build **before** the re-parse, the same re-parse
populates NFT collection names for existing contracts — **no separate backfill
pass**. (getLedgerEntries can't backfill: head-only, ~7-day retention; must use
the raw S3 `aws-public-blockchain` per-ledger meta.)

## Acceptance Criteria

- [x] NFT collection name captured from the ledger into
      `soroban_contract_metadata.name` (Fix A, PR #330; real-XDR regression test).
- [x] Served as `COALESCE(ledger, enrichment)` on list / detail / search +
      `filter[collection]` consistent with the displayed value (Fix B, PR #331;
      CH 26.3-verified, incl. precedence).
- [ ] Prod populated — deferred to the Phase-3 / lore-0359 archive re-parse (Fix A
      must be in the deploy build first). Measure `with_name / collections` after.
- [x] `name()` RPC demoted from primary to fallback (PR #301 work retained for
      WASM-baked names; not run on the primary path).
- [x] **Docs updated** — `xdr-parsing-overview.md` (Fix A) + this task doc. Follow-up:
      `enrichment.md` / runner README still describe `name()` RPC as the source —
      re-frame as fallback (small doc touch).
- [x] **API types** — N/A for Fix A (no `crates/api/**`); Fix B `check-generated`
      green (query-only change, no openapi drift).

## Issues Encountered

- **False "0%" from a key-matcher bug** (root cause of the whole redirect). The
  extractor's `symbol_is(key, "METADATA")` predicate is an exact `ScVal::Symbol`
  match; OZ NFTs use a `Vec`-wrapped enum key with different casing. The measurement
  that drove the name()-RPC approach was measuring the bug, not the chain. Lesson:
  verify "not in storage" claims against raw `getLedgerEntries`, not against a
  side-table populated by a possibly-incomplete extractor.

## Design Decisions

### Emerged

1. **Parser-first over name()-RPC** (2026-07-13). The name is in instance storage
   for OZ NFTs (the dominant standard); parsing it is free on the live path and
   backfills for free on the already-planned lore-0359 re-parse. name()-RPC stays
   as the fallback for the hand-rolled minority (e.g. Bachini `CDA5FGE4…`: empty
   instance storage, name="SorobanNFT" baked in WASM).
2. **Reuse `soroban_contract_metadata`** (not a new column/table). It is already
   the per-contract on-chain metadata table keyed by StrKey; NFT rows land there
   exactly like SEP-41 tokens. The asset read-path joins it by `contract_id`, so
   NFT rows never collide with asset rows.
3. **`filter[collection]` precedence** (Fix B, from code review). Filters by ledger
   name, OR enrichment only when the contract has no ledger name — matches the
   served COALESCE so a shadowed enrichment name can't be filtered.

## Future Work

- Run the prod backfill via the Phase-3 re-parse; measure population.
- Re-frame `enrichment.md` / runner README: ledger primary, `name()` RPC fallback.
- Decide PR #301's fate (keep as fallback impl, or fold into a leaner fallback).
