---
id: '0308'
title: 'Custom-ABI NFT family: parser shapes + classifier coverage (double-missed launchpad NFTs)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0306']
tags:
  [
    'nft',
    'xdr-parser',
    'classifier',
    'backfill',
    'phase-future',
    'effort-medium',
    'priority-medium',
  ]
links:
  - 'https://api.stellar.expert/explorer/public/contract/CBMKSLJL6UFPKIE76ASSEKSP4H7ZWL3PCX6NGMWBARR5RH2GHB7U5QMJ'
history:
  - date: 2026-06-19
    status: backlog
    who: karolkow
    note: 'Spawned from nft-reparse tripwire deep-dive + /devils-advocate chain verification. All claims chain-proven (RPC + stellar.expert archive + SEP-50 docs).'
---

# Custom-ABI NFT family: parser shapes + classifier coverage

## Summary

A family of **real, on-chain NFT collections** (launchpad codebase, e.g. "8888 SKELETONS",
"Doughy Donuts", "Skeletrons") is **double-missed** by our pipeline: the event parser drops
its event shapes, and the WASM classifier labels it `Other` so even recovered rows never reach
the hot `nfts` table. Result: these NFTs are invisible (hot `nfts` = 0) or, worse, partially
captured with **wrong ownership** (the standard-shaped subset parses, the rest is dropped).
This task makes the parser + classifier recognise this custom (non-SEP-50) ABI end-to-end.

## Context

Found while deep-diving an `xdr_parser::nft` tripwire WARN during an nft-reparse run
(`NFT event symbol matched but no known arg shape parsed`). Every claim below was verified
against **chain ground truth** (Soroban RPC + stellar.expert event archive + on-chain WASM
interfaces) and official SEP-50/OpenZeppelin docs — NOT our repo/logs.

**The family (chain-verified):**

- **11 contracts** (our `soroban_events` discovery — see caveat), **4 wasm versions**
  (`f84321e8`×4, `086b776c`×4, `297bfc31`×1, `f29c8762`×2; contracts UPGRADE — emit
  `executable_update`/`upgraded`, `versions:2`). All 11 fetched from chain export NFT functions
  `get_token_info`/`bulk_mint`/`freeze_collection`/`update_token_url`/`get_tokens`; **zero** fungible
  markers (`decimals`/`allowance`/`total_supply`). Token identity is `token_id: u32` everywhere.
- All 11 classified `contract_type = Other (1)` in `soroban_contracts`. hot `nfts` = 0;
  `nfts_pending` = 140 rows for 3 of them (partial capture from the standard-shaped subset).

**Why double-missed:**

1. **Parser** — the events use a non-SEP-50 inverted layout. SEP-50/OZ standard transfer is
   `topics=[transfer, from, to]`, `data=token_id:u32` (addresses in topics, id in data). Our parser
   correctly assumes the standard. This family does the reverse + lossy:
   - `transfer`: `topics=[Symbol("transfer"), u32 token_id]`, `data=Address(to)` — only ONE address =
     the recipient; `from` not emitted. **PROVEN data=`to`** (8/8 distinct recipients: stellar.expert
     event archive `to` == RPC `get_token_info` owner, zero our-DB).
   - `bulk_mint`: `topics=[Symbol("bulk_mint"), Address(to)]`, `data=vec[u32 token_id…]`.
     `extract_args` (Shapes A/B/C) matches none → dropped + tripwired. Worse: **`bulk_mint` is not a
     recognised symbol** (`nft.rs` match arms = `transfer|mint|burn|consecutive_mint`, then catch-all `_ => continue`)
     → silently skipped with **no tripwire** — and it's the dominant event. The tripwire under-reports.
2. **Classifier** — `classification.rs` is name-based: `Nft` iff the WASM exports a function named
   `owner_of`/`token_uri`/`approve_for_all`/`get_approved`/`is_approved_for_all`. This family uses
   systematic near-renames (`owner_of`→`get_token_info`, `token_uri`→`update_token_url`,
   `approve_for_all`→`approve_all`); `token_uri` exists only as a `TokenInfo` field + `mint` arg, not a
   function. So `Other` is **correct as written** — a coverage gap, not a bug. `Other`→`nfts_pending`,
   and `nft_reclassify` promotes **only** `contract_type=2 (Nft)` → these are dead-ended.

Example contract: `CBMKSLJL6UFPKIE76ASSEKSP4H7ZWL3PCX6NGMWBARR5RH2GHB7U5QMJ`
(i64 `4366918265184966584`, "8888 SKELETONS", `token_uri` → `https://8888skeletons.com/collection_json/<id>.json`).

## Implementation Plan

### Step 1: Parser — new shapes + close the silent gap (`crates/xdr-parser/src/nft.rs`)

- Add inverted `transfer` shape: `topics=[transfer, u32 id]` + `data=Address` → `NftEvent{ token_id,
to:Some, from:None }` (mint path already uses `from:None`).
- Add `bulk_mint` symbol: `topics=[bulk_mint, Address(to)]` + `data=vec[u32…]` → one mint per id.
- Make unrecognised mint-ish symbols **tripwire** instead of silent `_ => continue`, so the next
  custom ABI surfaces instead of vanishing.
- Guard against double-counting vs the existing standard-shaped events these contracts also emit.

### Step 2: Classifier — recognise this ABI (`crates/xdr-parser/src/classification.rs`)

- Extend the `Nft` discriminator set (e.g. `get_token_info` + `bulk_mint` + `freeze_collection`
  combination) OR classify by interface shape (a `transfer` taking `token_id:u32`, presence of
  `bulk_mint`), NOT a wasm hash (4 versions, they upgrade). Be conservative to avoid false-positives.
- Verify the relabel job (`contract_type_rebuild`) flips these `Other`→`Nft` so `nft_reclassify`
  promotes the pending rows to hot.

### Step 3: Backfill — re-derive, don't append

- Existing `nfts_pending` rows for these contracts are partial (transfers missing) → re-derive the
  whole token set per contract via the fixed parser (nft-reparse covers the range). Confirm ownership
  matches chain `get_token_info` for a sample after backfill.

## Acceptance Criteria

- [ ] Parser emits the inverted `transfer` (`to`/`from:None`) + `bulk_mint` shapes; unit tests with
      real on-chain XDR (CBMKS family) as ground-truth fixtures.
- [ ] Unrecognised mint/transfer/burn-ish symbols tripwire (no silent `_ => continue` drop).
- [ ] Classifier returns `Nft` for the 11 family contracts across all 4 wasm versions; no regression
      on existing SAC/SEP-41/SEP-50 fixtures (no false-positives).
- [ ] After backfill, the family appears in hot `nfts`; ownership for a sampled set matches on-chain
      `get_token_info`.
- [ ] **Docs updated** — changes XDR parsing responsibilities + ingestion/classification; update the
      relevant `docs/architecture/**` per [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — `N/A` (no `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`
      change expected; revisit if a handler/DTO is touched).

## Open Risks & Hardening (devils-advocate, 2026-06-19)

Shape assumptions were adversarially probed against chain/CH and **held**: the
`transfer` shape (`[transfer, u32]` + `data:address`, `topic_count=2`) is uniform
across all 4 wasm versions; `bulk_mint` data is always `vec<u32>` (never the
`Vec<(u32,String)>` the function takes). Backfill supersede is sound:
`nfts_pending` is `ReplacingMergeTree(current_owner_ledger)` keyed by
`(contract_id, token_id)`, and a re-derived row sees a superset of events so its
watermark is always ≥ the existing partial row → correct owner wins on merge. The
residual risks below are the parts the plan must nail BEFORE coding:

1. **[blocking] Classifier false-positives.** `get_token_info`/`bulk_mint`/
   `freeze_collection` are not NFT-exclusive names; a single-name match could
   mislabel a fungible/game contract `Nft` and pollute hot `nfts`. Rule must
   require a COMBINATION (≥2 family fns) AND absence of fungible markers
   (`decimals`/`allowance`/`total_supply`); test against the whole contract
   population, not just these 11.
2. **[verify first] Shape-based classify feasibility.** The classifier is
   name-only (`classification.rs:101-119`). "Classify by `transfer` taking
   `token_id:u32`" needs arg-TYPE access — confirm `classify_contract_from_wasm_spec`
   exposes types before choosing shape-based over name-list extension.
3. **[verify first] Reclassify trigger.** `contract_type_rebuild` may only process
   newly-observed wasm uploads; the 11 are already `Other` and won't auto-flip. Add
   an explicit reclassify pass over the 4 affected hashes to the backfill.
4. **bulk_mint guards.** Empty `vec[]` occurs on-chain (handle as no-op, no false
   tripwire) and veclen reaches 20+ (add a MAX element cap mirroring
   `MAX_CONSECUTIVE_RANGE` — DoS guard for the Lambda indexer).
5. **Direction proven on 1 of 4 versions.** `data=to` is proven only on `086b776c`
   (CBMKS). Structure is uniform across versions, so low risk — close it by running
   the `get_token_info` cross-check on one contract each from `f84321e8`/`f29c8762`/
   `297bfc31`.
6. **Observability.** Add an aggregate counter / periodic query of unparsed-symbol
   tripwire hits by contract, so the next custom ABI surfaces without log-eyeballing.
7. **Optional interim unblock.** As a stopgap before the general fix, manually set
   `contract_type=Nft` for the 4 known wasm hashes to surface these collections now
   (accepting it won't cover future uploads/new hashes).

## Notes

- **Disposition vs 0306:** [0306](0306_OPS_nft-surfacing-enrichment-prod-pipeline.md) is the NFT
  surfacing/enrichment pipeline; this task is the upstream parser+classifier coverage that feeds it.
- **Scope caveat (honest provenance):** the _nature_ of each of the 11 is chain-proven; the **count
  "11" is discovery from our `soroban_events` scan** — there may be MORE family contracts (no chain
  index by wasm-hash/ABI to enumerate exhaustively). A census step could widen coverage.
- **Verification trail:** direction `data=to` proven 8/8 chain-on-chain (stellar.expert `bodyXdr` decode
  vs RPC `get_token_info`); all 11 wasms fetched + interface-checked from chain; SEP-50/OZ confirm this
  is non-standard. Full record in auto-memory `project-custom-abi-nft-class-missed`.
