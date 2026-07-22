---
id: '0297'
title: 'FEATURE: on-chain Soroban token metadata (name/symbol/decimals) → soroban_contract_metadata side table'
type: FEATURE
status: completed
related_adr: ['0050']
related_tasks: ['0283', '0231', '0304']
tags: [clickhouse, enrichment, soroban, layer-data, priority-low, effort-medium]
links: []
history:
  - date: 2026-06-16
    status: backlog
    who: karolkow
    note: >
      Spawned from 0283 future work (G5 name-clobber structural close). Bundles
      the contract-name enrichment job with a minor ScVal::Bytes name-decode
      bug found in the same audit. The contract-name piece overlaps 0231 and may
      fold there.
  - date: 2026-06-17
    status: active
    who: karolkow
    note: >
      Promoted to active for a fresh-eye deep-dive. The "names are off-ledger"
      framing is contested: Staszek's 0283 hand-off asserts name/symbol/decimals
      ARE on-ledger in contract instance storage under Symbol("METADATA"), and
      that scval.rs drops inst.storage. Scope now also covers the decimals
      question. Body claims to be verified against mainnet before any rewrite.
  - date: 2026-06-17
    status: active
    who: karolkow
    note: >
      CHAIN-VERIFIED (mainnet getLedgerEntries, 37 live contracts): name/symbol/
      decimals ARE on-ledger in instance storage under Symbol("METADATA") (10/37,
      incl. WASM tokens liquidFi + Comet); 0/37 carry a standalone Symbol("name").
      scval.rs:84 drops inst.storage. The "off-ledger" framing is REFUTED
      (over-generalised from Bachini, the one sample with empty storage + no working
      name() getter). Decimals fold into the same parser fix. Full evidence +
      reconciliation in notes/S-onchain-metadata-location-chain-verified.md.
  - date: 2026-06-19
    status: completed
    who: karolkow
    note: >
      Code implementation done (option D). Parser → side table →
      API read-compose. Tests: xdr-parser 257 lib (+ token_metadata/metadata-
      writes, incl. restored), db-clickhouse + api green; clippy + fmt clean;
      api-types regenerated. Then a 5-agent review (code-review + 2× simplify +
      red/blue + requirements audit) ran on the full diff; fixes A,B,C,D,E,F,H
      applied (see Design Decisions → Emerged). All deploy/backfill/perf/
      frontend/legacy-cleanup deferred to spawned task 0304.
---

# FEATURE: contract-name enrichment + bytes-decode fix

> ⚠️ **CHAIN-VERIFIED CORRECTION (2026-06-17, karolkow).** The "Summary" and
> "Implementation Plan" below are the ORIGINAL framing and are **chain-refuted**.
> Live mainnet `getLedgerEntries` proves `name` / `symbol` / `decimals` are
> **on-ledger** in the contract's instance storage under `Symbol("METADATA")`
> (a `{decimal, name, symbol}` struct), which `scval.rs:84` silently drops. The
> real fix is a **parser fix** (recover the `METADATA` struct), not RPC `name()`
> enrichment; the off-chain path is only a fallback for NFTs like Bachini that
> have no on-ledger metadata. **Decimals are in scope** (same struct). See
> [`notes/S-onchain-metadata-location-chain-verified.md`](notes/S-onchain-metadata-location-chain-verified.md)
> before acting on anything below.

## Summary

Soroban token `name` / `symbol` / `decimals` are **on-ledger** — packed into one
struct in the contract's instance storage under `Symbol("METADATA")`
(`{decimal, name, symbol}`). The parser was dropping it (`scval.rs` serialised
only the instance `executable`, never `inst.storage`), and the old path matched a
standalone `Symbol("name")` entry that real tokens never write — hence
`soroban_contracts.name` was an empirical zero. Chain-verified on mainnet (see
note). Fix = the **parser recovers the METADATA struct** → a dedicated
**`soroban_contract_metadata`** side table → API read-composes.
Off-chain RPC is only a fallback for NFTs (e.g. Bachini) that carry no on-ledger
metadata — already served by the `nft_enrichment` / `token_uri` path.

## Context

Spawned from **0283**. The G5 guardrail (shipped in 0283) disabled the dead
on-ledger name-write loop and added a tripwire; the real fix for populating
`name` is enrichment, in the ADR 0053 side-table family. **Overlaps 0231**
(SEP-1/NFT enrichment side-tables, same AWS SQS+Lambda path) — consider folding
the name-enrichment piece into 0231 rather than a standalone runner.

## Implementation

**Scope of THIS task = code only.** Deploy / backfill / live-validation / perf
go to a separate spawned task.

1. **Schema** — `soroban_contract_metadata(contract_id, name, symbol, decimals,
version)`, `RMT(version)`, `ORDER BY (contract_id)`; `version` = observed
   ledger. A SEPARATE table (not columns on `soroban_contracts`), Option D:
   RMT whole-row replace + that table's many writers would clobber in-row
   metadata, and identity vs metadata update on different clocks.
2. **Parser** — `token_metadata::extract_token_metadata` (METADATA struct →
   typed), exposed on every instance change in `ledger_entry_changes`;
   `state::extract_contract_metadata_writes` collects `created` + `updated`,
   **skips SACs** (name/symbol/decimals derive from SAC identity).
3. **Indexer write** — `ParseOutput.contract_metadata_writes` →
   `persist`/backfill `sink` → `StagedLedger.metadata_rows`
   (`build_metadata_rows`) → writer → `soroban_contract_metadata`.
4. **API read** — contract detail + asset detail/list `LEFT JOIN` the metadata
   (argMax(version) subquery); surface `name`/`symbol`/`decimals`; `decimals`
   defaults to 7 for classic/SAC; `name` COALESCEs SEP-1 → METADATA → fallback.
   `libs/api-types` regenerated.
5. **`ScVal::Bytes` decode** (original Step 2) — re-examine, likely moot: the
   METADATA `name` is a `String`, and `decode_scval_string` consumes a different
   JSON shape than `scval.rs:45` produces. Verify before "fixing".

## Acceptance Criteria (revised — option D)

- [x] Parser extracts `name`/`symbol`/`decimals` from instance-storage `METADATA` on `created` AND `updated` instance changes — `crates/xdr-parser/src/token_metadata.rs` + `ledger_entry_changes.rs` (6 unit tests).
- [x] New `soroban_contract_metadata` side table (`RMT(version)`, key `contract_id`) in `init.sql`.
- [x] Indexer writes the table on `created` + `updated`, **SACs skipped**; `version` = observed ledger — `db-clickhouse` rows/stage/writer + `persist.rs` + backfill `sink.rs` + `indexer process.rs`.
- [x] API read-composes via `LEFT JOIN` (argMax subquery) on `contract_id`; `decimals` defaults to 7 for classic/SAC — contract detail + asset detail/list (`name`/`symbol`/`decimals`).
- [x] `libs/api-types` regenerated for new symbol/decimals response fields.
- [x] `ScVal::Bytes` decode claim re-examined — moot: METADATA `name` is `String`; the new `token_metadata::scval_text` already decodes `String`/`Symbol`/UTF-8 `Bytes` directly off the raw `ScVal` (no JSON-hex round-trip). No separate fix needed.
- [x] Docs (ADR 0032) — `database-schema-overview` + `xdr-parsing-overview` updated. (`backend-overview` + reference-SQL snapshots deferred to 0304.)
- [ ] JOIN/`FINAL` cost validated on the CH snapshot before the read-flag flip — **deferred to [0304](../../backlog/0304_FEATURE_0297-metadata-followups-ops-and-cleanup.md)**.
- [ ] Backfill plan for existing contracts (re-parse vs RPC dump) — **deferred to 0304**.

## Implementation Notes

- New module `crates/xdr-parser/src/token_metadata.rs`: `extract_token_metadata` (METADATA struct → typed `TokenMetadata`), `is_stellar_asset_instance` (typed SAC test), `scval_text` (String/Symbol/Bytes), `scval_u32` (U32/U64).
- Write path: `ExtractedLedgerEntryChange.{token_metadata,is_sac}` → `extract_contract_metadata_writes` → `build_metadata_rows` (assigned at both CH write sites — indexer `persist.rs` + backfill `sink.rs` — NOT threaded through `prepare`'s ~30 callers, to keep blast radius low).
- Read path: `argMax(_, version) GROUP BY contract_id` LEFT JOIN; detail uses a **point-lookup** (`WHERE contract_id = ?` inside the subquery), list keeps the full-table sub-aggregate.
- Removed the obsolete CH G5 tripwire + dead commented loop in `stage.rs`. Legacy `contract_name_writes` plumbing left inert (full un-thread deferred to 0304).

## Issues Encountered

- **argMax vs FINAL doc drift** — `init.sql` comment promised a `FINAL` read; code uses per-column `argMax`. Safe here only because every row is a whole-struct snapshot at one ledger — now documented as an INVARIANT in the schema comment (review fix B).
- **Env auto-staging** — the worktree re-stages the most-recently-Edited file (no git hook does it); the `init.sql`/`assets` doc-comment fixes kept folding into the index. Worked around by setting the index entry via `git update-index` to the 0297-only blob so review-fixes stayed unstaged until the final commit.

## Design Decisions

### From Plan

1. **Separate `soroban_contract_metadata` side table** (not columns on `soroban_contracts`) — RMT whole-row clobber + two independent update clocks (identity vs metadata). chain-proven in `notes/G-two-writers-proof.md`.
2. **SACs skipped** — name/symbol/decimals derivable from SAC identity; a row would bloat + duplicate.

### Emerged (multi-agent review fixes, 2026-06-19)

3. **A — contract-detail point lookup**: pushed `WHERE contract_id = ?` into the metadata sub-aggregate so a detail hit never aggregates the whole table (quota-sensitive). List stays full-table.
4. **D — accept `restored`**: producer now writes on `created`/`updated`/`restored` (was created/updated only). Closes the cold-start-after-eviction hole; `restored` already carried `token_metadata`. New test added.
5. **E — typed `is_sac`**: SAC skip now reads a typed `is_sac` bool computed off the XDR at parse time, not a deep dig into optional serialized `data` JSON (robust if `data` is ever trimmed).
6. **F — `nullIf` on contract detail name/symbol**: empty string → NULL, matching the assets path.
7. **B — schema-comment alignment**: documented the actual `argMax` read + whole-snapshot INVARIANT.
8. **C — documented `sc.contract_id` coupling**: the asset metadata join resolves via `soroban_contracts` (assets carry only the surrogate id); a missing `sc` row → NULL metadata. Structural, not fixable without a strkey column.
9. **H — guard order**: `extract_contract_metadata_writes` runs cheap guards before cloning metadata.
10. **G skipped** — fix A diverged the contract-detail (filtered) and asset-list (full-table) subqueries, so the proposed shared-const dedup no longer applies.

## Future Work

All non-code work is in **[0304](../../backlog/0304_FEATURE_0297-metadata-followups-ops-and-cleanup.md)**: backfill, perf/JOIN validation + read-flag flip, live tests, frontend amount rendering via `decimals`, legacy `contract_name_writes` un-threading, vestigial column DROP, remaining docs.
