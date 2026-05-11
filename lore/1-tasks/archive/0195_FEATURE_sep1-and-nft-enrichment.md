---
id: '0195'
title: 'Lambda 2 enrichment: SEP-1 assets (icon + name) + NFT metadata'
type: FEATURE
status: completed
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
  - date: '2026-05-11'
    status: active
    who: karolkow
    note: |
      Phase E shipped: real `NftTokenUriFetcher` replaces the `unimplemented!()` stub. Pipeline: `build_simulate_envelope(contract_id, token_u32)` → JSON-RPC `simulateTransaction` POST → `decode_token_uri_result` (ScVal::String / Symbol → UTF-8 URI) → `validate_uri` → `resolve_ipfs_to_https` (Cloudflare gateway) → HTTP GET → Content-Type branch. `token_id` assumed `ScVal::U32` (OpenZeppelin / ERC-721 sequential mint counter); non-u32 hard-fails with `MalformedTokenId` so the operator sees the misconfiguration in the DLQ. New error variants: `UnsafeScheme`, `MalformedUri`, `MalformedTokenId`, `MalformedContractStrKey`, `MalformedRpcResponse`, `Xdr`. Hardcoded `DEFAULT_SOROBAN_RPC_URL = https://mainnet.sorobanrpc.com`; switching to a dedicated provider (Quicknode / Validation Cloud) when rate-limited is a single-constant change. Hardcoded `IPFS_GATEWAY_BASE = https://cloudflare-ipfs.com/ipfs/`. 13 new unit tests (envelope roundtrip, ScVal decode, URI validate, IPFS resolve, host extraction, MalformedTokenId, MalformedContractStrKey) + 5 wiremock-driven JSON-RPC integration tests (happy path, JSON-RPC `error`, contract revert via `result.error`, missing `results` array, 5xx → transient). 34/34 tests pass. Metadata-URL fetch path is covered by unit tests of the discrete decode / validate / branch pieces; an end-to-end wiremock test would require HTTPS-on-loopback that `validate_uri` rejects on purpose. AC #6 reframed: hard-fail is the **stub** behaviour; real fetcher returns `Err(NftTokenUriError)` and the worker / api fold per the soft-fail downstream pattern (sentinel write on `None`, `null` on the wire respectively). Followups: api-types regenerated, `docs/architecture/indexing-pipeline/enrichment.md` created.
  - date: '2026-05-11'
    status: done
    who: karolkow
    note: |
      Task completed. §2a + §2d both shipped end-to-end: SEP-1 icon/name (commit 5803f1c), NFT token_uri write-side worker + read-side runtime fetch (commit d6ef420 + Phase E uncommitted). Live mainnet smoke confirmed real RPC compatibility (URI extracted from contract `CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY`). 34/34 unit + 1 live smoke pass. Trim pass collapsed `MalformedTokenId` + `MalformedContractStrKey` → single `MalformedInput { field, value }` variant (12 → 11 in `NftTokenUriError`); production code trimmed 1211 → 952 lines (-21%). **Deferred:** sample-query verification on backfill region (PR-time check; AC marked `[ ]` with that note); FE UI work for `asset_code` fallback when `name = ''` (separate frontend task). **Emerged decisions:** (1) hard-fail isolated to the stub `unimplemented!()` chokepoint, downstream stays soft-fail (sentinel write on `None`, wire `null` on detail) — matches §2a SEP-1 pattern, avoids spreading hard-fail invasively; (2) single-value `ScVal::U32` assumption for `token_id` per OpenZeppelin convention — alternative ScVal types DLQ with `MalformedInput` for operator to react (real-world hit: JamesBachini tutorial contract uses 0-arg `token_uri(env)`, surfaced cleanly via DLQ in live smoke); (3) `MalformedInput { field, value }` merge replaces two narrow `Malformed{TokenId,ContractStrKey}` variants for less surface, same triage value. Docs: ADR 0043 matrix amended, schema-overview/endpoint-queries updated, new `enrichment.md` type-1 deep-dive (type-2 path stays in `backend-overview.md`).
---

# Lambda 2 enrichment: SEP-1 assets (icon + name) + NFT metadata

## Summary

Two sub-blocks on 0191's SQS-driven type-1 enrichment worker, each populating columns that **cannot** be derived from the processed ledger:

- **§2a — DONE** (commit 5803f1c): extended existing `icon` kind to also persist `assets.name` from the same SEP-1 fetch (ClassicCredit + SAC).
- **§2d**: `nft_token_uri` kind — per-token Soroban RPC `token_uri()` + IPFS gateway for `nfts.{name, media_url, collection_name}`. Drops `nfts.metadata` JSONB column; the detail-page blob is served via runtime type-2 (mirrors `assets.description`).

(§2b moved to 0199; §2c asset_usd_price pulled — see history.)

Reuse `enrichment-shared` lib, worker dispatch, indexer producer, permanent/transient `EnrichError` taxonomy from 0191.

## Status: Completed

§2a merged (commit `5803f1c`). §2d Phase A-D scaffolding merged (commit `d6ef420`). §2d Phase E (real Soroban RPC + IPFS fetcher) implemented and tested — only pending sample-query verification on a backfill region with a known JSON-metadata collection (PR-time operator check). ADR 0043 (field allocation rule) accepted on develop.

## Context

### Field allocation per ADR 0043

Off-chain = data NOT in processed ledger. Decisions for this task:

- **`assets.name`** (asset_type IN (1, 2)) — SEP-1 TOML `CURRENCIES[].name`. ClassicCredit: no on-chain source (XDR `Asset` enum = code+issuer). SAC: on-chain `name()` returns `<code>:<issuer_strkey>` machine ID per CAP-46-6, not human name; project leaves `name=None` ([state.rs:820-827](crates/xdr-parser/src/state.rs:820)). Soroban-native (asset_type=3) is owned by indexer/0156. Native (asset_type=0) out of scope.
- **`nfts.{collection_name, name, media_url}`** — per-token `token_uri()` RPC + JSON fetch. Off-chain. List-endpoint fields → Lambda 2 columns. `nfts.metadata` (attributes/traits/description) is detail-only → runtime type-2 in API handler, JSONB column dropped.

LP `{tvl, volume, fee_revenue}` are owned by **task 0199** (consolidates 0125 + former 0195 §2b).

### Reuse from 0191

`enrichment-shared` lib (`sep1/`, `enrich_and_persist/`, EnrichError) + `enrichment-worker` Lambda (SqsEvent dispatch, `EnrichmentMessage` tagged enum) + `enrichment_publish.rs` Publisher + CDK queue/dlq/alarms + sentinel `''` pattern.

### Failure semantics per kind

Both kinds share the same shape: fetcher returns `Result<Option<Value>, Arc<NftTokenUriError>>`; worker classifies via `is_transient` (5xx / connect / timeout / SorobanRpc → transient; everything else → permanent). Transient → `EnrichError::Transient` → SQS retry → DLQ. Permanent → sentinel write + warn log. UPDATE uses `COALESCE(NULLIF($n, ''), col, $n)` priority `real > sentinel > NULL` for both kinds (matches §2a SEP-1 pipeline).

- **§2a icon (query-and-batch, inherited from 0191):** producer re-SELECTs on each ledger touch; sentinel `''` repels re-emission via the predicate `icon_url IS NULL OR (asset_type IN (1,2) AND name IS NULL)`.
- **§2d nft_token_uri (insert-hook):** producer emits exactly once per nft_id on mint; the sentinel `''` exists as UI-fallback / future-DLQ-replay protection rather than a dedup driver. Api detail handler folds errors fail-soft to `null` via `.ok().flatten()` plus a 3 s wall-clock timeout so a slow IPFS gateway can't approach the API Gateway 29 s ceiling.

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
  - `application/json` → parse → `name`, `image` → `media_url` (with `is_safe_media_url` re-check; unsafe scheme replaced with sentinel `''`), `collection` → `collection_name`.
  - `image/*` → fetcher synthesises `{"image": "<url>"}`; worker writes `media_url` only, leaves `name` / `collection_name` legitimately absent.
- `UPDATE nfts SET col = COALESCE(NULLIF($n, ''), col, $n)` per column → priority `real > sentinel > NULL`. Insert-hook → exactly-once per nft_id. `is_transient`-driven SQS retry on 5xx / connect / timeout; permanent failures write sentinels + warn log so the operator can grep.

**CDK:** zero delta — inherits 0191's queue / DLQ / DepthAlarm / concurrency=5 / visibility=30s / max-receives=3.

**Defensive guards** (ported from `sep1::client`): `validate_uri` (https / ipfs only, IP-literal / userinfo / RFC1035 reject + IPFS path-traversal reject), `Policy::limited(0)` redirect kill, `MAX_BODY_BYTES = 256 KB`, worker-side `is_safe_media_url` for `<img src>` XSS defence. Guard violations classified as permanent → sentinel write + warn log. Transient violations (Http 5xx / timeout / connect, SorobanRpc) → `EnrichError::Transient` → SQS retry → DLQ.

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

- [x] Migration drops `nfts.metadata` JSONB column; up + down land together (`20260507120000_drop_nfts_metadata.{up,down}.sql`).
- [x] Indexer + db-merge no longer reference `metadata` on INSERT / ON CONFLICT.
- [x] `ExtractedNft.metadata` removed; parser tests still pass (14/14 indexer persist integration).
- [x] Insert-hook emits exactly one `NftTokenUri` per mint; no re-emit on transfer/burn (`publish_for_minted_nfts` in `enrichment_publish.rs`).
- [x] Worker handles both `application/json` and `image/*` `token_uri` responses; image-only path synthesises `{"image": "<url>"}` so the worker writes only `media_url` (`name` / `collection_name` legitimately empty).
- [x] Error taxonomy + retry mapping: `is_transient` classifier (Http timeout/connect/5xx + SorobanRpc → transient; everything else → permanent / sentinel write). API detail handler folds errors fail-soft to `null` via `Option<Value>`.
- [ ] **Sample query:** non-NULL `name` / `media_url` / `collection_name` on minted NFTs from a known JSON-metadata collection (PR-time check against backfill region).
- [x] `runtime_enrichment::nft_token_uri::NftTokenUriFetcher::resolve` returns full JSON on detail endpoint with 24h LRU; fail-soft to `null`. Wire response field stays `metadata`.
- [x] List response no longer carries `metadata` field; detail response preserves it via runtime fetch.

**Common:**

- [x] Per-kind permanent/transient `EnrichError` mapping documented + unit-tested (`is_transient` + 4 unit tests).
- [x] §2d integration test: 5 wiremock-driven JSON-RPC tests (happy path, `error`, contract revert, missing `results`, 5xx). End-to-end metadata-URL HTTPS fetch under wiremock is intentionally not covered — `validate_uri` rejects loopback hosts on purpose; production hard-fail surfaces real-world misshapes via the DLQ.
- [x] No CDK delta — new kind dispatched by existing 0191 Lambda 2 / SQS / DLQ / DepthAlarm.
- [x] Docs updated: ADR 0043 matrix amendment marking `nfts.metadata` as runtime type-2; `docs/architecture/indexing-pipeline/enrichment.md` created; schema doc reflects column drop.
- [x] API types regenerated per `API types freshness` CI gate (`npx nx run @rumblefish/api-types:generate`).

## Future Work (out of scope)

- **Per-collection NFT batching** if per-token RPC cost prohibitive.
- **Per-kind CloudWatch metrics** (success/fail counters, fetch latency histograms).
- **`asset_usd_price`** — re-evaluate when product ask materialises.
- **Sentinel-in-VARCHAR retire**: replace `''` overload + `NULLIF` SQL with explicit `{icon_url, name}_attempted_at TIMESTAMPTZ` companion columns. Senior-correct status separation. Bundle with 0196's status-column work to retrofit both 0191's `icon_url` and 0195's `name` simultaneously.
