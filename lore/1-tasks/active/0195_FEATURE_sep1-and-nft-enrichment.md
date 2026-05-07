---
id: '0195'
title: 'Lambda 2 enrichment: SEP-1 assets (icon + name) + NFT metadata'
type: FEATURE
status: active
related_adr: ['0007', '0022', '0023', '0032', '0043']
related_tasks: ['0188', '0191', '0194', '0196', '0197', '0199']
tags:
  [priority-medium, effort-medium, layer-enrichment, layer-lambda, audit-gap]
milestone: 2
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-05-06'
    status: backlog
    who: karolkow
    note: 'Spawned from M2 enrichment planning. Original scope: 4 sub-blocks (icon-name extension, lp_tvl, asset_usd_price, nft_metadata).'
  - date: '2026-05-07'
    status: active
    who: karolkow
    note: |
      Activated. Spec consolidated (3 amendment passes folded in):
      §2c (asset_usd_price) pulled — no consumer.
      §2a scope corrected to ClassicCredit + SAC after parser audit (state.rs:824 explicit `name=None` on SAC; CAP-46-6 confirms SAC's on-chain `name()` is `code:issuer` machine ID, not human name).
      §2d producer pivoted to insert-hook (no live-path dedup; backfill → 0196).
  - date: '2026-05-07'
    status: active
    who: karolkow
    note: |
      §2b (LP TVL) moved to 0199 (LP analytics: TVL + volume + fee_revenue). 0199 absorbs §2b because tvl/volume/fee_revenue share the same `liquidity_pool_snapshots` table, the same insert-hook timing, and the same external price-API dependency — sibling tasks would force two PRs touching the same write path with the same external dep. 0125 archived as `superseded by: [0199]` (no longer references 0195). Title rename: "off-chain NULL fields" → "SEP-1 assets + NFT metadata" reflects narrowed scope.
      0195 §2a shipped (commit 5803f1c).
---

# Lambda 2 enrichment: SEP-1 assets (icon + name) + NFT metadata

## Summary

Two sub-blocks on 0191's SQS-driven type-1 enrichment worker, each populating columns that **cannot** be derived from the processed ledger:

- **§2a — DONE** (commit 5803f1c): extended existing `icon` kind to also persist `assets.name` from the same SEP-1 fetch (ClassicCredit + SAC).
- **§2d**: `nft_metadata` kind — Soroban RPC `token_uri()` + IPFS gateway for `nfts.{name, media_url, metadata}`.

(§2b moved to 0199; §2c asset_usd_price pulled — see history.)

Reuse `enrichment-shared` lib, worker dispatch, indexer producer, permanent/transient `EnrichError` taxonomy from 0191.

## Status: Active

§2a merged. §2d gated on IPFS gateway choice + Soroban RPC sizing decisions (recorded as AC). ADR 0043 (field allocation rule) accepted on develop.

## Context

### Field allocation per ADR 0043

Off-chain = data NOT in processed ledger. Decisions for this task:

- **`assets.name`** (asset_type IN (1, 2)) — SEP-1 TOML `CURRENCIES[].name`. ClassicCredit: no on-chain source (XDR `Asset` enum = code+issuer). SAC: on-chain `name()` returns `<code>:<issuer_strkey>` machine ID per CAP-46-6, not human name; project leaves `name=None` ([state.rs:820-827](crates/xdr-parser/src/state.rs:820)). Soroban-native (asset_type=3) is owned by indexer/0156. Native (asset_type=0) out of scope.
- **`nfts.{collection_name, name, media_url, metadata}`** — per-token `token_uri()` RPC + JSON fetch. Off-chain.

LP `{tvl, volume, fee_revenue}` are owned by **task 0199** (consolidates 0125 + former 0195 §2b).

### Reuse from 0191

`enrichment-shared` lib (`sep1/`, `enrich_and_persist/`, EnrichError) + `enrichment-worker` Lambda (SqsEvent dispatch, `EnrichmentMessage` tagged enum) + `enrichment_publish.rs` Publisher + CDK queue/dlq/alarms + sentinel `''` pattern.

### Sentinel strategy per producer model

- **Query-and-batch** (§2a, inherited from 0191): producer re-SELECTs rows missing column on each ledger touch. Sentinel `''` REQUIRED — without it the same row is re-emitted forever. Update SQL `COALESCE(NULLIF($n, ''), col, $n)`: `real > sentinel > NULL` priority — sentinels are upgradable, real values stick, NULLs fill.
- **Insert-hook** (§2d): producer emits exactly once on row INSERT. No dedup needed live. Sentinels (`nfts.name=''`, `metadata='{}'`) exist for downstream UI fallback only. Backfill of pre-existing rows → 0196.

## Implementation Plan

### §2a — Icon kind extension: also persist `assets.name` — DONE

Shipped in commit 5803f1c. Summary of what landed:

- `Sep1Currency.name: Option<String>` ([dto.rs](crates/enrichment-shared/src/sep1/dto.rs)).
- `enrich_asset_from_sep1` (rename of `enrich_asset_icon`) writes both columns in one UPDATE with `COALESCE(NULLIF($n, ''), col, $n)` priority `real > sentinel > NULL`.
- Producer SQL: `WHERE icon_url IS NULL OR (asset_type IN (1, 2) AND name IS NULL)`.
- Module rename `icon.rs` → `sep1_assets.rs` reflects that one fetch now writes both columns.
- 20 unit tests cover sentinel, upgrade, trim, length-limit, asset_type gating.

### §2d — `nft_metadata` `EnrichmentMessage` variant

- `EnrichmentMessage::NftMetadata { nft_id: i32 }`.
- New module `enrich_and_persist/nft_metadata.rs`. Pipeline: SELECT `(contract_id, token_id)` → Soroban RPC `token_uri()` → resolve URI (HTTP / IPFS gateway) → parse JSON → UPDATE `nfts`.
- Producer hook = INSERT on `nfts` (mint event only — not transfer/burn).
- Permanent fail: `name=''`, `media_url=''`, `metadata='{}'`, `collection_name` left NULL. Sentinels for UI fallback, NOT for dedup.
- Transient → `EnrichError::Transient`, SQS retry → DLQ.
- **DECISION GATING (must resolve before merge):**
  - **IPFS gateway**: single, fallback chain, or round-robin. Default proposal: Cloudflare primary + Pinata fallback.
  - **Soroban RPC sizing**: estimate sustained QPS from realistic mint volume × per-token cost; size DepthAlarm threshold + worker concurrency cap.

### Common — ADR + docs

- ADR 0043 amendment: per-kind matrix table updated for SEP-1 (icon + name) and NFT metadata kinds. (ADR 0029 NOT amended — read-path, not write-path.)
- `docs/architecture/indexing-pipeline/enrichment.md` (create if absent).
- `docs/architecture/database-schema/**` — column source attribution.

## Acceptance Criteria

**§2a (done):**

- [x] `Sep1Currency.name` added; combined UPDATE with `COALESCE(NULLIF(...), col, ...)` priority `real > sentinel > NULL`.
- [x] Sample query: non-NULL `name` on ClassicCredit + SAC assets with SEP-1 TOML support.
- [x] Test: producer dedup does not infinite-re-emit a sentinel-marked row.
- [ ] List/detail endpoints render `asset_code` when `name = ''` (UI work — separate frontend task).

**§2d:**

- [ ] Insert-hook emits exactly one `NftMetadata` per mint; no re-emit on transfer/burn.
- [ ] IPFS gateway choice pinned.
- [ ] Soroban RPC QPS estimate → DepthAlarm + concurrency cap derived.
- [ ] Sample query: non-NULL `name`/`media_url` on minted NFTs from a known collection.

**Common:**

- [ ] Per-kind permanent/transient `EnrichError` mapping documented + unit-tested.
- [ ] §2d integration test (mock RPC / mock IPFS gateway).
- [ ] CDK DepthAlarm thresholds reviewed for new producer rates.
- [ ] Docs updated (ADR 0043 amendment + enrichment.md + schema docs).
- [ ] API types regenerated if any DTO field exposed (e.g. `nfts.metadata` JSON shape).

## Future Work (out of scope)

- **Per-collection NFT batching** if per-token RPC cost prohibitive.
- **Per-kind CloudWatch metrics** (success/fail counters, fetch latency histograms).
- **`asset_usd_price`** — re-evaluate when product ask materialises.
- **Sentinel-in-VARCHAR retire**: replace `''` overload + `NULLIF` SQL with explicit `{icon_url, name}_attempted_at TIMESTAMPTZ` companion columns. Senior-correct status separation. Bundle with 0196's status-column work to retrofit both 0191's `icon_url` and 0195's `name` simultaneously.
