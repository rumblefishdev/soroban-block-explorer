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
    note: Spawned from M2 enrichment planning. Karol overrode 0191 Future-Work bullet #1 — backfill must be a separate crate, not a `backfill-runner` subcommand.
  - date: '2026-05-11'
    status: active
    who: karolkow
    note: Promoted from backlog. Branch cut from feat/0195 tip; reuses `enrich_asset_from_sep1` (0195 §2a) and `enrich_nft_token_uri` scaffold (0195 §2d, fetcher `unimplemented!()` until Phase E).
  - date: '2026-05-11'
    status: completed
    who: karolkow
    note: Shipped single-bin `enrich` (`crates/backfill-enrichment-runner`) with `icon` / `nft-metadata` / `status` subcommands; 12 pure-helper unit tests; real smoke against Circle USDC verified end-to-end. ADR 0043 amended with the type-1 drain path. Integration tests and 50K benchmark folded into 0197's post-merge verification gate.
  - date: '2026-05-11'
    status: completed
    who: karolkow
    note: >
      Post-completion review pass — three follow-up commits (8503ae9
      code-review fixes, 319be94 `Icon` → `Sep1Assets` rename, 48d10a5
      CodeRabbit pass). CLI subcommand renamed from `icon` to
      `sep1-assets`; SQS wire-format `kind` string still `"icon"` via
      `#[serde(rename = "icon")]` so in-flight messages + DLQ replays
      stay bit-compatible. `trimmed_string` split into `_chars`
      (VARCHAR(256) columns) and `_bytes` (TEXT media_url). IPFS
      `validate_uri` now percent-decodes `%2e` before path-traversal
      check (mixed-encoding bypass). Drained spawn handles on `--limit`
      break. Pool size scaled to `concurrency + 2` (review fix
      commit 8503ae9 had claimed-but-not-done). Stale Phase E
      `unimplemented!()` doc refs across worker / backfill / shared
      purged. Tests grown 12 → 14 (multibyte + bytes-cap). 65 tests
      enrichment-shared (+8 mixed-encoding + permanent-variant
      coverage). Workspace clippy `-D warnings` clean.
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

2. **CLI subcommand renamed `icon` → `sep1-assets`; SQS wire kept
   `"icon"`.** Post-completion Karol flag: `Icon` enrichment kind
   has written both `assets.icon_url` AND `assets.name` since 0195
   §2a — the name implied a narrower scope than reality. Renamed
   Rust identifiers (`EnrichmentMessage::Sep1Assets`, backfill
   `Kind::Sep1Assets`, CLI subcommand `sep1-assets`). SQS wire-
   format `kind` string preserved as `"icon"` via
   `#[serde(rename = "icon")]` so in-flight messages, DLQ replays,
   and the indexer publisher stay bit-compatible. Backfill `Kind`
   label string moved to `"sep1_assets"` for report headers.
   Operators using prior `enrich icon` invocation must switch to
   `enrich sep1-assets` (no alias kept).

3. **`trimmed_string` split into `_chars` (VARCHAR) and `_bytes`
   (TEXT).** Original single helper used `trimmed.len()` (byte
   count) for all columns. Postgres `VARCHAR(N)` caps character
   count, not bytes — a 200-character emoji string (800 bytes)
   would have been rejected by our byte check but accepted by the
   schema, and vice-versa. Split into `trimmed_string_chars`
   (`name` + `collection_name`, measured via `chars().count()`)
   and `trimmed_string_bytes` (`media_url`, byte cap because the
   column is TEXT and the limit is a body-size safeguard, not a
   schema constraint). Multibyte boundary test added.

4. **IPFS path-traversal validator percent-decodes `%2e` before
   segment split.** Original check only matched fully-encoded
   `%2e%2e`. Mixed encodings (`.%2e`, `%2e.`, single `%2e/`,
   uppercase `%2E%2E`) slipped through because the literal-dot
   segment match in `split('/').any(...)` never saw the encoded
   form. Fix: case-insensitive `%2e` → `.` replacement before
   the split. Four new test cases.

5. **Drained spawn handles on `--limit` break.** Inner await-loop
   used to `break` on `limit_reached`, leaking background DB
   writes whose results never reached the report. `effective_chunk`
   already caps each chunk at `cap - processed`, so the outer
   top-of-loop check terminates the drain cleanly without needing
   the inner break.

6. **`NftTokenUriError::Http` URL-leak refactor proposed and
   reverted.** CodeRabbit flagged that `reqwest::Error::Display`
   includes the full URL. Drafted `HttpFailureKind` enum + 7 new
   unit tests + `http_error()` redaction helper (~130 lines). On
   review with Karol: our HTTP call sites target the public SDF
   Soroban RPC and public IPFS gateway only — no query-string
   secrets exist on the deployed topology. The refactor mitigated
   a theoretical risk that does not match reality. Reverted; the
   3-line 408/429 retry-classification follow-on deferred until
   real upstreams surface those codes.

7. **Phase E stale-doc cleanup.** Merge from feat/0195 brought
   docs referencing `NftTokenUriFetcher::resolve()` as
   `unimplemented!()` even though Phase E (commit `af7e271` on
   feat/0195) shipped the real Soroban-RPC + IPFS fetcher. Purged
   "STUB STATUS" / "Hard-fail chokepoint" / `0195 Phase E gating`
   prose from `enrichment-shared`, `enrichment-worker`,
   `backfill-enrichment-runner`, `indexer/enrichment_publish.rs`,
   and `docs/architecture/indexing-pipeline/enrichment.md`. Also
   removed the dead `NftTokenUriError::NotImplemented` variant
   and the matching `is_transient` comment.

## Issues Encountered

- **Linter reverted my 408/429 `is_transient` patch twice.**
  After the option-2 minimal revert of the Http refactor, my
  3-line 408/429 amendment in `is_transient` was rolled back by
  the workspace linter / formatter to the pre-existing
  5xx-only classifier. Decision (with Karol): accept the linter
  state; deferred 408/429 fix until a real upstream returns
  those codes. Reverting the linter would need a separate
  policy discussion.

- **Stale `0196` archive doc references.** Task spec in the
  archive snapshots `12 unit tests` + subcommand named `icon`.
  Counts grew to 14 after multibyte / bytes-cap tests, and the
  subcommand renamed to `sep1-assets`. Archive task spec is a
  historical record — not retroactively modified. Live README +
  `enrichment.md` + `enrichment_publish.rs` module header are
  the up-to-date sources of truth.

- **Pool size claimed-but-not-fixed in commit 8503ae9.** The
  review-fix commit message announced "Pool size scaled to
  concurrency" but the diff still had `max_connections(4)`.
  Caught + actually fixed in 319be94.

## Future Work (out of scope)

Folded into **0197** — the post-merge verification gate for the
0194-0197 chain. 0197 absorbs:

- 50K real-world `enrich icon` benchmark on staging (confirms or
  amends the README "< 30 min" target with measured p50 / p95 /
  total wall clock).
