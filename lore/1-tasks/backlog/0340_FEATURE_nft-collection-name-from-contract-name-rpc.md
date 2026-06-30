---
id: '0340'
title: 'NFT collection_name: source from contract name() RPC simulate (storage-parse is 0%)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0231', '0306', '0301', '0212']
tags: ['phase-future', 'effort-small', 'priority-low', 'nft', 'enrichment']
links: []
history:
  - date: '2026-06-30'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from a 2026-06-30 investigation into why nft_enrichment.collection_name
      is 0% populated. Measured on prod CH (sorban-prod): of 68 hot NFT collections,
      0 have a name in soroban_contract_metadata (Symbol("METADATA")) and 0 in
      soroban_contracts.name (Symbol("name")). The collection name is not in any
      parsed storage slot — SEP-50 exposes it only via the name() contract function,
      reachable only by RPC simulate. Existing tasks (0212/0301/0306) assume it
      arrives via token_uri() JSON, which no contract emits. This task corrects the
      source.
---

# NFT collection_name: source from contract `name()` RPC simulate

## Summary

`nft_enrichment.collection_name` is already served (list / detail / search via
`queries_ch.rs`) but is **0% populated** on prod. The existing token_uri
enrichment reads a per-token JSON `"collection"` field that no Stellar NFT
emits, and both parsed on-chain storage slots are empty too. The real source is
the contract-level SEP-50 `name()` function, retrievable only via Soroban RPC
`simulateTransaction`. This task swaps the source: add a **per-contract-cached**
`name()` simulate to the NFT enrichment and write its result into
`nft_enrichment.collection_name`.

## Context

Measured on prod CH (`sorban-prod`, `app-clickhouse-1`) 2026-06-30:

- 68 distinct NFT collections in hot `nfts`.
- `soroban_contract_metadata.name` (from instance-storage `Symbol("METADATA")`
  struct, task 0297 / 0304 backfill): **0 / 68** with a name.
- `soroban_contracts.name` (from `Symbol("name")` ContractData, ADR 0042 /
  task 0156): **0%** (sample ordered `name DESC NULLS LAST` returned all NULL).
- Conclusion: the NFT collection name is in **neither** parsed storage slot.
  SEP-50 stores it behind the `name()` function only → needs an RPC call, not a
  storage parse. This answers the recurring "indexer or enrichment?" question:
  **enrichment** (the RPC layer), not the indexer (which only parses ledger
  storage).

Why this is low-risk on the serving side — everything downstream already exists:

- `nft_enrichment.collection_name Nullable(String)` exists, RMT(version), joined
  at read in `crates/api/src/nfts/queries_ch.rs` and `search/queries_ch.rs`. No
  schema change, no new table, no skip-index (none exists today; nothing to
  duplicate).
- The enrichment path already performs RPC `simulateTransaction` for
  `token_uri()` (`crates/enrichment-shared/src/nft_token_uri/`). The same client
  / decode path serves `name()`.

Corrects a stale assumption: `0212`, `0301`, and `0306` all describe
`collection_name` as arriving from `token_uri()` JSON metadata. It does not, and
those ACs cannot go green for `collection_name` without this change.

## Implementation Plan

### Step 1: per-contract `name()` fetch

Add `simulateTransaction(InvokeContract(contract, "name", []))` →
`ScVal::String` → trimmed + VARCHAR(256)-capped, mirroring the existing
`token_uri()` decode. `name()` is parameterless and per-collection — **cache by
`contract_id`** and call at most once per contract per run (68 distinct → 68 RPC
calls for a full drain; reuse the existing moka cache pattern). Do **not** call
per token.

### Step 2: write into `nft_enrichment.collection_name`

Persist via the existing `nft_enrichment` write path (same row + `version`
clock as `name` / `media_url`). Keep the empty-string sentinel and the
`real > sentinel > NULL` precedence so a later run upgrades a sentinel in place.

### Step 3: missing / non-standard `name()`

Contracts that do not export `name()` (custom-ABI / 0308 families) → write the
sentinel, classify permanent, no transient-retry storm.

### Step 4: one-shot backfill

Drain hot NFT contracts whose `collection_name` is NULL through the same fetch
(≤ 68 contracts). Slots into the `0306` NFT-enrichment prod pipeline step.

## Acceptance Criteria

- [ ] `nft_enrichment.collection_name` populated for NFT collections that export
      `name()` (measure `with_name / 68` on prod after backfill).
- [ ] `name()` fetched at most once per contract per run (cached), never per
      token.
- [ ] Contracts without `name()` write the sentinel; no retry storm.
- [ ] Served `collection_name` (list / detail / search) reflects the fetched
      value.
- [ ] Stale "from token_uri JSON" assumption corrected in the `0212` / `0301` /
      `0306` notes.
- [ ] **Docs updated** — update the enrichment / ingestion doc under
      `docs/architecture/**` to record `name()` RPC as the `collection_name`
      source per [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — `N/A` — no change under `crates/api/**` DTOs,
      `Cargo.{toml,lock}`, or `libs/api-types/**` (the `collection_name` field
      already exists in the DTO; this only changes its data source).

## Notes

- Volume is per-contract (68), not per-token (12.8k). A read-time semi-join over
  `soroban_contract_metadata` was considered but ruled out: the name is not in
  that table at all (0%), so there is nothing to join to until it is fetched.
- Custom-ABI collections (0308) that rename or omit `name()` stay NULL — out of
  scope here.
