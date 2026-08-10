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
   - [3.1 `sep1_assets` — SEP-1 issuer TOML](#31-sep1_assets--sep-1-issuer-toml)
   - [3.2 `nft_token_uri` — per-token contract view + IPFS](#32-nft_token_uri--per-token-contract-view--ipfs)
   - [3.3 Backfill drain — `crates/backfill-enrichment-runner` (task 0196)](#33-backfill-drain--cratesbackfill-enrichment-runner-task-0196)
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
- **NFT metadata via `token_uri()`** — `nfts.{name, media_url}` are
  produced by the contract's `token_uri(token_id)` view function
  returning a URL to a JSON document hosted on IPFS or HTTPS.
- **NFT collection name — ledger-primary, `name()` fallback** (task 0340
  parser-first redirect). The collection name is captured from the contract
  instance-storage metadata struct into `soroban_contract_metadata` by the
  indexer (the OpenZeppelin `NFTStorageKey::Metadata` key — Fix A / #330) and
  served on `nfts.collection_name` via
  `COALESCE(soroban_contract_metadata.name, nft_enrichment.collection_name)`
  (Fix B / #331). It is **not** in the `token_uri()` JSON (no real-world
  Stellar NFT emits a `"collection"` field — 0/68 on prod). The contract-level
  SEP-50 `name()` RPC `simulateTransaction` is a FALLBACK only, for hand-rolled
  contracts the ledger can't reach (empty instance storage, `name()` baked in
  WASM); cached per-CONTRACT (one RPC per collection, never per token), written
  into the enrichment column the COALESCE reads under the ledger name.

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

### 3.1 `sep1_assets` — SEP-1 issuer TOML

> **Naming:** Rust identifier `Sep1Assets`
> (`EnrichmentMessage::Sep1Assets`, `Kind::Sep1Assets`, CLI subcommand
> `sep1-assets`); SQS wire `kind` = `"sep1_assets"` via serde
> `rename_all = "snake_case"`. **Renamed in 0196 from the historical
> `"icon"`** — the kind has written both `assets.icon_url` AND
> `assets.name` since 0195 §2a, so the `icon` name was misleading.
> **Breaking wire change**: pre-0196 SQS messages or DLQ entries with
> `"kind":"icon"` will not deserialise against the current worker;
> drain the DLQ before deploying 0196.

|                     |                                                                                                                                                                                        |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Task**            | 0191 (worker scaffold) + 0195 §2a (extended to also write `name`)                                                                                                                      |
| **Producer hook**   | Query-and-batch: after commit, re-SELECT asset rows that match the parser's `ExtractedAsset` slice AND are still missing `icon_url IS NULL OR (asset_type IN (1, 2) AND name IS NULL)` |
| **Worker source**   | `https://{accounts.home_domain}/.well-known/stellar.toml` (per-issuer TOML)                                                                                                            |
| **Columns written** | `assets.icon_url`, `assets.name` (ClassicCredit + SAC only)                                                                                                                            |
| **Failure mode**    | **Fail-soft** — permanent fails write the `''` sentinel so the producer dedup query short-circuits next ledger touch. Transient → SQS retry → DLQ                                      |
| **UPDATE SQL**      | `COALESCE(NULLIF($n, ''), col, $n)` per column — priority `real > sentinel > NULL`                                                                                                     |

### 3.2 `nft_token_uri` — per-token contract view + IPFS

|                         |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Task**                | 0195 §2d                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| **Producer hook**       | Insert-hook: after commit, SELECT `nfts.id WHERE minted_at_ledger = ANY(...) AND (name IS NULL OR media_url IS NULL OR collection_name IS NULL)`. The producer emits exactly once per `nft_id` under normal operation; all three columns are checked so a partial-fill row (e.g. sentinel on one column, NULL on another from a flap) re-emits until every column lands a value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| **Worker source**       | `name` / `media_url`: Soroban RPC `simulateTransaction` of `InvokeContract(token_uri(token_id))` → URI string → HTTP fetch (resolving `ipfs://` via IPFS gateway) → JSON metadata. `collection_name`: **the ledger is primary, not RPC** (task 0340 parser-first redirect). The indexer parses it from contract instance storage into `soroban_contract_metadata.name` (Fix A / #330) and the read path serves `COALESCE(soroban_contract_metadata.name, nft_enrichment.collection_name)` (Fix B / #331), so nothing is fetched on the primary path. The contract-level SEP-50 `name()` `simulateTransaction` is a **fallback only**, for hand-rolled contracts the ledger can't reach (empty instance storage, name baked into WASM); cached per-CONTRACT (one RPC per collection, never per token). Measured on prod 2026-07-17: **54 of 66** hot collections carry a ledger name. The earlier "0/68" that motivated the RPC-first design was a key-matcher bug (the extractor missed OpenZeppelin's `Vec([Symbol("Metadata")])` key), not ground truth. |
| **Token id assumption** | `ScVal::U32` — OpenZeppelin / ERC-721 sequential-mint-counter convention. Token ids that don't parse as `u32` surface as `MalformedInput { field: "token_id (not u32)", … }` which `is_transient` classifies as **permanent** → sentinel write + warn log + SQS ack (the SQS retry budget is reserved for transient 5xx / network blips). Operators grep the warn log to identify contracts using non-OZ token-id schemes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| **Content-Type branch** | `application/json` → parsed JSON used directly. `image/*` → fetcher synthesises `{ "image": "<url>" }` so the worker writes only `media_url` (direct-image convention, e.g. JamesBachini Soroban example). Anything else → `UnsupportedContentType` (permanent)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| **Columns written**     | `nfts.name`, `nfts.media_url`, `nfts.collection_name`. The dropped `nfts.metadata` JSONB column is served at request time by the API via `runtime_enrichment::nft_token_uri` (per ADR 0043 detail-only carve-out)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| **Failure mode**        | Fetcher returns `Result<Option<Value>, Arc<NftTokenUriError>>`. Worker classifies via `is_transient` (Http 5xx / connect / timeout, SorobanRpc → transient; everything else → permanent). Transient → `EnrichError::Transient` → SQS retry → DLQ. Permanent → `''` sentinel write + warn log so the row is recorded as "tried, nothing available" and the predicate `name IS NULL` short-circuits next ledger touch. Api detail handler folds errors fail-soft to `null` via `.ok().flatten()` (plus a 3 s wall-clock timeout so a slow IPFS gateway can't approach the API Gateway ceiling).                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| **UPDATE SQL**          | `COALESCE(NULLIF($n, ''), col, $n)` per column — same shape as the `sep1_assets` kind, protects real values from being clobbered by sentinel writes on a flap                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |

## 3.3 Backfill drain — `crates/backfill-enrichment-runner` (task 0196)

The worker is **forward-only** — it processes rows the indexer emits onto
SQS after the queue's deployment. Rows that pre-date the queue are
covered by a one-shot drain CLI in `crates/backfill-enrichment-runner`:
single `enrich` binary with `sep1-assets` / `nft-metadata` /
`nft-collection-name` / `status` subcommands. The drain calls the
**same** `enrichment_shared::enrich_and_persist::*` functions the worker
calls, so a row enriched via backfill is bit-identical to one enriched
via SQS.

The `nft-collection-name` subcommand (task 0340) is the one exception to
the per-row shape: it walks DISTINCT **contracts** whose `nft_enrichment`
rows still lack a `collection_name` AND that have no ledger-sourced name in
`soroban_contract_metadata` (the parser-first redirect made `name()` a
FALLBACK — ledger-covered contracts are excluded from the cohort, never
re-fetched). For each it fetches `name()` once per contract,
then re-INSERTs that contract's rows with the name stamped on and their
existing `name` / `media_url` PRESERVED (the side table is a
`ReplacingMergeTree` with whole-row replace — a column-only update is
impossible). It exists because rows enriched before 0340 carry real
`name` / `media_url` but an empty `collection_name`, and so match
neither the default "no row yet" drain nor `--retry-sentinels` (which
requires ALL columns empty).

|                 |                                                                                                                                                                                                                        |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Trigger**     | Operator-on-demand only (never a cron). One-shot after deploying a new kind, plus `--force-retry` re-walks after upstream fixes (issuer fixes TOML).                                                                   |
| **Drain shape** | Cursor SELECT `WHERE <kind predicate> AND id > $last ORDER BY id LIMIT N` → `tokio::spawn` fan-out bounded by `Semaphore` (default 10) → `enrich_*`                                                                    |
| **No SQS**      | Direct DB → `enrich_*` → DB. A 50K-row publish would hit SQS rate limits and waste visibility-timeout overhead when we already hold a DB connection.                                                                   |
| **Predicates**  | `sep1-assets`: producer-aligned (`icon_url IS NULL OR (asset_type IN (1, 2) AND name IS NULL)`). `nft-metadata`: stricter than producer (all three NULL — only pre-queue rows). `--force-retry` drops the predicate.   |
| **Pool**        | `concurrency + 2` for drain, `2` for `status`. Sized at the subcommand boundary so fan-out is not throttled.                                                                                                           |
| **Exit code**   | `0` on a clean run (every row reached a terminal outcome — real value or `''` sentinel), `1` on transient or DB failure. Operator-chainable (`enrich sep1-assets && enrich nft-metadata`).                             |
| **Crate split** | Separate from `crates/backfill-runner` (which re-ingests Stellar ledgers from XDR archives). Different data sources, different concerns; task 0191 design decision #8 forbids modifying the ledger-backfill code path. |

See [`crates/backfill-enrichment-runner/README.md`](../../../crates/backfill-enrichment-runner/README.md)
for the runbook and pre-flight checklist.

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

| Knob                        | Value                                                   | Where                                                                                                                                                         |
| --------------------------- | ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Worker reserved concurrency | 5                                                       | CDK enrichment stack (task 0191)                                                                                                                              |
| SQS visibility timeout      | 30 s                                                    | Same                                                                                                                                                          |
| Max receives → DLQ          | 3                                                       | Same                                                                                                                                                          |
| DepthAlarm threshold        | inherited from 0191                                     | Same                                                                                                                                                          |
| Cache TTL (both kinds)      | 24 h                                                    | `Sep1Fetcher` / `NftTokenUriFetcher`                                                                                                                          |
| Cache capacity (both kinds) | 1024 entries                                            | Same                                                                                                                                                          |
| RPC pool                    | 4 keyless endpoints (SDF, gateway.fm, Ankr, OnFinality) | `DEFAULT_SOROBAN_RPC_URLS` in code — round-robin + failover (task 0311); shared by worker, API and backfill CLI. `SOROBAN_RPC_URLS` env is an ad-hoc override |
| IPFS gateways               | `ipfs.io` + `gateway.pinata.cloud`                      | `DEFAULT_IPFS_GATEWAYS` in code — failover pair (task 0311; the prior single `cloudflare-ipfs.com` was sunset). `IPFS_GATEWAY_BASES` env overrides            |
