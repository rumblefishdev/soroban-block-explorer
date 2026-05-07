---
id: '0195'
title: 'Lambda 2 enrichment: off-chain NULL fields (icon-name extension, lp_tvl, nft_metadata)'
type: FEATURE
status: active
related_adr: ['0007', '0022', '0023', '0032', '0043']
related_tasks: ['0125', '0188', '0191', '0194', '0196', '0197']
tags: [priority-medium, effort-large, layer-enrichment, layer-lambda, audit-gap]
milestone: 2
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-05-06'
    status: backlog
    who: karolkow
    note: 'Spawned from M2 enrichment planning. Subsumes 0125 (LP TVL), 0191 future-work LP analytics + asset_usd_price.'
  - date: '2026-05-07'
    status: active
    who: karolkow
    note: |
      Activated. Spec consolidated (3 amendment passes folded in):
      §2c (asset_usd_price) pulled — no consumer.
      §2a scope corrected to ClassicCredit + SAC after parser audit (state.rs:824 explicit `name=None` on SAC; CAP-46-6 confirms SAC's on-chain `name()` is `code:issuer` machine ID, not human name).
      §2b/§2d producers pivoted to insert-hook (no live-path dedup; backfill → 0196).
      §2b oracle ordering corrected: StellarExpert primary, Reflector limited-scope (open question: §2b/§2d lack frontend consumer — same speculative case as pulled §2c, decision pending).
---

# Lambda 2 enrichment: off-chain NULL fields

## Summary

Three sub-blocks on 0191's SQS-driven type-1 enrichment worker, each populating a column that **cannot** be derived from the processed ledger:

- **§2a** — extend existing `icon` kind to also persist `assets.name` from the same SEP-1 fetch (ClassicCredit + SAC).
- **§2b** — `lp_tvl` kind: USD price oracle for `liquidity_pool_snapshots.tvl`.
- **§2d** — `nft_metadata` kind: Soroban RPC `token_uri()` + IPFS gateway for `nfts.{name,media_url,metadata}`.

(§2c asset_usd_price pulled — see history.)

All reuse `enrichment-shared`, worker dispatch, indexer producer, and the permanent/transient `EnrichError` taxonomy from 0191.

## Status: Active

ADR 0043 (field allocation rule) accepted on develop.

**PR ordering**: §2a (~30 LoC, no decisions pending) ships first. §2b gated on oracle-source decision. §2d gated on IPFS gateway + Soroban RPC sizing. Spec stays bundled for ADR 0043 amendment + docs coherence.

## Context

### Field allocation per ADR 0043

Off-chain = data NOT in processed ledger. Decisions:

- **`assets.name`** (asset_type IN (1, 2)) — SEP-1 TOML `CURRENCIES[].name`. ClassicCredit: no on-chain source (XDR `Asset` enum = code+issuer). SAC: on-chain `name()` returns `<code>:<issuer_strkey>` machine ID per CAP-46-6, not human name; project leaves `name=None` ([state.rs:820-827](crates/xdr-parser/src/state.rs:820)). Soroban-native (asset_type=3) is owned by indexer/0156. Native (asset_type=0) out of scope.
- **`liquidity_pool_snapshots.tvl`** — USD-denominated, requires oracle. Off-chain.
- **`nfts.{collection_name, name, media_url, metadata}`** — per-token `token_uri()` RPC + JSON fetch. Off-chain.

LP `volume` / `fee_revenue` are on-chain → task 0199 (depends on §2b oracle for USD half).

### Reuse from 0191

`enrichment-shared` lib (`sep1/`, `enrich_and_persist/`, EnrichError) + `enrichment-worker` Lambda (SqsEvent dispatch, `EnrichmentMessage` tagged enum) + `enrichment_publish.rs` Publisher + CDK queue/dlq/alarms + sentinel `''` pattern.

### Sentinel strategy per producer model

- **Query-and-batch** (§2a, inherited from 0191): producer re-SELECTs rows missing column on each ledger touch. Sentinel `''` REQUIRED — without it the same row is re-emitted forever. Update SQL `COALESCE(NULLIF($n, ''), col, $n)`: `real > sentinel > NULL` priority — sentinels are upgradable, real values stick, NULLs fill.
- **Insert-hook** (§2b, §2d): producer emits exactly once on row INSERT. No dedup needed live. Sentinels (`tvl=0`, `nfts.name=''`, etc.) exist for downstream UI fallback / WARN breadcrumb only. Backfill of pre-existing rows → 0196 (which owns its own status-column / `_attempted_at` strategy).

## Implementation Plan

### §2a — Icon kind extension: also persist `assets.name`

Cheapest path: extend the existing `icon` kind (already fetches the same TOML for `image`).

**Changes:**

- `Sep1Currency.name: Option<String>` ([dto.rs:32](crates/enrichment-shared/src/sep1/dto.rs)).
- `enrich_asset_from_sep1` (rename of `enrich_asset_icon`) writes both columns in one UPDATE:
  ```sql
  UPDATE assets
     SET icon_url = COALESCE(NULLIF($1, ''), icon_url, $1),
         name     = COALESCE(NULLIF($2, ''), name,     $2)
   WHERE id = $3
  ```
  Bind for `name`: `Some("real")` if SEP-1 yields, `Some("")` sentinel for permanent fail on asset_type IN (1, 2), `None` (NULL bind, COALESCE no-op) for asset_type 0/3 — protects indexer/0156-set Soroban-native names.
- Producer SQL: `WHERE icon_url IS NULL OR (asset_type IN (1, 2) AND name IS NULL)`.
- No new `EnrichmentMessage` variant — reuse `Icon { asset_id }` (one TOML fetch yields both fields).
- UI: list/detail must treat `name = ''` as "no SEP-1 name" → fall back to `asset_code`.

### §2b — `lp_tvl` `EnrichmentMessage` variant

Supersedes 0125 (LP TVL part; volume/fee_revenue → 0199).

- `EnrichmentMessage::LpTvl { pool_id: [u8; 32], snapshot_id: i64 }`.
- New module `enrich_and_persist/lp_tvl.rs` exposing `enrich_pool_tvl(pool, pool_id, snapshot_id, oracle)`.
- Compute `tvl = reserve_a × price_a_usd + reserve_b × price_b_usd`. UPDATE `liquidity_pool_snapshots.tvl`.
- Producer hook = INSERT-driven on each new snapshot row. Live exactly-once; backfill → 0196.
- Permanent fail: write `tvl=0` + WARN log (pool_id, snapshot_id, per-leg oracle errors). `0` ambiguous vs "empty pool" — disambiguator is the log.
- Transient (5xx, network, timeout) → `EnrichError::Transient`, SQS retry → DLQ.
- **Oracle source ordering — DECISION GATING (must resolve before merge; see AC):**
  1. Pegged-direct fast path (USDC/USDT/EURC issuer-StrKey allowlist).
  2. **StellarExpert** `/asset/<code>-<issuer>` `price7d` (broad coverage; likely real primary).
  3. **Reflector** Soroban on-chain feed (limited supported list; pin contract ID + asset codes).
  4. Horizon `/trade_aggregations` last resort.
  5. CoinGecko skipped (duplicates StellarExpert).

### §2c — REMOVED

Pulled 2026-05-06: speculative, no PM ticket / consumer / frontend mock. `assets.usd_price` column add (originally 0194 §1a) also pulled. Re-evaluate when product ask materialises.

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

- ADR 0043 amendment: per-kind matrix table for new kinds. (ADR 0029 NOT amended — read-path, not write-path.)
- `docs/architecture/indexing-pipeline/enrichment.md` (create if absent).
- `docs/architecture/database-schema/**` — column source attribution.

## Acceptance Criteria

**§2a:**

- [ ] `Sep1Currency.name` added; combined UPDATE with `COALESCE(NULLIF(...), col, ...)` priority `real > sentinel > NULL`.
- [ ] Sample query: non-NULL `name` on ClassicCredit + SAC assets with SEP-1 TOML support.
- [ ] Test: producer dedup `WHERE icon_url IS NULL OR (asset_type IN (1, 2) AND name IS NULL)` does not infinite-re-emit a sentinel-marked row.
- [ ] List/detail endpoints render `asset_code` when `name = ''`.

**§2b:**

- [ ] Insert-hook emits exactly one `LpTvl` per new snapshot.
- [ ] Oracle source ordering pinned in spec (no TBD at merge).
- [ ] `tvl=0` written on permanent fail with WARN log; transient → SQS retry.

**§2d:**

- [ ] Insert-hook emits exactly one `NftMetadata` per mint; no re-emit on transfer/burn.
- [ ] IPFS gateway choice pinned.
- [ ] Soroban RPC QPS estimate → DepthAlarm + concurrency cap derived.
- [ ] Sample query: non-NULL `name`/`media_url` on minted NFTs from a known collection.

**Common:**

- [ ] Per-kind permanent/transient `EnrichError` mapping documented + unit-tested.
- [ ] Per-kind integration test (mock oracle / mock RPC / mock IPFS gateway).
- [ ] CDK DepthAlarm thresholds reviewed for new producer rates.
- [ ] Docs updated (ADR 0043 amendment + enrichment.md + schema docs).
- [ ] API types regenerated if any DTO field exposed.
- [ ] 0125 archived as `superseded by: ["0195", "0199"]`.
- [ ] 0196 backlog updated to capture §2b/§2d backfill dedup ownership.

## Future Work (out of scope)

- **Per-collection NFT batching** if per-token RPC cost prohibitive.
- **`lp_tvl` periodic janitor** — only if observed stale TVLs in production.
- **Per-kind CloudWatch metrics** (success/fail counters, fetch latency histograms).
- **`asset_usd_price`** — re-evaluate when product ask materialises.
- **Sentinel-in-VARCHAR retire**: replace `''` overload + `NULLIF` SQL with explicit `{icon_url, name}_attempted_at TIMESTAMPTZ` companion columns. Senior-correct status separation. Bundle with 0196's status-column work to retrofit both 0191's `icon_url` and 0195's `name` simultaneously.
