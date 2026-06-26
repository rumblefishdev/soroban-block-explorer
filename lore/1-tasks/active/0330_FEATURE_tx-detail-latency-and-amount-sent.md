---
id: '0330'
title: 'tx-detail: cut E3 latency (cache + overlap archive fetch) + show amount sent in UI'
type: FEATURE
status: active
related_adr: ['0029']
related_tasks: []
tags:
  [
    'phase-current',
    'effort-medium',
    'priority-high',
    'api',
    'frontend',
    'performance',
  ]
links: []
history:
  - date: 2026-06-25
    status: active
    who: fmazur
    note: 'Task created — prod GET /v1/transactions/:hash up to 3.37s; review asks to show amount sent.'
---

# tx-detail: cut E3 latency (cache + overlap archive fetch) + show amount sent in UI

## Summary

`GET /v1/transactions/:hash` (E3) takes up to **3.37s** in production. The
latency is dominated by the read-time heavy-field fetch (ADR 0029):
`StellarArchiveFetcher::fetch_ledger` does an **uncached, cross-region** S3 GET
of the whole ledger `.xdr.zst` (Lambda in eu-central-1 → public archive bucket
in us-east-2), then zstd-decompresses and deserializes the entire ledger batch.
This task reduces that latency (in-process cache + overlap the S3 fetch with the
DB queries) and, separately, surfaces the **amount sent** in the transaction
detail UI — a review ask. The amount already exists in the API response
(`heavy.operations[].details.amount`), so the UI part is frontend-only.

## Context

Measured: ledger file ~178 KB compressed / ~1.5 MB decompressed; cross-region
transfer from Europe ~1–2.7s (TTFB ~0.5–1s + TLS). `stellar_archive/mod.rs`
explicitly notes _"No caching — follow-up task if needed"_. The three CH detail
queries (PK seeks to Hetzner) are minor by comparison but run fully sequentially
before the S3 fetch in `handlers.rs::get_transaction`.

The project's own `LedgerBucket` (eu-central-1) is **not** a full archive mirror,
so a same-region source for arbitrary historical ledgers is not available — hence
caching + overlap rather than a region switch.

## Implementation Plan

### Step 1: In-process cache for the archive fetch (API)

- Add a moka cache (via `crate::cache::ttl_cache`) keyed by `ledger_sequence →
Arc<LedgerCloseMeta>` (or `tx_hash → Arc<E3HeavyFields>` to also skip the
  per-request re-parse — decide during impl, document under Emerged).
- tx detail is immutable once finalized → long TTL, bounded capacity.
- Wire through `StellarArchiveFetcher` / `RuntimeEnrichment` / `AppState`.

### Step 2: Overlap S3 fetch with DB queries (API)

- `lookup_hash_ledger` yields `ledger_sequence` early. Kick off `fetch_ledger`
  concurrently (`tokio::join!`) with `fetch_detail` + `fetch_operations` instead
  of running it strictly after them.

### Step 3: Show amount sent (frontend)

- `web/src/pages/transaction-detail/normal/humanizeOp.ts` builds "Sent XLM to
  GA5X…GKTM" but omits the amount. Read the amount from `heavy.details`
  (`amount` for PAYMENT, `sendAmount`/`destAmount` for path payments,
  `startingBalance` for CREATE_ACCOUNT), format stroops → display (÷ 1e7), and
  render e.g. "Sent 100.50 XLM to GA5X…GKTM". Falls back to current text when
  heavy is unavailable.

## Acceptance Criteria

- [x] Repeat E3 requests for the same tx (warm Lambda) skip the S3 GET + XDR parse (cache hit) — `transactions::cache` (per-hash heavy, `try_get_with` single-flight).
- [x] S3 fetch runs concurrently with the DB detail/operations queries — `tokio::join!` in `handlers::get_transaction`.
- [x] Transaction detail UI shows the sent amount + asset for payment-type operations — `humanizeOp.ts` + `formatTokenAmount`.
- [x] Graceful fallback when `heavy_fields_status = "unavailable"` (no amount, old text) — covered by `humanizeOp` test + only-successes-cached.
- [x] **Docs updated** — `docs/architecture/backend/backend-overview.md` notes the new E3 warm cache + cross-region context.
- [x] **API types regenerated** — `N/A`: `nx run @rumblefish/api-types:generate` produced **no diff** (response shape unchanged; cache/overlap internal, UI reads existing `heavy.details` fields).

## Implementation Notes

**API (`crates/api`):**

- `transactions/cache.rs` — new `TxHeavyCache` (`moka::future::Cache<String, Arc<E3HeavyFields>>`), 30-min TTL, **byte-bounded** via a `weigher` (12 MB budget, `approx_heavy_bytes`). The weigher charges `JSON_NODE_BYTES` (~24 B) **per value node** plus content length, so the weight tracks real resident RAM of the `serde_json::Value` tree (not just serialized length). `try_get_with` gives stampede protection; only `Ok(Some(heavy))` is cached.
- `state.rs` — `AppState.tx_heavy_cache` field, built in `new()` (so `for_tests` is unchanged).
- `handlers.rs::get_transaction` — extracted `compute_heavy_cached()` (cache + ADR 0029 read path + lore-0046 parse_error skip + out-of-range degrade; the single-flight initialiser borrows `state`/`hash` so only the cache key is allocated), and `tokio::join!`-ed it with `fetch_operations_for_source` to overlap the DB ops query under the archive latency.
- `runtime_enrichment/stellar_archive/mod.rs::fetch_ledger` — the synchronous zstd decompress + full-batch XDR deserialize now runs on `tokio::task::spawn_blocking` so it does not stall the async worker (and genuinely overlaps the co-`join!`ed DB query).

**Frontend:**

- `libs/ui/src/format/stroops.ts` — extracted private `stroopsToDecimal()` (BigInt, accepts string for large-value precision), refactored `formatFee` onto it (behaviour identical), added exported `formatTokenAmount(stroops, assetCode?)`.
- `web/.../normal/humanizeOp.ts` — payment/path-payment/create-account arms now read the amount from `heavy.details` (`amount` / `sendAmount` / `destAmount` / `startingBalance`) and the unit from the details asset (`native`→XLM, `CODE:ISSUER`→CODE), falling back to the prior asset-only label when heavy is absent.

**Tests:** `stroops.test.ts` (6), `humanizeOp.test.ts` (9), `cache.rs` (4). All green; lint/typecheck/rustfmt/prettier clean.

## Review (5-agent parallel review, `/code-review`-style)

Five subagents reviewed the diff (Rust correctness, data-leak/security, DB-query performance, cache memory/concurrency, frontend/precision). No Critical/High. Security: APPROVE (cache key = full normalized lowercase hash → no collision / wrong-tx serve; React auto-escapes the label). Findings addressed in-task:

- **Weigher under-counted scalar-heavy JSON (Medium)** → per-node charge + budget 24 MB → 12 MB (above).
- **Sync XDR parse on the async worker (Low)** → `spawn_blocking` in `fetch_ledger` (above).
- **Double hash allocation (Nit)** → initialiser now borrows; one alloc.
- **STRICT_SEND comment/test gap, dead `?? light.asset_code` fallback, precision-comment honesty (Low/Nit)** → `amountFieldsFor` comment corrected, fallback simplified to `unit ?? 'XLM'`, `asAmount`/`formatTokenAmount` docs note the wire form is a number (large-amount lossiness); added `STRICT_SEND`, zero-amount, and heavy-present-but-no-amount tests.

Accepted as-is (not bugs): on an ops-query error the archive fetch still runs under `join!` (wasted work on a rare error path, result still valid); warn-level S3 error log (server-side only, pre-existing); per-hit deep clone of the heavy block (cheaper than the fetch it replaces; threading `Arc` through `merge_e3_response` needs serde `rc`).

**Second pass** (same 5 lenses, re-run on the fixed diff): all five returned CLEAN / ship-it. Remaining items addressed:

- **`JoinError` mislabeled as `FetchError::S3` (Low, 2 reviewers)** → added a dedicated `FetchError::Join` variant; a parse-task panic now logs honestly and degrades to `unavailable` (it never cancels, so a join error = panic).
- **Weigher test didn't exercise the per-node charge (Low)** → added `weigher_charges_per_node_for_scalar_heavy_trees` (1000 scalar nodes, near-zero content) asserting `>= JSON_NODE_BYTES * N` — would fail under the old content-only weigher.
- **`spawn_blocking` comment overstated "overlap with the DB query" (Nit)** → reworded to "doesn't monopolise the async worker / block the reactor" (the PK-seek DB query usually finishes during the S3 download anyway).
- **Single-entry-over-budget undocumented (Nit)** → one-line comment near `CACHE_WEIGHT_BUDGET` (such an outlier is never cached → refetch, degrades safely).
- **CREATE_ACCOUNT-without-`startingBalance` fallback untested (Nit)** → added that test.

Doc-drift on the budget (24→12 MB) reflects the first-pass fix and is intentional, not stale.

## Design Decisions

### From Plan

1. **Per-Lambda moka warm cache + `tokio::join!` overlap** — as scoped in the plan.

### Emerged

2. **Cache keyed by tx hash → `E3HeavyFields`, NOT ledger_sequence → `LedgerCloseMeta`.** The plan left this open. The API Lambda is **256 MB** (`infra/envs/production.json`); a parsed ledger is ~1.5 MB decompressed, so caching whole ledgers would blow the budget at a handful of entries. The extracted per-tx heavy block is a few KB–tens of KB, is memory-safe, AND skips the re-parse too. Trade-off: two distinct txs in the same ledger each fetch once — accepted, since the hot pattern is repeat views of the _same_ tx.
3. **Byte-bounded cache (moka `weigher`), not a fixed entry count.** Heavy blocks span orders of magnitude (plain payment ~few KB; Soroban tx with full event topics/data ~hundreds of KB), so an entry cap would let a burst of large detail views push the 256 MB Lambda toward OOM. The `weigher` (`approx_heavy_bytes`) caps the cache by **12 MB** and charges `JSON_NODE_BYTES` (~24 B) **per `serde_json::Value` node** in addition to content length — so a scalar/many-key-dominated tree (near-zero serialized length but allocation-heavy in RAM) is weighted by its real footprint, not under-counted (review fix). Large entries evict more small ones; overflow → TinyLFU eviction → fallback to the normal archive fetch (never an error).
4. **Only successful extractions are cached.** `try_get_with` returns `Err(())` on archive failure / tx-not-in-ledger and moka does not cache `Err`, so a degraded `unavailable` response stays retryable — matching the existing `SHORT` cache-control. `parse_error` rows are short-circuited before the cache.
5. **Unit derived from `heavy.details` asset, not solely `light.asset_code`.** For path payments `light.asset_code` is ambiguous (send vs dest); reading `sendAsset`/`destAsset` from heavy keeps the unit correct, with `asset_code` as fallback.

## Future Work

- **Large-amount precision:** `serde_json` serialises i64 stroops as a JSON number; JS `JSON.parse` is lossy above 2^53 stroops (~900M XLM). `formatTokenAmount`/`asAmount` already accept a string to stay exact, but the wire value is a number. If exactness matters, serialise amounts as strings (API change + api-types regen — outward-facing, deferred deliberately). (spawn if needed)
- **AWS SDK retry/timeout tuning** to avoid cross-region compounding (noted below).
- _(Done in-task, was a review finding)_ `spawn_blocking` for the zstd + XDR parse.

## Notes

- Memory: `tx-detail-archive-cross-region-latency`.
- Consider tuning AWS SDK retry/attempt-timeout to avoid cross-region compounding (possible follow-up).
- Possible larger follow-up (out of scope): persist a minimal heavy subset (amount/memo/result_code) in DB to avoid the archive on the hot path — conflicts with ADR 0029 tradeoff.
