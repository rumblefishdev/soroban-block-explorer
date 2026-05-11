---
id: '0196'
title: 'Enrichment backfill: new crate that drains pre-existing un-enriched DB rows for every kind'
type: FEATURE
status: completed
related_adr: ['0007', '0032', '0043']
related_tasks: ['0188', '0191', '0194', '0195', '0197']
tags: [priority-medium, effort-medium, layer-cli, layer-enrichment]
milestone: 2
links: []
history:
  - date: '2026-05-06'
    status: backlog
    who: karolkow
    note: Spawned from M2 enrichment planning. Karol override of 0191 Future-Work bullet #1 — backfill must be a separate crate, not a `backfill-runner` subcommand.
  - date: '2026-05-11'
    status: active
    who: karolkow
    note: Promoted from backlog. Branch cut from feat/0195 tip; reuses `enrich_asset_from_sep1` (0195 §2a) and `enrich_nft_token_uri` scaffold (0195 §2d, fetcher `unimplemented!()` until Phase E).
  - date: '2026-05-11'
    status: completed
    who: karolkow
    note: Shipped single-bin `enrich` (`crates/backfill-enrichment-runner`) with `icon` / `nft-metadata` / `status` subcommands; 12 pure-helper unit tests; real smoke against Circle USDC verified end-to-end. ADR 0043 amended with the type-1 drain path. Integration tests and 50K benchmark folded into 0197's post-merge verification gate.
---

# Enrichment backfill: new crate that drains pre-existing un-enriched DB rows for every kind

## Summary

`crates/backfill-enrichment-runner` — single binary `enrich` with
`icon` / `nft-metadata` / `status` subcommands. Drains DB rows the
live SQS-driven worker never saw (population pre-dating the
queue's deployment) by calling the same
`enrichment_shared::enrich_and_persist::*` functions the live
worker uses. Drain and live path share one implementation; no SQS
in the drain.

## What was built

- **Crate:** `crates/backfill-enrichment-runner`, single
  `[[bin]] name = "enrich"`, ~670 LoC `src/main.rs` (incl. 12
  unit tests). Layout mirrors `backfill-runner`.
- **Drain mechanism per subcommand:** chunked cursor SELECT
  (`WHERE <kind predicate> AND id > $last ORDER BY id LIMIT N`)
  → `tokio::spawn` fan-out bounded by
  `Arc<Semaphore::new(concurrency)>` → call the matching
  `enrich_*` function per row → tally into `BackfillReport`.
- **Flags:** `--concurrency` (default 10), `--chunk-size`
  (default 200), `--limit`, `--id` (surgical single-row),
  `--force-retry` (γ-overwrite: drop the NULL filter).
- **Pool size:** `max_connections = concurrency + 2` for drain
  subcommands, `2` for `Status` — sized per-subcommand at the
  `match cli.command` boundary.
- **Failure handling:** spawn panics caught in `collect_join`
  and tallied as `db_failed` so the drain survives bad rows
  (matters because `NftTokenUriFetcher::resolve` is
  `unimplemented!()` until 0195 Phase E).
- **Exit code:** `0` clean, `1` on any transient or db_failed —
  operator-chainable.
- **Smoke:** Circle USDC issuer (centre.io) against Docker pg
  populated real `icon_url` + `name` end-to-end.

## Why

- **Why a new crate, not a `backfill-runner` subcommand:** ledger
  backfill and enrichment backfill have different data sources
  (S3 XDR vs DB rows + HTTP), different concurrency models, and
  different operational profiles. 0191 design decision #8 was
  emphatic that `backfill-runner` must not be modified;
  separate crate keeps that guarantee.
- **Why no SQS path in the drain:** a 50K-row publish would hit
  SQS rate limits, and per-message visibility-timeout overhead
  wastes time when we already hold a DB connection.
- **Why γ-semantics for `--force-retry`:** `enrich_*` functions
  are already idempotent under
  `COALESCE(NULLIF($n, ''), col, $n)`. Clear-step alternatives
  (α: clear sentinels first; β: NULL the column first) either
  fail to catch real → sentinel re-classification or open a
  NULL-flicker window across the entire table.

## Acceptance Criteria

- [x] Crate builds, lints, integrated into workspace.
- [x] `icon` + `nft-metadata` + `status` subcommands wired.
  `lp-tvl` not in scope (owned by 0199, blocked on price oracle).
- [x] `--force-retry` γ-semantics, `--id N` surgical mode,
  `--limit N` cap.
- [x] `status` subcommand prints per-kind NULL / sentinel counts.
- [x] Pure-helper unit tests (12, all green).
- [x] Manual smoke against Circle USDC.
- [x] README runbook + ops checklist.
- [x] ADR 0043 amended with the type-1 drain path; 0191
  Future-Work bullet #1 marked obsolete in the 0191 archive.
- [ ] Integration tests per subcommand and 50K real-world
  `enrich icon` benchmark — **folded into 0197** (post-merge
  verification gate; no separate tasks).

## Design Decisions (Emerged)

1. **Pool size scaled to concurrency, not hardcoded.** Initial
   draft was `max_connections(4)` while concurrency=10, which
   throttled effective fan-out by 60%. Caught in the code-review
   pass (simplify skill).

## Future Work (out of scope)

Folded into **0197** — the post-merge verification gate for the
0194-0197 chain. 0197 absorbs:

- 50K real-world `enrich icon` benchmark on staging (confirms or
  amends the README "< 30 min" target with measured p50 / p95 /
  total wall clock).
