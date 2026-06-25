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

- [ ] Repeat E3 requests for the same tx (warm Lambda) skip the S3 GET + XDR parse (cache hit).
- [ ] S3 fetch runs concurrently with the DB detail/operations queries.
- [ ] Transaction detail UI shows the sent amount + asset for payment-type operations.
- [ ] Graceful fallback when `heavy_fields_status = "unavailable"` (no amount, old text).
- [ ] **Docs updated** — `docs/architecture/**` if the read-path/caching shape changes (ADR 0029 area); else `N/A — reason`.
- [ ] **API types regenerated** — `N/A` unless the response shape changes (cache/overlap are internal; UI reads existing fields).

## Notes

- Memory: `tx-detail-archive-cross-region-latency`.
- Consider tuning AWS SDK retry/attempt-timeout to avoid cross-region compounding (possible follow-up).
- Possible larger follow-up (out of scope): persist a minimal heavy subset (amount/memo/result_code) in DB to avoid the archive on the hot path — conflicts with ADR 0029 tradeoff.
