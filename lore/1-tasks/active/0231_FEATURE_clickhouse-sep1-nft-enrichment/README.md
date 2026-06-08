---
id: '0231'
title: 'FEATURE: ClickHouse SEP-1 + NFT token_uri enrichment (AWS Lambda/SQS → CH side tables)'
type: FEATURE
status: active
related_adr: ['0044', '0045', '0047', '0048']
related_tasks: ['0195', '0196', '0212', '0214', '0228', '0243']
blocked_by: []
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
  - date: '2026-06-08'
    status: backlog
    who: karolkow
    note: >
      Plan rewritten after a deep research round (3 simulations on live CH 26.3 +
      3 adversarial reviews — see notes/R-clickhouse-enrichment-write-strategy.md).
      Original premise ("no Lambda/SQS on CH") was a category error and is
      retracted; the CH-queue-table + staging+EXCHANGE approach is superseded by a
      side-table design written over the existing AWS SQS+Lambda path. Title
      updated.
  - date: '2026-06-08'
    status: backlog
    who: karolkow
    note: >
      Blocker satisfied: `blocked_by` 0228 is `completed` (parallel-backfill
      merged + repaired on Hetzner) and 0241 is `completed` (live indexer→CH
      cutover, "mTLS path live"), so prod CH is live and stable. This task is now
      effectively READY to start (no remaining blockers) — code is buildable
      locally and the prod drain has its stable CH. Left in `backlog` until picked
      up on its own branch.
  - date: '2026-06-08'
    status: active
    who: karolkow
    note: >
      Activated (backlog → active) to start implementation on a dedicated branch
      off `feat/0243-...`. Local activation only — the standard promote commit +
      push to develop (board deploy) is deferred (no-commit directive + GitHub
      auth blocked).
---

# FEATURE: ClickHouse SEP-1 + NFT `token_uri` enrichment

## Summary

Fill the off-chain enrichment columns on ClickHouse — `assets.{icon_url, name}`
(SEP-1 issuer TOML) and `nfts.{name, media_url, collection_name}` (`token_uri()`

- IPFS) — which have sat NULL since the CH pilot. This is the CH successor to the
  PG enrichment (PG is being retired, ADR 0047).

> **The how was settled by a deep research round (2026-06-08).** Full problem
> definition (P1–P9), every option + measured decision matrix, simulations, and
> the adversarial panel live in
> [`notes/R-clickhouse-enrichment-write-strategy.md`](notes/R-clickhouse-enrichment-write-strategy.md).
> This README is the **plan**; the note is the **evidence**.

## Status: active — implementation in progress

Unblocked: `blocked_by` 0228 + 0241 are **completed** (parallel-backfill on
Hetzner + live indexer→CH cutover — prod logged `mTLS ClickHouse client ready`),
so prod CH is live; code is built/tested locally against the docker CH. On a
dedicated branch (`feat/0231_clickhouse-sep1-nft-enrichment`). Fetchers
(`Sep1Fetcher`, `NftTokenUriFetcher`) reused verbatim; only the CH write path is
new.

**Done (local; not yet committed/pushed — no-commit directive):**

- **Step 1** — `asset_enrichment` / `nft_enrichment` side tables in `init.sql`
  (validated end-to-end on CH 26.3).
- **Step 2** — storage layer: `AssetEnrichmentRow` / `NftEnrichmentRow`
  (`db-clickhouse/persist/rows.rs`), `insert_*` helpers (`persist/enrichment.rs`),
  column-order pin tests.
- **Step 3** — batch CH drain `enrich --datasource clickhouse {sep1-assets |
nft-metadata | status}` (`backfill-enrichment-runner/src/{ch_sep1,ch_nft}.rs`);
  reuses the shared resolvers; length caps relaxed for CH (`Sep1Caps`/`NftCaps`).

**Next:** Step 4 (read-path join — integrates with task 0243) → Step 6 (live
wiring) → Step 8 (drop `assets.icon_url`). Full checklist below.

## The core problem (one line)

ClickHouse has **no cheap per-column UPDATE**, and a **continuous indexer**
re-writes whole `assets`/`nfts` rows (enrichment columns NULL) **concurrently**
with the enrichment writer — so naively writing enrichment into those tables gets
**clobbered**. (Postgres never hit this: its per-column `UPDATE` + MVCC let the
two writers compose; the clobber is a CH whole-row-replace artefact.)

## Chosen design — side table, written over SQS, read via join

- **Side tables in the same CH database:** `asset_enrichment` / `nft_enrichment`
  = `ReplacingMergeTree(version)`, sitting next to `assets`/`nfts`. The indexer
  **never** writes them → no clobber by construction; the `version` clock makes
  them order-safe; and the enricher can **clear** a value (e.g. issuer removed
  their logo) — none of which the in-place engines can do.
- **Trigger = the existing AWS SQS + Lambda** (NOT a new CH queue). Enrichment
  runs on AWS and writes to CH-on-Hetzner via mTLS, exactly like the indexer
  Lambda already does. The indexer already publishes the per-row SQS messages
  (currently stubbed); `enrichment-worker` is already a Lambda (today on PG).
- **Single owner per value — no column lives in two tables** (the read composes
  disjoint sources in `crates/api`, task 0243):

  | display value                          | single owner                                                  | read                          |
  | -------------------------------------- | ------------------------------------------------------------- | ----------------------------- |
  | asset icon                             | `asset_enrichment.icon_url` (enrichment, off-chain SEP-1)     | `NULLIF(ae.icon_url,'')`      |
  | asset name — classic/SAC (1/2)         | `asset_enrichment.name` (enrichment, SEP-1)                   | in the COALESCE below         |
  | asset name — soroban (3)               | `soroban_contracts.name` (indexer, on-chain `Symbol("name")`) | already-joined `sc.name`      |
  | asset name — native (0)                | constant                                                      | API literal `"Stellar Lumen"` |
  | nft name / media_url / collection_name | `nft_enrichment.*` (enrichment, off-chain `token_uri`)        | `NULLIF(ne.col,'')`           |

  Asset name read = `COALESCE(NULLIF(ae.name,''), sc.name, <native const>)` — the
  three sources are **disjoint by `asset_type`**, so it is one value, never a
  clash. NFT metadata read = `NULLIF(ne.col,'')` **directly** (the indexer never
  had it). A soroban token with no on-chain name (rare) → `name` is NULL → the
  API/FE falls back to the **contract StrKey** (don't fake a name).

  **Ground truth** (code + Stellar protocol research, 2026-06-08): only
  `assets.name` for **soroban** is indexer-derivable from ledger XDR (SEP-41
  `name` lives in a `CONTRACT_DATA` instance-storage entry — readable, **no RPC**)
  and it is already in `soroban_contracts.name`. Classic/SAC names, ALL icons, and
  ALL per-token NFT metadata are **off-chain only** (SEP-1 TOML / `token_uri`
  JSON). So the indexer's `assets.{name,icon_url}` and
  `nfts.{name,media_url,collection_name}` were placeholders (**always `None`**) →
  dropped (step 8).

- **"Tried" marker = row existence; value keeps the `''` sentinel
  (read-neutralised with `NULLIF`).**
  - _Don't re-process:_ a versioned row **existing** for the key = "tried"
    (`… WHERE (key) NOT IN (SELECT key FROM *_enrichment)`; `--force-retry` drops
    it). Existence — not the value — drives skipping.
  - _Value:_ stored as the shared resolver produces it — `real` or the `''`
    sentinel — for **PG parity**; the read uses `NULLIF` to treat `''` as absent.
  - `--force-retry` re-fetches and reflects the current source, including
    **clearing** a removed value.

## What changed vs the original plan (after research)

| #   | Original plan (2026-05-18)                                                  | After research (2026-06-08)                                                                                                                                                                                                                                                                                                                                                                                |
| --- | --------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | "No SQS/Lambda on the CH stack"                                             | **False** — SQS+Lambda are on **AWS**; they write to CH-Hetzner via mTLS like the indexer. Live IS buildable.                                                                                                                                                                                                                                                                                              |
| 2   | New `ch_enrichment_queue` CH table + pull-loop (attempt counters, backoff)  | **Dropped** — reuse the existing AWS **SQS** (queue+retry+DLQ already there) + the enrichment-worker Lambda.                                                                                                                                                                                                                                                                                               |
| 3   | Write via **staging-table + `EXCHANGE TABLES`** (reuse 0228 repair pattern) | **Rejected** — measured to lose rows under live concurrency (5000/10000); correct only in a frozen/batch window.                                                                                                                                                                                                                                                                                           |
| 4   | (implied) write enrichment **into** `assets`/`nfts`                         | **Rejected for in-place** — indexer whole-row writes clobber it; CMT / AggregatingMergeTree are block-order-unsafe + can't-clear. → **separate side tables**.                                                                                                                                                                                                                                              |
| 5   | Enrichment columns already on `assets`/`nfts`                               | **Single owner per value.** The indexer's `assets.{name,icon_url}` + `nfts.{name,media_url,collection_name}` are **always-`None` placeholders** (off-chain data it can't derive) → dropped (step 8). Soroban token name (the one on-chain name) already lives in `soroban_contracts.name`; native name is an API constant; everything else is enrichment-only in the side tables. No column in two tables. |
| 6   | Add to `init.sql`                                                           | **Migration** (prod CH is live) + `init.sql` mirror.                                                                                                                                                                                                                                                                                                                                                       |
| 7   | "Does not touch `crates/api`"                                               | **Touches the read path** — `assets/queries_ch.rs` `ASSET_CH_SELECT` gains the enrichment join (task 0243 integration).                                                                                                                                                                                                                                                                                    |
| 8   | Three `backfill-runner enrichment-*` subcommands as the primary path        | Primary = the **live SQS Lambda**; the batch CLI (`backfill-enrichment-runner` CH mode) is the secondary/operator path.                                                                                                                                                                                                                                                                                    |

## Implementation steps (in order)

1. ✅ **DONE — Schema migration.** `asset_enrichment` / `nft_enrichment`
   `ReplacingMergeTree(version)` added to `init.sql` (idempotent — re-applying on
   live prod creates only the new tables; for NEW tables init.sql IS the
   migration here, there is no migration runner). Validated end-to-end on CH 26.3.
2. ✅ **DONE — Write storage layer.** `AssetEnrichmentRow` / `NftEnrichmentRow`
   (`persist/rows.rs`) + `insert_asset_enrichment` / `insert_nft_enrichment`
   (`persist/enrichment.rs`, insert-only, `version` = ms) + column-order pin tests
   (`tests_cross.rs`, 21 green). `cargo build -p db-clickhouse` clean.
3. ✅ **DONE (sep1) — Write path (batch).** `backfill-enrichment-runner
--datasource clickhouse sep1-assets` (`src/ch_sep1.rs`). Candidate =
   `assets a FINAL LEFT JOIN accounts iss … WHERE asset_type IN (1,2) AND (key)
NOT IN (SELECT key FROM asset_enrichment)` (`--force-retry` drops the NOT IN),
   keyset-paginated over the 4-tuple. Per row: fetch (shared `Sep1Fetcher`),
   `resolve_currency_outcome` / `permanent_fail_outcome` (shared resolver),
   store the outcome **as-is** (incl. the `''` sentinel), `version = now_ms`,
   `insert_asset_enrichment`. Verified on live CH (candidate → permanent-fail →
   `''` sentinel row written; row-existence skips re-processing). **Self-contained
   CLI — no Lambda/SQS.** `nft-metadata` (`src/ch_nft.rs`, reuses
   `nft_token_uri::extract_columns`) and CH `status` (coverage counts) also wired
   - verified. Remaining: concurrency (currently sequential per-row) — optional
     optimisation.
4. **Read path (task 0243 integration — AFTER the enrichment scope lands).**
   `ASSET_CH_SELECT` (already joins `soroban_contracts sc`): asset **icon** =
   `NULLIF(ae.icon_url,'')`; asset **name** =
   `COALESCE(NULLIF(ae.name,''), sc.name, <native const>)` (disjoint by
   asset_type: enrichment classic/SAC · `soroban_contracts` soroban · API const
   native; falls back to contract StrKey when all NULL). NFT read = `nft_enrichment`
   joined, `NULLIF(ne.col,'')` **directly** (no COALESCE — indexer never had NFT
   metadata). api-types unchanged.
5. **Integration smoke** — `#[ignore]` tests via the batch path (USDC TOML + a
   known NFT collection); verify sentinel + priority + clear-on-refresh.
6. **Wire the LIVE path** (separate step — do **before going live**; NOT needed
   for the batch path / tests). Un-stub the indexer SQS publish
   (`indexer/src/handler/enrichment_publish.rs`); repoint the `enrichment-worker`
   Lambda PG → CH (build the mTLS client like `crates/indexer/src/main.rs`, call
   the `insert_*` helpers with `version = now_ms` instead of the PG `UPDATE`;
   reuse the fetchers; keep sentinel + priority).
7. **Production drain** (prod CH ready — 0228/0241 done) — run live and/or batch
   until coverage stabilises; report SEP-1/NFT NULL ratios + RPC quota;
   **measure the read-join cost on the 1M+ `nfts`** under the read quota
   (dictionary / refreshable MV if it bites — research note §6/§8).
8. **Cleanup — drop the dead / redundant indexer columns** (gated; LAST step).
   Two reasons per the provenance audit:

   - **Always-`None` placeholders:** `assets.icon_url`, `nfts.{name, media_url,
collection_name}` — off-chain data the indexer can never derive.
   - **Redundant:** `assets.name` — its only real values are the **soroban** name
     (already in `soroban_contracts.name`) and the **native** constant (moves to an
     API literal); classic/SAC are enrichment. So it duplicates data sourced
     elsewhere.

   Once the read path (step 4) is live and **nothing reads them**, drop all five
   via operator runbook `ALTER TABLE … DROP COLUMN` (manual, like
   `docs/runbooks/0217_*`; heavy ALTER on the live 1M+ `nfts` → low-traffic
   window). **This also requires an indexer-crate change** — remove `name` +
   `icon_url` from `AssetRow` and `name`/`media_url`/`collection_name` from
   `NftRow` (+ `stage.rs`), so the indexer stops emitting them. The soroban name
   keeps flowing to `soroban_contracts.name` (KEEP — the one on-chain name, which
   the asset read sources). Best split to its own follow-up task (touches the
   indexer write path, not just enrichment).

## Acceptance Criteria

- [ ] `asset_enrichment` / `nft_enrichment` created via **migration** (+ init.sql
      mirror); column order pinned in tests.
- [ ] `ch_enrichment_queue` **NOT** created — SQS is the queue.
- [ ] Fetchers reused verbatim; only the CH write path (side-table INSERT) is new.
- [ ] Live write path wired (indexer SQS publish un-stubbed + worker repointed
      PG→CH) and/or the batch CH mode.
- [ ] Read: asset name = `COALESCE(NULLIF(ae.name,''), sc.name, native-const)`
      (enrichment / `soroban_contracts` / API const, disjoint by asset_type;
      contract-StrKey fallback); asset icon = `NULLIF(ae.icon_url,'')`; nft meta =
      `NULLIF(ne.col,'')` directly (no COALESCE — indexer never had it).
- [ ] Replay-idempotent; sentinel breaks the retry loop; `--force-retry`
      re-enriches and can clear removed values.
- [ ] Live integration test passes (USDC + NFT round-trip).
- [ ] Production drain reported (NULL ratios, RPC quota, measured `nfts`
      read-join cost).
- [ ] **Docs updated** per ADR 0032 (enrichment write-up + the 0243 read-path
      change). **API types** regenerated as a sanity check (shape unchanged).
- [ ] (gated, LAST) Dead placeholder columns dropped — `assets.{name,icon_url}` +
      `nfts.{name,media_url,collection_name}` (indexer always `None`) — via
      operator ALTER + indexer stops emitting them, **after** the read path is
      live. `soroban_contracts.name` retained. May be a follow-up task.

## Notes

- RPC quota = 1× (single live consumer; vs the rejected 3× pre-FREEZE
  alternative in the 2026-05-18 plan).
- This **repoints** the existing PG enrichment Lambda to CH; it is the CH
  successor, not a parallel addition (PG retiring per ADR 0047).
- The superseded `ch_enrichment_queue` + `staging+EXCHANGE` plan is preserved in
  git history; reasoning for dropping it is in the research note.
- Live vs batch ordering (which to ship first) is deferred to implementation —
  both write the same side table, so the schema is unaffected.
- **Sentinel `''` kept** (decision, karolkow): the side table stores the
  resolver's `''` "tried-nothing" marker (PG parity); the read neutralises it
  with `NULLIF`. Row-existence (`NOT IN`) drives the don't-re-process candidate
  filter, independent of the value.
- **Length caps relaxed for CH** (decision, karolkow): the shared resolvers are
  parameterised — `Sep1Caps::PG`/`NftCaps::PG` keep the VARCHAR widths (256/1024)
  for the live PG worker; `…::CH` are generous (name/collection ~4096, URL ~8192)
  so a long-but-valid value is stored on CH's unbounded `String`, not sentinel'd.
  `https://`-only + `javascript:`/`data:` rejection (XSS/mixed-content) is kept
  for both.
