# Stellar Block Explorer — Type-1 Enrichment

> Companion to [`indexing-pipeline-overview.md`](./indexing-pipeline-overview.md).
> Describes the SQS-driven type-1 enrichment worker introduced in task
> 0191 and extended in task 0195. Field-allocation policy is governed
> by [ADR 0043](../../../lore/2-adrs/0043_field-allocation-rule.md).
>
> **Scope:** only the write-side type-1 path (worker Lambda persists
> off-chain data into typed columns). The complementary type-2 path
> (per-request fetch in the API handler, no persistence) is documented
> in [`backend/backend-overview.md`](../backend/backend-overview.md)
> under `runtime_enrichment::*`. The two paths share the same fetcher
> code (`enrichment_shared::{sep1, nft_token_uri}`) but route the data
> to different consumers — see ADR 0043 for which fields land where.

---

## Table of Contents

1. [Why type-1 enrichment exists](#1-why-type-1-enrichment-exists)
2. [Pipeline shape](#2-pipeline-shape)
3. [Enrichment kinds](#3-enrichment-kinds)
   - [3.1 `icon` — SEP-1 issuer TOML](#31-icon--sep-1-issuer-toml)
   - [3.2 `nft_token_uri` — per-token contract view + IPFS](#32-nft_token_uri--per-token-contract-view--ipfs)
4. [Defensive guards](#4-defensive-guards)
5. [Failure model](#5-failure-model)
6. [Operational knobs](#6-operational-knobs)

---

## 1. Why type-1 enrichment exists

The indexer (Lambda 1) parses XDR ledger files and writes typed columns
that are derivable purely from the processed ledger. Some columns the
API needs are **off-chain** — derivable only by an extra round-trip
beyond the indexer's standard stream:

- **SEP-1 TOML** — `assets.icon_url`, `assets.name` (classic credit /
  SAC) are published by issuers at
  `https://{home_domain}/.well-known/stellar.toml`.
- **NFT metadata via `token_uri()`** — `nfts.{name, media_url,
collection_name}` are produced by the contract's
  `token_uri(token_id)` view function returning a URL to a JSON
  document hosted on IPFS or HTTPS.

ADR 0043 keeps these off the indexer write path: an extra round-trip
per row would slow ingest below the ~5 s ledger cadence. Instead the
indexer publishes one SQS message per row-needing-enrichment after its
persistence transaction commits; a dedicated worker Lambda consumes
the queue, fetches the data, and writes the typed columns.

## 2. Pipeline shape

```
        ┌──────────────────────────┐
        │ Indexer (Lambda 1)       │
        │ commits ledger writes    │
        │ then publishes SQS msgs  │
        └────────────┬─────────────┘
                     │
                     ▼
        ┌──────────────────────────┐
        │ Enrichment queue (SQS)   │
        │ + DLQ + DepthAlarm       │
        └────────────┬─────────────┘
                     │
                     ▼
        ┌──────────────────────────┐
        │ Enrichment worker        │
        │ (Lambda 2, reserved      │
        │  concurrency = 5)        │
        │  • match msg.kind        │
        │  • fetch off-chain data  │
        │  • UPDATE typed columns  │
        └──────────────────────────┘
```

The worker dispatches each `SqsMessage` to a per-kind handler in
`enrichment_shared::enrich_and_persist`. Kinds plug in by adding a new
`EnrichmentMessage` enum variant + match arm in
`crates/enrichment-worker/src/main.rs` — **no CDK change** (the same
queue / DLQ / alarm / concurrency cap absorbs every kind).

## 3. Enrichment kinds

### 3.1 `icon` — SEP-1 issuer TOML

|                     |                                                                                                                                                                                        |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Task**            | 0191 (worker scaffold) + 0195 §2a (extended to also write `name`)                                                                                                                      |
| **Producer hook**   | Query-and-batch: after commit, re-SELECT asset rows that match the parser's `ExtractedAsset` slice AND are still missing `icon_url IS NULL OR (asset_type IN (1, 2) AND name IS NULL)` |
| **Worker source**   | `https://{accounts.home_domain}/.well-known/stellar.toml` (per-issuer TOML)                                                                                                            |
| **Columns written** | `assets.icon_url`, `assets.name` (ClassicCredit + SAC only)                                                                                                                            |
| **Failure mode**    | **Fail-soft** — permanent fails write the `''` sentinel so the producer dedup query short-circuits next ledger touch. Transient → SQS retry → DLQ                                      |
| **UPDATE SQL**      | `COALESCE(NULLIF($n, ''), col, $n)` per column — priority `real > sentinel > NULL`                                                                                                     |

### 3.2 `nft_token_uri` — per-token contract view + IPFS

|                         |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Task**                | 0195 §2d                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| **Producer hook**       | Insert-hook: after commit, SELECT `nfts.id WHERE minted_at_ledger = ANY(...) AND (name IS NULL OR media_url IS NULL OR collection_name IS NULL)`. The producer emits exactly once per `nft_id` under normal operation; all three columns are checked so a partial-fill row (e.g. sentinel on one column, NULL on another from a flap) re-emits until every column lands a value                                                                                                                                                                                                               |
| **Worker source**       | Soroban RPC `simulateTransaction` of `InvokeContract(token_uri(token_id))` → URI string → HTTP fetch (resolving `ipfs://` via Cloudflare gateway) → JSON metadata                                                                                                                                                                                                                                                                                                                                                                                                                             |
| **Token id assumption** | `ScVal::U32` — OpenZeppelin / ERC-721 sequential-mint-counter convention. Token ids that don't parse as `u32` surface as `MalformedInput { field: "token_id (not u32)", … }` which `is_transient` classifies as **permanent** → sentinel write + warn log + SQS ack (the SQS retry budget is reserved for transient 5xx / network blips). Operators grep the warn log to identify contracts using non-OZ token-id schemes                                                                                                                                                                     |
| **Content-Type branch** | `application/json` → parsed JSON used directly. `image/*` → fetcher synthesises `{ "image": "<url>" }` so the worker writes only `media_url` (direct-image convention, e.g. JamesBachini Soroban example). Anything else → `UnsupportedContentType` (permanent)                                                                                                                                                                                                                                                                                                                               |
| **Columns written**     | `nfts.name`, `nfts.media_url`, `nfts.collection_name`. The dropped `nfts.metadata` JSONB column is served at request time by the API via `runtime_enrichment::nft_token_uri` (per ADR 0043 detail-only carve-out)                                                                                                                                                                                                                                                                                                                                                                             |
| **Failure mode**        | Fetcher returns `Result<Option<Value>, Arc<NftTokenUriError>>`. Worker classifies via `is_transient` (Http 5xx / connect / timeout, SorobanRpc → transient; everything else → permanent). Transient → `EnrichError::Transient` → SQS retry → DLQ. Permanent → `''` sentinel write + warn log so the row is recorded as "tried, nothing available" and the predicate `name IS NULL` short-circuits next ledger touch. Api detail handler folds errors fail-soft to `null` via `.ok().flatten()` (plus a 3 s wall-clock timeout so a slow IPFS gateway can't approach the API Gateway ceiling). |
| **UPDATE SQL**          | `COALESCE(NULLIF($n, ''), col, $n)` per column — same shape as the `icon` kind, protects real values from being clobbered by sentinel writes on a flap                                                                                                                                                                                                                                                                                                                                                                                                                                        |

## 4. Defensive guards

Both kinds share a defensive layer at the HTTP boundary
(`crates/enrichment-shared/src/{sep1,nft_token_uri}/client.rs`):

| Guard                             | What it does                                                                                                                                                                        |
| --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Policy::limited(0)`              | reqwest refuses any 3xx redirect — a malicious issuer / contract cannot bounce us to a loopback / link-local / RFC-1918 host after the up-front host check passed                   |
| URI validation                    | Rejects non-`https://` schemes (and `ipfs://` for `nft_token_uri`), IP literals, `userinfo` (`user:pass@`), bare hostnames without a dot                                            |
| Body cap                          | `MAX_BODY_BYTES` (100 KB for SEP-1, 256 KB for `nft_token_uri`) — streamed chunk-by-chunk, bails before fully buffering                                                             |
| `is_safe_media_url` (worker side) | Re-checks the `image` field inside JSON metadata before writing it to `nfts.media_url` — defence in depth against a contract that smuggles `javascript:` / `data:` past the fetcher |

## 5. Failure model

Two-bucket error split mirrored across both kinds via
`enrich_and_persist::EnrichError`:

- **Transient** — network blip, 5xx, RPC outage, IPFS gateway slow.
  Returns `EnrichError::Transient` → SQS reports a per-record
  `BatchItemFailure` → message redelivers per `redrivePolicy.maxReceiveCount`
  → lands in the DLQ if the upstream issue persists. The DLQ
  DepthAlarm pages the operator.
- **Permanent** — 4xx, malformed JSON, unsupported content type,
  unsafe scheme, malformed `token_uri()` return shape, malformed
  contract StrKey, non-u32 `token_id`. The message is acked (no retry)
  and the worker writes the `''` sentinel so the row records "tried,
  nothing available" and the producer dedup predicate short-circuits.

The api crate's `runtime_enrichment::nft_token_uri` is the detail-side
twin of the worker: same fetcher (same cache TTL, same guards), but
the call site `.await`s the `Option<Value>` directly — fetcher errors
collapse to `null` `metadata` on the wire so the API never 5xx's
because of an enrichment failure.

## 6. Operational knobs

| Knob                        | Value                                         | Where                                                                                                                                |
| --------------------------- | --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Worker reserved concurrency | 5                                             | CDK enrichment stack (task 0191)                                                                                                     |
| SQS visibility timeout      | 30 s                                          | Same                                                                                                                                 |
| Max receives → DLQ          | 3                                             | Same                                                                                                                                 |
| DepthAlarm threshold        | inherited from 0191                           | Same                                                                                                                                 |
| Cache TTL (both kinds)      | 24 h                                          | `Sep1Fetcher` / `NftTokenUriFetcher`                                                                                                 |
| Cache capacity (both kinds) | 1024 entries                                  | Same                                                                                                                                 |
| RPC URL                     | `https://mainnet.sorobanrpc.com` (SDF public) | Hardcoded constant. Switch to dedicated provider (Quicknode, Validation Cloud) by changing `DEFAULT_SOROBAN_RPC_URL` if rate-limited |
| IPFS gateway                | `https://cloudflare-ipfs.com/ipfs/`           | Hardcoded constant. Adding a fallback chain (Pinata, dweb.link) is Future Work                                                       |
