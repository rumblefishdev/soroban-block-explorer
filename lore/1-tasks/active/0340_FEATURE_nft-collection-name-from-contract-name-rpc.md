---
id: '0340'
title: 'NFT collection_name: source from contract name() RPC simulate (storage-parse is 0%)'
type: FEATURE
status: active
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
  - date: '2026-07-02'
    status: active
    who: stkrolikiewicz
    note: >
      Promoted to active to start implementation. Pre-work code audit confirmed
      heavy reuse (envelope builder already zero-arg-capable, same ScVal::String
      decode, moka/RPC-pool/persist all in place). Three flagged design points:
      (1) collection_name is per-token in an RMT whole-row-replace table — backfill
      must rewrite full rows; (2) runner's Sentinels mode requires ALL columns
      empty so it skips the name/media-populated rows — backfill goes per-contract
      (68 name() calls + row rewrite preserving name/media_url); (3) RMT is
      latest-wins both ways, so --force-retry re-drain risks downgrading real
      values — avoided.
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
      `name()` (measure `with_name / 68` on prod after backfill). — deferred:
      code shipped; measurement happens when the prod `nft-collection-name`
      drain runs (blocked behind the enrichment-worker conc=0 hold / 0306).
- [x] `name()` fetched at most once per contract per run (cached), never per
      token — per-CONTRACT `name_cache` in `NftTokenUriFetcher`; test
      `resolve_collection_name_happy_path_hits_rpc_once` proves the 2nd resolve
      is a cache hit (`expect(1)` on the mock).
- [x] Contracts without `name()` write the sentinel; no retry storm — a
      permanent `name()` fail folds to `Ok(None)` and is CACHED (test
      `resolve_collection_name_permanent_fail_is_cached_none`); the live path
      keeps the `''` sentinel, the backfill writes nothing.
- [x] Served `collection_name` (list / detail / search) reflects the fetched
      value — unchanged read path (`nfts/queries_ch.rs`, `search/queries_ch.rs`
      already join `nft_enrichment`); this only changes the write source.
- [x] Stale "from token_uri JSON" assumption corrected in the `0212` / `0301` /
      `0306` notes.
- [x] **Docs updated** — `docs/architecture/indexing-pipeline/enrichment.md`
      (§1 sources, §3.2 worker source, §3.3 backfill) + the runner README record
      `name()` RPC as the `collection_name` source per
      [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [x] **API types regenerated** — `N/A` — no change under `crates/api/**` DTOs,
      `Cargo.{toml,lock}`, or `libs/api-types/**` (the `collection_name` field
      already exists in the DTO; this only changes its data source).

## Implementation Notes

Landed on branch `feat/0340_nft-collection-name-from-contract-name-rpc` (PR
pending). Blast radius: 4 files in `enrichment-shared`, 1 in
`backfill-enrichment-runner`, 2 arch/README docs.

- **Fetcher** (`nft_token_uri/client.rs`): new `resolve_collection_name(contract_id)`
  - a second per-CONTRACT `name_cache`. Generalised `build_simulate_envelope`
    (takes a `function_name`) and `decode_token_uri_result` → `decode_string_result`
    (both `token_uri()` and `name()` return `ScVal::String`). New
    `simulate_name_with_failover` (zero-arg, no arity fallback). Failure model is
    deliberately different from `resolve()`: a PERMANENT "no usable name" folds to
    `Ok(None)` and IS cached (a stable contract fact — one RPC per collection, not
    per token); only transient RPC faults surface as `Err` (uncached → retried).
- **Live path** (`enrich_and_persist/nft_token_uri.rs`): after the `token_uri`
  match, if `collection_name` is still empty, call `resolve_collection_name`.
  A non-empty JSON `"collection"` still wins (none exists in practice). Runs
  even after a permanent token_uri fail — a 0308 custom-ABI contract can rename
  `token_uri` but keep `name()`.
- **Backfill** (`enrich_and_persist/nft_collection_name.rs` +
  `persist::rewrite_nft_collection_name` + runner `nft-collection-name`
  subcommand): walks DISTINCT contracts with empty `collection_name`, one
  `name()` per contract, then one INSERT-SELECT that re-writes each row with the
  name stamped on and `name`/`media_url` PRESERVED (RMT whole-row replace).
  Idempotent (re-applies the `collection_name=''` predicate). This is the path
  the runbook AC-1 measurement will use.

## Design Decisions

### From Plan

1. **`name()` via RPC simulate, cached per-contract** — as specified.

### Emerged

2. **Separate `nft-collection-name` subcommand, not `--force-retry`** — the
   existing runner modes can't target the cohort: `Untried` skips rows that
   have a side-table row, and `Sentinels` requires ALL three columns empty
   (these rows have real `name`/`media_url`). `--force-retry` (mode `All`) would
   re-drain 12.8k tokens through full `token_uri()` + HTTP AND — because the
   `nft_enrichment` RMT is latest-wins in BOTH directions — a flaky upstream on
   the re-drain could DOWNGRADE a real `name`/`media_url` to a sentinel. The
   per-contract path avoids both (one `name()` RPC per contract; the
   INSERT-SELECT reads and re-writes the existing real values, never re-fetches
   them).
3. **`collection_name` filled AFTER the token_uri match, unconditionally on
   empty** — so a permanent `token_uri()` fail still yields a collection name.
4. **Char-cap `name()` at 4096** — mirrors the worker's existing `token_uri`
   JSON caps; an oversize/whitespace value → treated as "no name".

## Notes

- Volume is per-contract (68), not per-token (12.8k). A read-time semi-join over
  `soroban_contract_metadata` was considered but ruled out: the name is not in
  that table at all (0%), so there is nothing to join to until it is fetched.
- Custom-ABI collections (0308) that rename or omit `name()` stay NULL — out of
  scope here.
