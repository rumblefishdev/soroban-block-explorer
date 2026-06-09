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
  - date: '2026-06-09'
    status: active
    who: karolkow
    note: >
      Step 6 producer lookup un-stubbed (the headline remaining piece). The
      indexer producer now runs a per-batch CH anti-join `(key) NOT IN (SELECT
      key FROM *_enrichment)` and publishes only the misses — assets + NFTs,
      fail-open, worker unchanged. Mechanism: four/two parallel typed arrays
      zipped CH-side via `arrayZip` (new `enrich_and_persist::filter`),
      sidestepping the clickhouse-0.15 None-in-tuple bind defect + asset_code
      injection; verified on a live local CH via `#[ignore]` anti-join tests.
      Also landed: Step 4a asset read-join, Step 5 sep1 real-fetch smoke,
      shared `is_safe_https_url` + `send_one_batch` refactors, stale-comment
      cleanup. 4 commits (aee77ae2, 5d8280d3, 139b549c, 2d04fa10) local on the
      branch; enrichment-shared 63 + indexer 20 green, clippy+fmt clean. Open:
      worker/indexer deploy (CDK env + GitHub auth blocked), prod drain (7),
      NFT read-join (4b — needs nfts→CH), full smoke (NFT RPC + clear-on-
      refresh), columns drop (8), async_insert (9).
---

# FEATURE: ClickHouse SEP-1 + NFT `token_uri` enrichment

## Summary

Populate the off-chain enrichment values on ClickHouse — asset **icon / name**
(SEP-1 issuer TOML) and NFT **name / media_url / collection_name** (`token_uri()`

- IPFS). Written into dedicated **side tables** (`asset_enrichment` /
  `nft_enrichment`, ADR 0048) — never the indexer-owned `assets`/`nfts` — and
  read-composed by the API (Option C: enrichment owns the off-chain values;
  on-chain soroban names come from `soroban_contracts`). The CH successor to the PG
  enrichment (PG retiring, ADR 0047).

> **The how was settled by a deep research round (2026-06-08).** Full problem
> definition (P1–P9), every option + measured decision matrix, simulations, and
> the adversarial panel live in
> [`notes/R-clickhouse-enrichment-write-strategy.md`](notes/R-clickhouse-enrichment-write-strategy.md).
> This README is the **plan**; the note is the **evidence**.

## Status: active — write path code-complete; deploy + prod drain remain

Unblocked: `blocked_by` 0228 + 0241 are **completed** (parallel-backfill on
Hetzner + live indexer→CH cutover — prod logged `mTLS ClickHouse client ready`),
so prod CH is live; code is built/tested locally against the docker CH. On a
dedicated branch (`feat/0231_clickhouse-sep1-nft-enrichment`). Fetchers
(`Sep1Fetcher`, `NftTokenUriFetcher`) reused verbatim; only the CH write path is
new.

**Approach decided (B): the whole write path was converted PG → CH _in place_,
not added alongside.** Postgres is being retired (ADR 0047), so there is a single
CH variant — `enrich_*` writes the side tables, the worker + the batch runner both
call it. No `ch_*`-suffixed parallel modules, no datasource flag. (This supersedes
an earlier draft that kept a separate `ch_sep1`/`ch_nft` drain alongside PG.)

**Done (committed locally on `feat/0231_clickhouse-sep1-nft-enrichment`; not
pushed — GitHub auth blocked):**

- **Step 1** — `asset_enrichment` / `nft_enrichment` side tables in `init.sql`
  (validated end-to-end on CH 26.3).
- **Step 2** — storage layer: `AssetEnrichmentRow` / `NftEnrichmentRow`
  (`db-clickhouse/persist/rows.rs`), `insert_*` helpers (`persist/enrichment.rs`),
  column-order pin tests.
- **Step 3** — write path PG → CH in place (see step 3 below): `enrich_*` now
  take a `clickhouse::Client` + the composite key and INSERT the side tables;
  caps loosened inline for CH; shared `EnrichmentMessage` wire type.
- **Step 6 — DONE (code).** Live wiring: `enrichment-worker` repointed to the
  mTLS CH client + shared composite wire; the indexer producer's lookup is now
  **un-stubbed** — a per-batch CH anti-join publishes only un-enriched keys
  (assets + NFTs, fail-open, worker unchanged). New
  `enrich_and_persist::filter`. Remaining for Step 6 = deploy config (CDK env)
  - the deploy itself.
- **Step 4a + Step 5 (sep1) — DONE.** Asset read-join (`ASSET_CH_SELECT`
  enrichment join, commit 17916d8a) + sep1 real-fetch smoke (`#[ignore]`).
- **Batch runner** — rewritten CH-only (`enrich {sep1-assets | nft-metadata |
status}`), keyset candidate stream + semaphore fan-out calling `enrich_*`;
  functionally exercised on a live local CH.

**Next:** deploy (indexer + worker — blocked on GitHub auth + the worker's CDK
env) → Step 7 (prod drain) → Step 4b (NFT read-join — needs the nfts API module
on CH) → full smoke (NFT RPC round-trip + clear-on-refresh) → Step 8 (drop dead
columns; follow-up) → Step 9 (`async_insert`). Full checklist below.

## The core problem (one line)

ClickHouse has **no cheap per-column UPDATE**, and a **continuous indexer**
re-writes whole `assets`/`nfts` rows (enrichment columns NULL) **concurrently**
with the enrichment writer — so naively writing enrichment into those tables gets
**clobbered**. (Postgres never hit this: its per-column `UPDATE` + MVCC let the
two writers compose; the clobber is a CH whole-row-replace artefact.)

## Chosen design — side table, written over SQS, read via join

- **Side tables in the same CH database:** `asset_enrichment` / `nft_enrichment`
  = `ReplacingMergeTree(version)`, sitting next to `assets`/`nfts`. The indexer
  **never** writes them → no clobber by construction. Writes are plain INSERTs
  with `version = now_ms`; RMT keeps the latest write per key (**latest-wins**).
  _(Whether a later sentinel may overwrite a real value — auto-clear vs sticky —
  is a deferred design decision; see Notes. The shipped model is the simplest
  latest-wins.)_
- **Trigger = the existing AWS SQS + Lambda** (NOT a new CH queue). Enrichment
  runs on AWS and writes to CH-on-Hetzner via mTLS, exactly like the indexer
  Lambda. The indexer publishes **composite-key** SQS messages (the _which-keys_
  lookup is currently stubbed); `enrichment-worker` is **repointed PG → CH**
  (task 0231).
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
| 8   | Three `backfill-runner enrichment-*` subcommands as the primary path        | Primary = the **live SQS Lambda**; the CH-only batch CLI (`backfill-enrichment-runner`) is the secondary/operator path.                                                                                                                                                                                                                                                                                    |

## Implementation steps (in order)

1. ✅ **DONE — Schema migration.** `asset_enrichment` / `nft_enrichment`
   `ReplacingMergeTree(version)` added to `init.sql` (idempotent — re-applying on
   live prod creates only the new tables; for NEW tables init.sql IS the
   migration here, there is no migration runner). Validated end-to-end on CH 26.3.
2. ✅ **DONE — Write storage layer.** `AssetEnrichmentRow` / `NftEnrichmentRow`
   (`persist/rows.rs`) + `insert_asset_enrichment` / `insert_nft_enrichment`
   (`persist/enrichment.rs`, insert-only, `version` = ms) + column-order pin tests
   (`tests_cross.rs`, 21 green). `cargo build -p db-clickhouse` clean.
3. ✅ **DONE — Write path PG → CH, _in place_.** The shared
   `enrich_asset_from_sep1` / `enrich_nft_token_uri` (`enrichment-shared`) were
   edited in place — Postgres is retired (ADR 0047), so no PG variant survives and
   there are no `ch_*`-suffixed modules. They now take a `clickhouse::Client` + the
   composite key (no CH surrogate `id`), look up the issuer `home_domain` /
   contract StrKey on CH, run the shared fetcher + resolver, and INSERT the side
   table via `insert_*` (`version = now_ms`), storing the outcome **as-is** (incl.
   the `''` sentinel). `EnrichError::Database` wraps `clickhouse::error`. Both
   callers go through these:

   - **Batch runner** (`backfill-enrichment-runner`, CH-only): `enrich
{sep1-assets | nft-metadata | status}`; candidate = `(key) NOT IN (SELECT key
FROM *_enrichment)` (`--force-retry` drops it), keyset-paginated over the key
     tuple, `Semaphore` fan-out calling `enrich_*`. The old `ch_sep1.rs`/`ch_nft.rs`
     parallel drains + the `--datasource` flag are gone. Verified on a live local
     CH (`status` + `sep1-assets`).
   - **Live worker** — same `enrich_*`, see step 6.

   Caps were loosened inline for CH (`Nullable(String)` is unbounded), and the
   shared `EnrichmentMessage` (composite key) is the producer↔worker wire contract.

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
6. **Wire the LIVE path.** ✅ **DONE (code):** `enrichment-worker` repointed PG →
   CH — builds the mTLS client via `db_clickhouse::mtls::client_from_lambda_env`
   (same as `crates/indexer/src/main.rs`), decodes the shared composite
   `EnrichmentMessage`, calls the converted `enrich_*` (which INSERT the side
   tables). The indexer producer's SQS wire helpers
   (`indexer/src/handler/enrichment_publish.rs`) now carry the composite key.
   ✅ **DONE (2026-06-09):** producer _lookup_ un-stubbed.
   `Publisher::publish_for_{extracted_assets,minted_nfts}` derive the batch keys
   (mirroring `persist::stage` identity), run the per-batch anti-join `(key) NOT
IN (SELECT key FROM *_enrichment)` via `arrayZip` of parallel typed arrays
   (new `enrich_and_persist::filter`), and publish only the misses — **fail-open**
   (a CH error publishes all; the idempotent RMT worker absorbs the over-fetch).
   Assets pass-all (no first-seen marker); NFTs **mint-gated** at the call site
   (`token_uri` is set at mint, immutable on transfer; backfill covers non-mint
   gaps). Verified on a live local CH via `#[ignore]` anti-join tests.
   **Remaining = deploy only:** the worker Lambda needs `MTLS_SECRET_NAME` /
   `CH_DOMAIN` env (CDK) + the indexer/worker deploy (GitHub auth blocked).
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

9. **Perf — enable `async_insert` on the enrichment clients** (independent
   optimisation; NOT yet done). `enrich_*` does one INSERT per key (per-SQS-message
   model), so the live worker + the batch runner emit many tiny inserts → many
   small CH parts → merge pressure (and, at the extreme, "too many parts"). Set on
   both clients (worker mTLS client + runner client):

   ```rust
   client
       .with_option("async_insert", "1")
       .with_option("wait_for_async_insert", "1")
   ```

   CH then buffers the small inserts server-side and flushes them as one larger
   part. `wait_for_async_insert=1` is **required** for the worker: the SQS ack must
   happen only after a durable write (with `=0`, ack + buffer-loss-on-crash =
   dropped enrichment). Only affects INSERTs; the candidate SELECTs are untouched.

## Acceptance Criteria

- [ ] `asset_enrichment` / `nft_enrichment` created via **migration** (+ init.sql
      mirror); column order pinned in tests.
- [x] `ch_enrichment_queue` **NOT** created — SQS is the queue.
- [x] Fetchers reused verbatim; only the CH write path (side-table INSERT) is new.
- [x] Live write path wired (code): worker repointed PG→CH; producer lookup
      **un-stubbed** — per-batch CH anti-join publishes only un-enriched keys
      (assets + NFTs, fail-open). Batch CLI (CH-only) done. _(Deploy config/CDK +
      the deploy itself still pending — GitHub auth blocked.)_
- [~] Read: asset name/icon join **done** in `ASSET_CH_SELECT` (Step 4a —
  `COALESCE(NULLIF(ae.name,''), sc.name, native-const)` +
  `NULLIF(ae.icon_url,'')`). NFT meta read (`NULLIF(ne.col,'')` direct)
  **pending** — the nfts API module is still PG-only (Step 4b).
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
- **Length caps relaxed for CH** (decision, karolkow): the shared resolvers'
  length caps were loosened **inline** (name/collection 4096, URL/media 8192) —
  CH columns are unbounded `Nullable(String)`, so a long-but-valid value is
  stored, not sentinel'd. (An earlier `Sep1Caps`/`NftCaps` PG/CH preset split was
  reverted — PG is retiring, a single generous cap suffices.) `https://`-only +
  `javascript:`/`data:` rejection (XSS/mixed-content) kept.
- **Write-conflict model = simplest "latest-wins", whole-row atomic** (decision,
  karolkow). The live worker always enriches + INSERTs on every message (no
  pre-check); each fetch is atomic (one source → all columns get real/`''`; a
  transient/timeout fails the WHOLE fetch → no row → retry); `version = now_ms`,
  newest write wins. **Analysed but DEFERRED to future** (kept simple for now):
  - _Sentinel/empty re-fetch can wipe a real value_ on `--force-retry` (a
    transient-disguised-as-permanent 404). Fixes parked: per-column
    last-non-NULL engine (CoalescingMergeTree) / read-time `argMax` / a
    version-floor trick. Refresh is rare today → low risk.
  - _Per-field independent outcomes_ (real + sentinel + NULL-pending from
    different sources) would need `''` (done-empty) vs `NULL` (pending-retry) +
    per-field retry, OR a separate table per source. Not needed while all fields
    come from one atomic fetch.
  - _Worker has no pre-check + producer lookup stubbed + SQS at-least-once_ →
    redundant third-party fetches possible; add a refresh-intent flag to the
    message before going live at scale.
  - _Sentinels never auto-retry_ (`NOT IN` skips them forever) → a momentarily-
    broken / late-published source stays empty; a sentinel-TTL re-enroll fixes it.
  - _`now_ms().unwrap_or(0)` + `DateTime64(3)` version_ — fine under latest-wins;
    revisit only if a version-priority scheme is ever adopted.

## Future Work (spawn as backlog tasks **on develop**, per project convention)

Surfaced during the 2026-06-09 Step 6 producer un-stub — small, deferred to
keep this branch focused (not micro-decomposed here):

- **Shared anti-join predicate.** `(key) NOT IN (SELECT key FROM *_enrichment)`
  now exists twice — the producer `filter` and the backfill `select_*_chunk`
  (identical subquery, different candidate source: in-memory batch keys via
  `arrayZip` vs a keyset-paginated `assets`/`nfts FINAL` scan). Hoist the
  subquery fragment to one shared `const` so a future column change can't drift
  them apart.
- **Shared key derivation.** `AssetKey`/`NftKey` from `ExtractedAsset`/`Nft` is
  duplicated in `persist::stage` (the table row) and the producer
  `*_candidate_keys` (the key); they MUST stay byte-identical (incl. the
  `unwrap_or(0)` / `unwrap_or_default()` "absent" sentinels) or the anti-join
  silently never matches. Extract one `AssetKey::from_extracted` constructor.
- **CH-mode end-to-end parity smoke.** No test exercises the full
  SQS→worker→side-table→read round-trip; the `#[ignore]` filter + sep1-fetch
  tests cover the pieces, not the wire. (Carries over the pre-existing
  "CH-mode parity tests" gap.)
