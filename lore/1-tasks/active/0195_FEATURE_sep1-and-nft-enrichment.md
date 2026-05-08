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
  - date: '2026-05-08'
    status: active
    who: karolkow
    note: |
      §2d spec locked. Drop `nfts.metadata` JSONB column → runtime type-2 (`runtime_enrichment::nft_token_uri`) per ADR 0043 detail-only carve-out + FE spec §6.11/§6.12. Lambda 2 writes 3 list columns (`name`, `media_url`, `collection_name`); zero CDK delta — inherits 0191's queue/DLQ/alarm/concurrency. Source-named modules everywhere (`nft_token_uri`), matching the `sep1` / `stellar_archive` convention; the wire response field stays `metadata` (API contract, mapped at handler boundary). Worker branches on Content-Type to handle both JSON-metadata and direct-image `token_uri` conventions. Defensive guards ported from `sep1::client`: `validate_uri` (https://+ipfs:// only, IP-literal / userinfo / RFC1035 reject), `Policy::limited(0)` redirect kill, 256 KB body cap, plus worker-side `is_safe_media_url` for `<img src>` XSS defence.
---

# Lambda 2 enrichment: SEP-1 assets (icon + name) + NFT metadata

## Summary

Two sub-blocks on 0191's SQS-driven type-1 enrichment worker, each populating columns that **cannot** be derived from the processed ledger:

- **§2a — DONE** (commit 5803f1c): extended existing `icon` kind to also persist `assets.name` from the same SEP-1 fetch (ClassicCredit + SAC).
- **§2d**: `nft_token_uri` kind — per-token Soroban RPC `token_uri()` + IPFS gateway for `nfts.{name, media_url, collection_name}`. Drops `nfts.metadata` JSONB column; the detail-page blob is served via runtime type-2 (mirrors `assets.description`).

(§2b moved to 0199; §2c asset_usd_price pulled — see history.)

Reuse `enrichment-shared` lib, worker dispatch, indexer producer, permanent/transient `EnrichError` taxonomy from 0191.

## Status: Active

§2a merged. §2d gated on IPFS gateway choice + Soroban RPC sizing decisions (recorded as AC). ADR 0043 (field allocation rule) accepted on develop.

## Context

### Field allocation per ADR 0043

Off-chain = data NOT in processed ledger. Decisions for this task:

- **`assets.name`** (asset_type IN (1, 2)) — SEP-1 TOML `CURRENCIES[].name`. ClassicCredit: no on-chain source (XDR `Asset` enum = code+issuer). SAC: on-chain `name()` returns `<code>:<issuer_strkey>` machine ID per CAP-46-6, not human name; project leaves `name=None` ([state.rs:820-827](crates/xdr-parser/src/state.rs:820)). Soroban-native (asset_type=3) is owned by indexer/0156. Native (asset_type=0) out of scope.
- **`nfts.{collection_name, name, media_url}`** — per-token `token_uri()` RPC + JSON fetch. Off-chain. List-endpoint fields → Lambda 2 columns. `nfts.metadata` (attributes/traits/description) is detail-only → runtime type-2 in API handler, JSONB column dropped.

LP `{tvl, volume, fee_revenue}` are owned by **task 0199** (consolidates 0125 + former 0195 §2b).

### Reuse from 0191

`enrichment-shared` lib (`sep1/`, `enrich_and_persist/`, EnrichError) + `enrichment-worker` Lambda (SqsEvent dispatch, `EnrichmentMessage` tagged enum) + `enrichment_publish.rs` Publisher + CDK queue/dlq/alarms + sentinel `''` pattern.

### Failure semantics per kind

- **§2a icon (query-and-batch, inherited from 0191):** fail-soft. Producer re-SELECTs rows missing the column on each ledger touch; sentinel `''` REQUIRED to repel re-emission. UPDATE `COALESCE(NULLIF($n, ''), col, $n)` priority `real > sentinel > NULL` — sentinels upgradable, real values stick, NULLs fill.
- **§2d nft_token_uri (insert-hook): hard-fail (NFT-only divergence).** Worker propagates ALL fetch / parse / validation failures as `EnrichError::Transient` → SQS retry → DLQ → DepthAlarm. **No sentinel write.** A row that fails to enrich stays NULL until manual DLQ replay or 0196 backfill. Plain `UPDATE SET ... = $n` (no COALESCE) — sentinels are never written so the upgrade pattern is unnecessary. Api detail handler `.expect()`s the fetcher result — fetcher error → 502. Rationale: NFT enrichment ships in beta; the team prefers visible failures over silent staleness. SEP-1 stays fail-soft (already in production).

## Implementation Plan

### §2a — Icon kind extension: also persist `assets.name` — DONE

Shipped in commit 5803f1c. Summary of what landed:

- `Sep1Currency.name: Option<String>` ([dto.rs](crates/enrichment-shared/src/sep1/dto.rs)).
- `enrich_asset_from_sep1` (rename of `enrich_asset_icon`) writes both columns in one UPDATE with `COALESCE(NULLIF($n, ''), col, $n)` priority `real > sentinel > NULL`.
- Producer SQL: `WHERE icon_url IS NULL OR (asset_type IN (1, 2) AND name IS NULL)`.
- Module rename `icon.rs` → `sep1_assets.rs` reflects that one fetch now writes both columns.
- 20 unit tests cover sentinel, upgrade, trim, length-limit, asset_type gating.

### §2d — NFT `token_uri()` enrichment

Lambda 2 writes 3 list columns `nfts.{name, media_url, collection_name}` — none derivable from the processed ledger; the standard NFT pattern (OpenZeppelin / OpenSea, copied by Soroban) leaves them in an off-chain JSON file referenced by the contract's per-token `token_uri()` view function. Mechanism is per-token (Soroban RPC + IPFS / HTTPS), unlike per-issuer SEP-1 TOML — see [`enrichment_shared::nft_token_uri` module doc](../../../crates/enrichment-shared/src/nft_token_uri/mod.rs) for the side-by-side.

`nfts.metadata` JSONB column **dropped** (detail-only per ADR 0043; mirrors the `assets.description` precedent in migration `20260424000000_drop_assets_sep1_detail_cols.up.sql`). Detail endpoint serves the blob via runtime type-2 (`runtime_enrichment::nft_token_uri`, LRU 24h, fail-soft). All modules are source-named `nft_token_uri` (matching `sep1` / `stellar_archive`); the wire response field stays `metadata`.

**Worker pipeline** (msg `EnrichmentMessage::NftTokenUri { nft_id }`, kind `nft_token_uri`):

- SELECT `(contract_id, token_id)` → Soroban RPC `token_uri(token_id)` → `validate_uri(uri)?` → fetch URI → branch on Content-Type:
  - `application/json` → parse → `name`, `image` → `media_url` (with `is_safe_media_url` hard-fail check), `collection` → `collection_name`.
  - `image/*` → URL → `media_url`; `name` / `collection_name` left empty (legitimate "field absent in source", NOT a sentinel).
- Plain `UPDATE nfts SET name = $1, media_url = $2, collection_name = $3 WHERE id = $4`. Insert-hook → exactly-once per nft_id. Any failure path (fetch, parse, validate) → `EnrichError::Transient` → SQS retry → DLQ.

**CDK:** zero delta — inherits 0191's queue / DLQ / DepthAlarm / concurrency=5 / visibility=30s / max-receives=3.

**Defensive guards** (ported from `sep1::client`): `validate_uri` (https / ipfs only, IP-literal / userinfo / RFC1035 reject), `Policy::limited(0)` redirect kill, `MAX_BODY_BYTES = 256 KB`, worker-side `is_safe_media_url` for `<img src>` XSS defence. **Hard-fail policy: every guard violation → `Err(NftTokenUriError)` → DLQ.** No silent fallback to sentinel.

**Code touchpoints:** drop-column migration; new `enrichment_shared::nft_token_uri` (fetcher) + `enrich_and_persist::nft_token_uri` (worker) + `runtime_enrichment::nft_token_uri` (api shim); `EnrichmentMessage::NftTokenUri` variant + worker dispatch; `publish_for_minted_nfts` producer hook in `enrichment_publish.rs`; remove `metadata` from `NftItem` DTO + add `NftDetailResponse`; remove `metadata` from list/detail queries + `ExtractedNft` + `detect_nfts` + indexer / db-merge INSERTs; regen api-types.

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

- [ ] Migration drops `nfts.metadata` JSONB column; up + down land together.
- [ ] Indexer + db-merge no longer reference `metadata` on INSERT / ON CONFLICT.
- [ ] `ExtractedNft.metadata` removed; parser tests still pass.
- [ ] Insert-hook emits exactly one `NftTokenUri` per mint; no re-emit on transfer/burn.
- [ ] Worker handles both `application/json` and `image/*` `token_uri` responses; image-only path leaves `name` / `collection_name` empty (legitimate field absence, NOT sentinel).
- [ ] Hard-fail propagation verified: every `NftTokenUriError` variant routes through `EnrichError::Transient` to the DLQ. No sentinel writes. API detail handler `.expect()`s — stub fetcher returns `Err(NotImplemented)` → 502.
- [ ] Sample query: non-NULL `name` / `media_url` / `collection_name` on minted NFTs from a known JSON-metadata collection.
- [ ] `runtime_enrichment::nft_token_uri::NftTokenUriFetcher::resolve` returns full JSON on detail endpoint with 24h LRU; fail-soft to `null`. Wire response field stays `metadata`.
- [ ] List response no longer carries `metadata` field; detail response preserves it via runtime fetch.

**Common:**

- [ ] Per-kind permanent/transient `EnrichError` mapping documented + unit-tested.
- [ ] §2d integration test (mock RPC + mock IPFS gateway, both Content-Type branches).
- [ ] No CDK delta — new kind dispatched by existing 0191 Lambda 2 / SQS / DLQ / DepthAlarm. Verify queue grants + log group cover the new kind without changes.
- [ ] Docs updated (ADR 0043 matrix amendment marking `nfts.metadata` as runtime type-2; enrichment.md; schema docs reflecting column drop).
- [ ] API types regenerated per `API types freshness` CI gate.

## Future Work (out of scope)

- **Per-collection NFT batching** if per-token RPC cost prohibitive.
- **Per-kind CloudWatch metrics** (success/fail counters, fetch latency histograms).
- **`asset_usd_price`** — re-evaluate when product ask materialises.
- **Sentinel-in-VARCHAR retire**: replace `''` overload + `NULLIF` SQL with explicit `{icon_url, name}_attempted_at TIMESTAMPTZ` companion columns. Senior-correct status separation. Bundle with 0196's status-column work to retrofit both 0191's `icon_url` and 0195's `name` simultaneously.
