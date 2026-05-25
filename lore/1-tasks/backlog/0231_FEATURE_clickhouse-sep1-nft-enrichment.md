---
id: '0231'
title: 'FEATURE: ClickHouse port of SEP-1 + NFT token_uri enrichment (no Lambda/SQS)'
type: FEATURE
status: backlog
related_adr: ['0044', '0045']
related_tasks: ['0195', '0196', '0212', '0214', '0228']
blocked_by: ['0228']
tags:
  [
    priority-medium,
    effort-large,
    layer-data,
    clickhouse,
    enrichment,
    sep1,
    nft,
    post-merge,
  ]
milestone: 2
links: []
history:
  - date: '2026-05-18'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from the 2026-05-18 CH-enrichment planning session
      (plan file `~/.claude/plans/powiedz-jaki-jest-status-dynamic-avalanche.md`).
      Stage 2 of the two-stage plan; Stage 1 (Tier-1 column rebuild +
      asset aggregates + NFT Phase 3 reclassify) lives inside task 0228.
      Stage 2 deliberately split out because porting SEP-1 + NFT
      `token_uri` enrichment to CH is a fresh-architecture problem
      (no Lambda, no SQS in the CH stack), large enough to warrant its
      own task. Blocked on 0228 landing on Hetzner so we have a stable
      production CH to write the enrichment runner against.
---

# FEATURE: ClickHouse port of SEP-1 + NFT `token_uri` enrichment

## Summary

Port the two per-row Lambda+SQS enrichment workers that today fill
Postgres `assets.{icon_url, name}` and `nfts.{name, media_url,
collection_name}` to run against the production Hetzner ClickHouse —
without Lambda and without SQS, both of which exist only in the PG
stack. Schema columns are already present on CH (see
[`crates/db-clickhouse/schema/init.sql`](../../../crates/db-clickhouse/schema/init.sql))
and have been sitting NULL since the CH pilot landed.

## Status: backlog

Blocked on [task 0228](../active/0228_FEATURE_parallel-backfill-merge-and-validation/README.md)
finishing the parallel-backfill merge + Stage 1 post-merge repair pass
on Hetzner. Stage 1 fills `accounts.first_seen_ledger`,
`lp_positions.first_deposit_ledger`, NFT/contract Tier-1 columns, and
asset aggregates — but leaves `assets.{icon_url, name}` and
`nfts.{name, media_url, collection_name}` NULL. This task fills them.

## Context

PG side already does this in two SQS-driven Lambdas:

- [`crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs`](../../../crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs)
  — fetches issuer TOML via `Sep1Fetcher`, writes back to PG via SQL
  UPDATE with `COALESCE(NULLIF(…))` priority logic.
- [`crates/enrichment-shared/src/enrich_and_persist/nft_token_uri.rs`](../../../crates/enrichment-shared/src/enrich_and_persist/nft_token_uri.rs)
  — fetches contract `token_uri()` via Soroban RPC + IPFS gateway via
  `NftTokenUriFetcher`, same write-back pattern.

Both fetchers (`Sep1Fetcher` and `NftTokenUriFetcher`, in
[`crates/enrichment-shared/src/sep1/client.rs`](../../../crates/enrichment-shared/src/sep1/client.rs)
and
[`crates/enrichment-shared/src/nft_token_uri/client.rs`](../../../crates/enrichment-shared/src/nft_token_uri/client.rs))
are storage-agnostic — pure HTTP fetch + parse, `moka` LRU caches.
They get **reused verbatim** by this task. The SQS dispatch loop and
PG-coupled persist paths are NOT ported; CH gets a new runner.

## Architecture

No SQS, no Lambda on the CH stack. Use a **CH queue table** + pull-loop
inside `backfill-runner`:

```sql
CREATE TABLE ch_enrichment_queue (
    kind         Enum8('sep1_assets' = 1, 'nft_token_uri' = 2),
    target_key   String,
    enqueued_at  DateTime DEFAULT now(),
    attempt      UInt8 DEFAULT 0,
    last_error   String DEFAULT '',
    next_retry_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(enqueued_at)
ORDER BY (kind, target_key);
```

- **Producer** (`backfill-runner enrichment-enqueue`): scan `assets` /
  `nfts` for NULL `icon_url` / `name` / `media_url`, insert pending
  rows into the queue.
- **Consumer** (`backfill-runner enrichment-loop --kind …`): poll the
  queue, batch N rows, call the fetcher, write via the staging-table
  pattern from task 0228 Stage 1 (idempotent under replay). Mark
  success rows with `attempt = 255` (done sentinel). On failure
  increment `attempt`, push `next_retry_at` by exponential backoff,
  store `last_error`.

Idempotency: `(kind, target_key)` is the `ORDER BY`; RMT dedups on
re-insert, keeping the latest `enqueued_at`. The done sentinel
(`attempt = 255`) excludes rows from re-enqueue via a discovery
WHERE filter.

## Implementation Plan

### Step 1 — CH queue table

Add `crates/db-clickhouse/schema/migrations/NNNN_ch_enrichment_queue.sql`
(+ `init.sql` entry per the project's idempotent-DDL convention).
Pin via column-order test in
[`crates/db-clickhouse/src/persist/tests_cross.rs`](../../../crates/db-clickhouse/src/persist/tests_cross.rs).

### Step 2 — `enrichment` module in `backfill-runner`

`crates/backfill-runner/src/enrichment/{mod.rs, queue.rs, sep1.rs,
nft_token_uri.rs}`. Three CLI subcommands:

```text
backfill-runner enrichment-enqueue
backfill-runner enrichment-loop --kind sep1_assets  --batch-size 100 --max-attempts 10
backfill-runner enrichment-loop --kind nft_token_uri --batch-size 50  --max-attempts 10
```

Reuse `Sep1Fetcher` + `NftTokenUriFetcher` from `enrichment-shared`.
Write back via per-row staging + `INSERT … SELECT` to preserve CH
RMT semantics (no `UPDATE`).

### Step 3 — Integration smoke (task 0212 fixture set)

Live-network `#[ignore]` tests against known-good issuers (USDC,
AQUA) + a known JSON-metadata NFT collection. Mirrors task 0212's
pattern for cross-DB validation.

### Step 4 — Production drain on Hetzner

Run enqueue once, then `enrichment-loop --kind sep1_assets` and
`enrichment-loop --kind nft_token_uri` until queue drains. Sanity:
`SELECT countIf(icon_url IS NULL) / count() FROM assets WHERE
asset_type IN (1, 2)` should drop to < 5% (typical SEP-1 coverage
ceiling).

## Acceptance Criteria

- [ ] CH `ch_enrichment_queue` table created with the schema above
      and column-order pinned in tests.
- [ ] `Sep1Fetcher` + `NftTokenUriFetcher` reused without rewrites;
      CH-side write paths are new code.
- [ ] Three new `backfill-runner` subcommands wired and documented.
- [ ] Replay-idempotent: running `enrichment-loop` twice on the same
      queue does not double-write or stuck-loop.
- [ ] Live integration test (gated `#[ignore]`) verifies USDC TOML
      and a known NFT collection round-trip.
- [ ] Production drain run reported in the task — SEP-1 NULL ratio,
      NFT NULL ratio, queue size before/after, RPC quota usage.
- [ ] **Docs updated** — Architecture overview gains a "Hetzner-side
      enrichment runner" section under
      `docs/architecture/data-pipeline/` analogous to the existing
      PG Lambda enrichment write-up.
- [ ] **API types regenerated** — N/A: this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Notes

- Single-consumer architecture: one CH-enrichment process at a time.
  RPC quota = 1× (vs the rejected 3× pre-FREEZE alternative analysed
  in the 2026-05-18 plan).
- The Lambda+SQS PG path stays untouched — this task adds CH-side
  enrichment, it does not migrate PG off Lambda.
- Future work: shared "CH enrichment runner" abstraction if more
  enrichment kinds appear. Today two kinds is below the abstraction
  threshold; coded duplicated.
