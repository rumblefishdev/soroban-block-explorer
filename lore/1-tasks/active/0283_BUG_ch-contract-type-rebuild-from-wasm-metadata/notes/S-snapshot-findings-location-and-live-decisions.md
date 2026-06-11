# S — Snapshot findings, crate location, and live-gap decision

> Synthesis note, 2026-06-11, karolkow (working session with Claude).
> Status: mature. Feeds Step 0 (empirical sizing), Step 1 (where the tool
> lives), Step 4 (live-gap strategy), and a new Step 6 (Bachini i128 gap).
> Parent: task 0283.

## Investigation setup

The full prod CH is on Hetzner (mTLS-gated). For fast iteration we instead
restored the **small tables** from the local ClickHouse native backup
`~/snapshots/snapshot_b_post_0252_20260526` (690 GB, taken 2026-05-26, holds
the post-Phase-6 merged backfill, ledgers 50,457,424–62,527,999) into a
throwaway container:

- `docker` container `ch-snap` = `clickhouse/clickhouse-server:26.3` with the
  backup mounted **read-only** as a backup source, `RESTORE`d into its own
  volume (snapshot stays pristine). Browse via ch-ui (`localhost:3488`) or
  `/play` (`localhost:8123`, user `default` / pass `clickhouse`).
- Restored (fit in ~67 GB free disk): `ledgers, accounts,
account_balances_current, soroban_contracts, assets, liquidity_pools,
lp_positions, wasm_interface_metadata, nfts_pending, nft_ownership_pending`.
  The giants (`transactions` 184 GB, `soroban_events` 204 GB, `*_index/
*_appearances`) do NOT fit locally — those need prod.

So all numbers below are the **2026-05-21 (Phase 6) state** as a proxy. Re-run
Step 0 on live prod for the real go-live sizing (live 2026-06-10 pending was
larger: 59.7M / 138.5M, with SAC-leak regrown).

## Step 0 results (run locally as proxy)

**Q1 — verdict breakdown (`soroban_contracts FINAL`, contract_type):**

| contract_type | count   | meaning                         |
| ------------- | ------- | ------------------------------- |
| 0             | 294,963 | SAC (is_sac=true)               |
| 1             | 21,523  | Other                           |
| **2**         | **1**   | **Nft** ← only ONE ever written |
| 3             | 2       | Fungible                        |
| NULL          | 4,875   | unclassified                    |

`is_sac`: true 294,963 / false 26,401. Total 321,364 contracts.
This is the smoking gun: of 26,401 non-SAC contracts, exactly **1** ever got a
`Nft` verdict — confirms root-cause #1 (verdict written only on the
never-happening same-ledger coincidence).

**Q2 — would-be-Nft contracts after rebuild (join to `wasm_interface_metadata`): 107.** would-be-Fungible: 4,159. So the rebuild flips `Nft` from 1 → **107**.

**Q3 — Bachini `CDA5FGE4…` (only verified real mainnet NFT):**
`contract_type = 1 (Other)`, `is_sac = false`, `has_wasm = 1`. A real NFT,
has WASM, not SAC — sitting as `Other`. Proves the bug on a known-good row.
**Additional finding:** Bachini has **0 rows** in `nfts_pending` AND **0** in
`nft_ownership_pending` → its token events were never extracted at all (see
Step 6 below).

**Q4 — pending volume under the 107 would-be-Nft contracts (= promote volume):**
`nfts_pending` **11,023** tokens, `nft_ownership_pending` **19,451** events.
NOT zero — there IS real NFT data trapped in pending.

**Contracts vs rows (the key mental model):** Q2 counts **contracts**
(collections); Q4 counts **rows** (individual tokens / ownership events). One
NFT collection mints many tokens — `CBHUX3RS…` alone holds 10,056 of the
11,023 tokens. 107 collections → 11,023 tokens (~103 avg) → 19,451 ownership
events (~1.8/token).

**What `nft-reclassify` does to all of pending** (post-rebuild):

| `nfts_pending` (48.85M) | rows       | action            |
| ----------------------- | ---------- | ----------------- |
| Other/wasm              | 48,566,311 | DROP              |
| Fungible                | 278,985    | DROP              |
| **Nft**                 | **11,023** | **PROMOTE → hot** |

| `nft_ownership_pending` (112.30M) | rows        | action      |
| --------------------------------- | ----------- | ----------- |
| Other/wasm                        | 111,762,794 | DROP        |
| Fungible                          | 521,884     | DROP        |
| **Nft**                           | **19,451**  | **PROMOTE** |

(No SAC bucket here — this snapshot is _after_ Phase 5 dropped 27.6M SAC rows;
on live prod the SAC-leak has regrown and would re-appear as a DROP bucket.)
Concrete proof of the task's "198M pending rows ≠ 198M of decision work":
~99.97% of pending is dropped, only ~0.02% is real NFT to promote.

`nfts_pending` = the tokens themselves (catalog: token_id, collection, media_url,
current owner). `nft_ownership_pending` = the ownership/transfer event log
(token_id, owner, event_type, ledger). Both keyed `(contract_id, token_id)`.

## Asset model & SAC (answering "do SAC assets need the SAC?")

`assets.asset_type` is an **explorer-synthetic** enum (NOT the Horizon XDR
discriminator) — `crates/domain/src/enums/token_asset_type.rs:19-24`:

| value | variant       | meaning                                                                    | identity                   |
| ----- | ------------- | -------------------------------------------------------------------------- | -------------------------- |
| 0     | Native        | XLM singleton                                                              | empty, contract_id=0       |
| 1     | ClassicCredit | classic issued asset (alphanum4/12)                                        | code+issuer, contract_id=0 |
| 2     | Sac           | classic/native asset **wrapped** as a Stellar Asset Contract               | contract_id set            |
| 3     | Soroban       | bespoke Soroban contract token classified **Fungible** by WASM (not a SAC) | contract_id only           |

Data: type0=1, type1=298,542, type2=2,065, **type3=2**. type3 is only 2 because
a row is written only when a deployed contract's WASM classifies `Fungible`
AND it is not a SAC (`crates/xdr-parser/src/state.rs:856-871`) — almost all
token activity on Stellar is SAC-wrapped classics (type 2), not natively-coded
Soroban fungibles. Producers: `state.rs:834-845` (Sac), `:856-871` (Soroban),
`:961` (ClassicCredit), `:986` (Native).

**Asset ↔ SAC — user intuition "a SAC asset needs the SAC" is correct for
type-2 rows:** all 2,065 type-2 asset rows join to a `soroban_contracts` row
and **every one has `is_sac=true`**; `assets.contract_id` → `soroban_contracts.id`.
Correction: a _classic_ asset does NOT require a SAC to exist — it lives as a
type-1 row on its own (from trustlines). A SAC is a _separate, additional_
type-2 row that appears only once the asset is wrapped/deployed as a contract.
One logical asset can thus be two rows (type-1 classic + type-2 SAC), sharing
aggregates (`crates/backfill-runner/src/asset_aggregates.rs:36-40`).

**Skeleton-derivation gap (context, not a 0283 bug):** there are 294,963
`is_sac=true` contracts but only 2,065 type-2 SAC _asset_ rows. The indexer
forward-derives the deterministic SAC `contract_id` from every observed
classic/native asset and flips `is_sac=true` on a skeleton
`soroban_contracts` row (`crates/indexer/src/handler/persist/mod.rs:319-327`,
task 0218), even when no SAC was ever deployed; a type-2 asset row is only
written when a SAC deployment is actually observed.

**Conclusion on assets:** asset indexing is healthy — no 0283-class problem.
The classification gap is purely on the Soroban-contract (fungible/NFT) side.

## Step 1 location decision — NOT backfill-runner → new `ch-maintenance-runner` crate

Operator decision (karolkow): the rebuild must **not** live in
`backfill-runner`. Rationale (confirmed in code):

- `backfill-runner`'s charter is **S3 historical ledger ingestion** (`Cargo.toml:5`
  "public S3 → Postgres or ClickHouse via parse-and-persist"; `main.rs:1-5`).
  Its real subcommands are `Run`/`Status`/`Bootstrap` — all S3-ingest scoped.
  That job is **complete** (task 0228 archived).
- `RepairTier1`, `AssetAggregates`, `NftReclassify` were bolted on as "0228
  Phase 5" cleanup only because the crate already had a CH `Sink`. They share
  nothing with ingestion (no S3, no `process_ledger`, no partitions) and even
  `panic` on the Postgres sink arm (`repair_tier1.rs:103`) — they want a bare
  `clickhouse::Client`, not the dual-target ingest sink.

**Decision:** create **`crates/ch-maintenance-runner`** (bin `ch-maint`) — a CH
post-hoc maintenance toolbox, mirroring the standalone-CLI precedent of
`backfill-enrichment-runner`. Subcommands: `contract-type-rebuild` (new),
plus **relocate** `repair-tier1`, `asset-aggregates`, `nft-reclassify` here
(same family: staging+`EXCHANGE TABLES` / `ALTER DELETE` CH state-correction
passes). Move, don't call into backfill-runner (avoids pulling its
indexer[pg-persist]/aws/reqwest dependency surface; and rebuild → nft-reclassify
is a single ordered pipeline best co-located, since nft-reclassify promotes
`WHERE contract_type=2` which only the rebuild populates).

Deps: `db-clickhouse` (client/config), `xdr-parser`
(`classify_contract_from_wasm_spec`, `classification.rs:101`), `domain`,
`clickhouse`, `clap`, `tokio`, `tracing(-subscriber)`, `serde_json`,
`thiserror`. Add to workspace `members`. Codegen gate stays N/A (no
`crates/api/**`; only `Cargo.lock` changes).

**Command-string fallout:** README/AC, ADR-0046 amendment, runbooks 0217/0221
currently say `backfill-runner contract-type-rebuild` / `… nft-reclassify`;
must become `ch-maint …`.

**"Do we update the `status` command?"** — backfill-runner's `Status` is
S3-ingest progress; it is _not_ touched by this work and stays as-is. If we
want a status/report for the maintenance ops (verdict breakdown, would-be-Nft
count, pending promote/drop volumes), that's a NEW `ch-maint status` (or the
`--dry-run` summaries) living in `ch-maintenance-runner` — not a change to
backfill-runner's command.

## Step 4 live-gap decision — option (c) cache + batched fallback

Latency simulation (measured on local ch-snap; prod RTT assumed 30 ms,
verify against real Lambda→Hetzner):

- **Current per-ledger cost** (backfill log `persist_ms`): mean 20 ms, p99 37,
  max 68 — vs the **~4 s** budget (ledgers close ~5–6 s). Huge headroom.
- **Deploys/ledger** (`soroban_contracts.deployed_at_ledger`): avg ~1.08,
  p99 2, **max (peak) 59** (ledger 62463442). Bursty.
- **Point lookup**: `soroban_contracts` by wasm_hash ~3 ms server-side;
  `wasm_interface_metadata` ~9 ms. **Cold-start full read** of
  `wasm_interface_metadata` (3,216 rows / 10.7 MB): ~9 ms server / ~150 ms
  wall.

Per-ledger added cost:

| option                                                         | avg       | peak (59 deploys)                        | verdict                               |
| -------------------------------------------------------------- | --------- | ---------------------------------------- | ------------------------------------- |
| (b) DB-lookup **per deploy**                                   | ~43 ms    | **~2.36 s** (≈3.5 s @50 ms RTT)          | **risky** — burst can blow the budget |
| (b) DB-lookup **batched** (`wasm_hash IN (...)`, 1 RTT/ledger) | ~30–60 ms | ~30–60 ms                                | acceptable                            |
| (c) in-memory cache (bootstrap at cold start)                  | ~0 ms     | ~0 ms (+~150–250 ms once per cold start) | **safest**                            |
| (a) no live change                                             | 0 ms      | 0 ms                                     | leaves the bug live                   |

**Decision:** **option (c)** — bootstrap a `HashMap<wasm_hash, verdict>` at
Lambda cold start from `wasm_interface_metadata` (trivial: 3,216 rows / 10 MB),
O(1) steady-state lookups, immune to deploy bursts. Add a **cache-miss
fallback** that does a **single batched** `wasm_hash IN (...)` lookup per
ledger (never per-deploy) for hashes not yet cached (covers same-/recent-ledger
WASM uploads), plus periodic/cold-start refresh. The only configuration that
threatens the 4 s budget is plain per-deploy option (b) — avoid it.

Implication for indexer (Lambda, ephemeral): a persistent in-memory map "for
ever" is impossible across cold starts, but per-warm-container bootstrap +
batched miss-fallback fits the Lambda model. This is the build target for the
spawned follow-up (Step 4 still spawns a task; now with a decided approach).

## Step 6 (new) — Bachini / i128 token_id event-extraction gap

Bachini (`CDA5FGE4…`) is `Other` (Step 6a, fixed by the rebuild) **and** has
**0 rows** in both pending tables. So even a correct rebuild + reclassify
surfaces nothing for it — its mint/transfer events were never extracted. The
deep-dive note flags Bachini as SEP-39 with **i128 token_id**; the event
parser likely doesn't capture that token_id shape. This is a **separate gap**
from 0283's classification bug. Add as the last step: document + spawn a
follow-up to investigate i128/SEP-39 NFT event extraction. Without it,
"NFTs fixed" still leaves the flagship NFT empty.

## Batch reclassification — measured timing (ch-snap, local CH 26.3)

Actually ran the whole Step-2 pipeline on the restored snapshot (full-scale,
on copies). Numbers, not estimates:

| operation                                                                        | scale          | time       |
| -------------------------------------------------------------------------------- | -------------- | ---------- |
| contract-type-rebuild: staging build (CREATE+INSERT…SELECT over 321k, join wasm) | 321,364 rows   | **0.43 s** |
| `EXCHANGE TABLES` swap                                                           | atomic         | **0.13 s** |
| promote INSERT → hot (nfts)                                                      | 11,023 rows    | **0.14 s** |
| promote INSERT → hot (ownership)                                                 | 19,451 rows    | **0.11 s** |
| `ALTER … DELETE` full drain (nfts_pending)                                       | delete 48.84M  | **1.15 s** |
| `ALTER … DELETE` full drain (ownership_pending)                                  | delete 112.28M | **6.64 s** |
| `OPTIMIZE … FINAL` (both)                                                        | —              | **~0.2 s** |
| **total one-shot reclassification (full-drain upper bound)**                     |                | **~9 s**   |

Key correction to the task's prior assumption: the README's Step 2 note about
mutations being "heavy (ALTER DELETE on ~30M+ rows)" needing the 0281
maintenance window is **over-cautious** — even deleting _all_ 112M ownership
rows is ~7 s locally. The real prod drop (types 0/3 = SAC-leak + fungible,
~9M/19M live) is smaller. Caveat: prod is a single remote Hetzner node, live
ownership_pending is larger (138M), and live merge load competes — budget
seconds-to-low-tens-of-seconds, still NOT a long window. Verify on prod, but
don't gate on 0281 purely for runtime.

## API contracts-list — in scope as a CONSEQUENCE (one small CH label follow-up)

`GET /v1/contracts` (`crates/api/src/contracts/{handlers.rs:72, queries_ch.rs:103}`)
is a **pure consumer** of `soroban_contracts.contract_type` (reads `sc.contract_type`
via `FINAL`; `filter[type]=nft` → `AND sc.contract_type = 2`). So the rebuild
flips the list counts **1 Nft / 2 Fungible → 107 / ~3,937 with NO API code
change** — confirmed in scope. The user's "≈1 vs ≈2" observation = exactly the
broken state (type2=1, type3=2).

**One latent CH-path bug to fix alongside:** `contract_type_name()` in
`queries_ch.rs:48-54` only maps 0→token, 1→other; Nft(2)/Fungible(3) fall to
`None`. After the rebuild the API returns correct `contract_type` int but
`contract_type_name: null` on the CH datasource (PG path already fixed via
migration). A frontend rendering the _name_ would show blank. 2-line fix +
stale test at `queries_ch.rs:648`. Add to 0283 docs/AC (small) or a tiny
follow-up.

## Assets `asset_type=3 = 2` — same bug class, SEPARATE table, NOT in 0283 scope

`asset_type=3` (Soroban bespoke fungible) having only 2 rows is **the same
same-batch-coincidence bug**, not "correct as-is" — the type-3 asset row is
emitted only when the WASM is classified in the same ingest batch as the deploy
(`crates/xdr-parser/src/state.rs:853-871`), identical gate to the contract_type
verdict. Decisively: the 2 type-3 asset rows are the **same 2 contracts** as
the 2 `contract_type=3` rows.

Data: of ~3,937 would-be-fungible non-SAC contracts, only **2 have any `assets`
row** → **~3,935 missing from `assets`**. The PG persist path has a late-WASM
bridge (`insert_assets_from_reclassified_contracts`,
`crates/indexer/src/handler/persist/write.rs:543-584`) but the **ClickHouse
path never ported it** — so CH `assets` permanently under-represents Soroban
fungibles.

**Scope:** 0283's rebuild touches `soroban_contracts.contract_type` ONLY, not
`assets`. So after 0283 the assets table is still wrong. This is a **separate
follow-up** (don't widen 0283 to a second table with its own identity key):
(1) a one-shot CH `INSERT INTO assets … SELECT … FROM soroban_contracts WHERE
contract_type=3 AND NOT is_sac AND NOT EXISTS(asset row)` after the rebuild,
and (2) port the late-WASM assets bridge to the CH live path so new fungibles
keep landing in `assets`. Belongs next to the Bachini follow-up.

## Live-gap — deeper analysis revises the choice: hybrid (d) over plain (c)

A ClickHouse **dictionary** is the better primitive than a per-Lambda HashMap.
The codebase already proves the pattern: `transaction_hash_dict`
(`crates/db-clickhouse/schema/init.sql:457-473`) — loopback `CLICKHOUSE`
SOURCE via a dedicated `dict_reader` user, `LIFETIME MIN 300 MAX 360`
auto-refresh, idempotent `CREATE DICTIONARY IF NOT EXISTS` applied by the same
schema sidecar. (Deployed but currently only exercised in a smoke test — there's
machinery but no prod read path queries it yet.)

**New option (d): `wasm_verdict_dict`** (`wasm_hash → Int16 verdict`, sourced
from `wasm_interface_metadata`, classifier as a SOURCE-side `multiIf`,
`LAYOUT(HASHED)` — 3,216 rows fully in RAM, shared across ALL consumers, zero
per-Lambda bootstrap). Validated: a dictGet-equivalent over the 21,523 Other
contracts immediately recovers 106 NFT + 4,157 Fungible.

Devil's advocate, one strongest failure each:

- **(a) cron rebuild** — staleness isn't cosmetic: NFT events for an
  as-yet-Other contract keep routing to `*_pending` and stay invisible to the
  API until the next cron + a _second_ drain pass; a perpetual growing backlog
  - two recurring mutations forever.
- **(b) per-ledger DB lookup** — puts a CH round trip on the critical path of
  _every_ ledger (even the ~92% with 0 deploys) and couples persist latency to
  CH health (a merge/`parts_to_throw_insert` storm becomes a persist stall).
  Strictly worse than a RAM dictionary once gated to deploy-bearing ledgers.
- **(c) in-memory HashMap** — Lambda concurrency → N caches → N× cold-start CH
  reads + N× RAM; a WASM uploaded after a container bootstrapped is invisible to
  it → needs a DB fallback anyway, i.e. c is really **c+b**. (The code already
  learned this: `classification_cache.rs` deliberately never caches `Other` so
  late WASM can still promote.)
- **(d) dictionary** — `LIFETIME` refresh lag (up to ~6 min on the tx-dict
  precedent) + chicken-and-egg when upload+deploy share a ledger (dict not yet
  refreshed → writes Other like today). A query-time variant adds a per-row
  dictGet on the hot contracts-list scan.

**Recommended hybrid (build target for the Step-4 follow-up):**
**d1 write-time dictGet + same-ledger in-stage fallback + `nft_reclassify` backstop.**

1. Add `wasm_verdict_dict` (HASHED, short LIFETIME ~30–60 s; reuse the
   `dict_reader`/loopback/idempotent-DDL machinery the tx-dict already proves).
2. The pure stage (`stage.rs`) has **no DB handle** (by design, comment at
   `stage.rs:891-893`), but the **writer/handler DO** hold a `clickhouse::Client`
   (`writer.rs:72`, `indexer/handler/mod.rs:130`). So as a post-stage step on
   deploy-bearing ledgers only: resolve verdict from (i) the in-process
   same-ledger `wasm_classification` map already built in-stage
   (`stage.rs:344-352` — kills the chicken-and-egg for constructor-pattern
   same-ledger upload+deploy), else (ii) `dictGet(wasm_verdict_dict)` for
   earlier-ledger WASM. Overwrite `contract_type` before `write_ledger`. This
   restores the parity the CH cutover dropped — the PG path already does this
   via `reclassify_contracts_from_wasm` (`indexer/handler/persist/mod.rs:260-264`).
3. Keep `nft_reclassify` (now in `ch-maintenance-runner`) as a periodic idempotent
   backstop for residual dictionary-lag edge cases.

**Hard prerequisite (load-bearing, masked today by the all-Other state):** fix
`queries_ch.rs::contract_type_name` (2→nft, 3→fungible) BEFORE any fix lands.
Verify on prod: dict load under the `users.d` `:ro` bind-mount (needs CH
restart; README.md:195 documents a `CLICKHOUSE_PASSWORD` mismatch failure
mode), and that the in-stage same-ledger map is populated in the _live
single-ledger_ writer path, not just multi-ledger backfill.

## Live decision REVISED (2026-06-11 pm) — 3rd async Lambda, dict dropped

New operator info: live reclassification will **not** be inline in the indexer
— it will be a **separate Lambda**, like the existing `enrichment-worker` (the
indexer already triggers a 2nd Lambda for SEP-1/NFT enrichment via SQS,
fail-open, off its critical path). This **supersedes the dict hybrid above.**

How the enrichment pattern works (file:line):

- Indexer publishes per-batch, **after** ledgers are durably persisted,
  best-effort/non-blocking (`indexer/handler/mod.rs:323-354`, fail-open at
  `:344-346`); message carries the key (`enrich_and_persist/message.rs:21-28`)
  with a per-batch CH anti-join to publish only misses
  (`enrich_and_persist/filter.rs:43-113`, optimisation not correctness gate).
- Worker = SQS event-source consumer, no per-ledger budget, idempotent RMT
  INSERTs, `ReportBatchItemFailures` retry/DLQ, cold-start client reuse
  (`enrichment-worker/src/main.rs:49-144`), writes only side tables.

Why the 3rd Lambda is better than inline (a/b/c/d): moving the work **off the
indexer removes the ~4 s-budget concern entirely** → no cache, no
`wasm_verdict_dict`, no chicken-and-egg, no per-deploy RTT math. The indexer
keeps writing `Other` + routing NFT events to `*_pending` (cheap, correct by
construction); the worker fixes verdicts + drains pending async — the
enrichment consistency model the product already tolerates.

**Critical asymmetry → trigger choice.** Reclassification is a **set operation
keyed on `contract_type`** (~9 s whole pipeline: rebuild + promote +
`ALTER DELETE` + OPTIMIZE), NOT a per-entity fetch like enrichment. Therefore:

- Trigger = **scheduled cron** (EventBridge, every N min) or a **coalesced**
  doorbell (short SQS visibility + self-guard), NOT per-deploy (that re-runs
  the ~9 s mutation pipeline on every deploy and self-contends). Message is a
  content-free doorbell — no useful per-entity payload (operates on the whole
  set), the _opposite_ of the key-carrying enrichment message.
- **Biggest risk = `ALTER DELETE`/OPTIMIZE mutation contention** on the single
  Hetzner node (enrichment gets idempotent-INSERT safety "for free"; this does
  not). Mitigate with a **singleton guard**: reserved concurrency = 1, or a
  `system.mutations` in-flight check that no-ops, so runs never overlap each
  other or the live merge load / 0281 window.

**The worker IS the `ch-maint` pipeline triggered live** — `ch-maintenance-runner`
stays the single source of the logic; the Lambda is a thin trigger wrapper that
calls the same rebuild → assets-backfill → nft-reclassify. So Step 1's crate
decision is reinforced, not changed. The dict (d1) is dropped: it only fixed
verdicts _going forward at write time_ and still needed nft-reclassify as a
backstop for rows already in pending — the worker is that backstop promoted to
the primary mechanism, with far less machinery (no dictionary DDL/refresh/
dictGet wiring).

### "Why are indexer DB lookups normally cheap but this 'expensive'?"

They aren't inherently expensive — the concern was specific. Normal indexer DB
ops are **batched writes** (the work itself) + at most **one bounded read per
batch** (the enrichment anti-join), off the correctness path, fail-open. The
"expensive" framing applied ONLY to a naive **per-deploy synchronous** verdict
lookup on the indexer's 4 s critical path (peak 59 deploys × RTT, blocking the
write pipeline, coupling to CH liveness). A single _batched_ lookup is ~30–60 ms
— also cheap. And once the work moves to the **3rd async Lambda (off the
critical path)**, a plain indexed read is perfectly fine — `wasm_interface_metadata`
is already `ORDER BY wasm_hash` (its "index"), so no dict needed. The dict only
ever earned its keep to keep an _inline-on-the-4s-path_ lookup at ~0 ms; off
that path, the index suffices.

## Addendum (2026-06-11 pm) — indexer reads verified, naming, live RE-OPENED

### "Does the indexer really only see the current ledger?" — user was RIGHT to push back

Full inventory of indexer DB reads:

- **Live CH path (prod Lambda today):** only (1) the cursor
  `SELECT max(sequence) FROM ledgers` (`handler/mod.rs:204-209`) and (2) the
  enrichment dedup anti-join (`enrich_and_persist/filter.rs:43-93`, fail-open).
  The CH persist tree (`stage.rs`/`writer.rs`/`rows.rs`) has **zero SELECTs**;
  surrogate ids are CityHash-computed, not looked up. So "persist sees only
  the current ledger" is accurate **for the live CH path**.
- **PG path (`pg-persist`, dropped from the prod binary at the 0241 cutover):**
  about a dozen cross-ledger reads/RMWs — crucially the three classification
  bridges: `reclassify_contracts_from_wasm` (`persist/write.rs:240-325`),
  `insert_assets_from_reclassified_contracts` (`write.rs:543-584`),
  `promote_pending_nfts_to_hot` (`write.rs:337-417`), plus
  `apply_sac_overrides_for_skeleton_contracts` (`write.rs:482-510`),
  `resolve_nft_filter` + `classification_cache.rs`, upsert-RETURNING id
  resolution, orphan-pool detection, asset aggregates.

**So the blanket claim "indexer does no DB reads" was wrong** — the PG path
read and fixed cross-ledger state at persist time. The CH cutover dropped
exactly those bridges; that IS bug #4. This reframes the live fix as a
**parity port**, not new design.

### Crate naming + shared-code audit

- Family convention is `*-runner` (`backfill-runner`,
  `backfill-enrichment-runner`) → new crate named **`ch-maintenance-runner`**
  (bin `ch-maint`). `reclassify-runner` rejected (too narrow — also hosts
  repair-tier1/asset-aggregates); bare `maintenance-runner` rejected (loses
  the CH scoping).
- Triads: enrichment = `enrichment-shared` / `backfill-enrichment-runner` /
  `enrichment-worker`. Indexing = the `indexer` crate is **both lib and bin**
  — its lib target IS the shared layer (backfill-runner depends on
  `indexer = { features = ["pg-persist"] }` and calls
  `indexer::handler::process::process_ledger`).
- **"xdr-parser is not used in the indexer" is FALSE**: `indexer/Cargo.toml:35`
  depends on it; `xdr_parser::` used in 6 indexer files, incl. the classifier
  call at `handler/persist/staging.rs:561`. xdr-parser is the most-shared
  crate in the workspace (api, audit-harness, backfill-runner, indexer,
  db-clickhouse, backfill-bench). **Classifier stays in xdr-parser**: its
  input `&[ContractFunction]` is a parser type (moving it to `domain` would
  invert the dependency), and it already ships
  `From<ContractClassification> for domain::ContractType`.

### Live decision RE-OPENED — dev-cost comparison favors the inline port

Given the PG bridges exist as a reference implementation, the inline option is
no longer "new machinery on the 4s path" — it's a port:

|             | A: inline port (CH writer)                                              | B: 3rd Lambda                                      |
| ----------- | ----------------------------------------------------------------------- | -------------------------------------------------- |
| dev         | **~2–3 days** (port + tests)                                            | ~4–6+ days (bin + IaC + cert + guard + monitoring) |
| infra       | none                                                                    | Lambda + cron/queue + DLQ + alarms                 |
| runtime     | ~50 ms on the ~8% WASM/deploy ledgers; ~1 s promote only on a real flip | 0 on indexer; ~9 s per cron tick                   |
| consistency | immediate                                                               | minutes window                                     |
| risk        | extra CH read on persist (fail-open)                                    | mutation overlap; ops surface                      |

**Recommendation: A.** CH mechanics: verdict flip = re-INSERT into the RMT
(reads use FINAL), no ALTER UPDATE; the dict/cache options are all dead — a
plain batched read against `ORDER BY wasm_hash` suffices. Decision awaits
operator confirmation; B's full analysis above remains valid if ops prefers
async isolation.

## Addendum 2 (2026-06-11 eve) — fundament audit: quarantine, design intent, full gap list

### Is `nfts_pending` "speculative classification"? — NO, it's the opposite

User challenged: "why classify things on-spec into nfts*pending?" Audit verdict:
the quarantine is the mechanism that **prevents** speculation from reaching
users. `grep nfts_pending crates/api/` → **zero hits**; ADR 0046: *"API
endpoints never read the `_pending` tables"_; hot `nfts` receives only
WASM-confirmed rows. The pre-quarantine design measured **99.4% garbage** in
`/v1/nfts*` — quarantine was the fix. "Better no data than wrong-class data"
is already the hot-table policy; pending = "no data \_yet_, evidence retained."
Drop-outright was Alternative 4 in ADR 0046, rejected because it would have
silently destroyed the real SEP-39 NFT (Bachini) — _"no amount of downstream
sophistication can recover rows that never reached the parser's emit."_

### CORRECTION of an earlier claim in this note + a NEW bug

The "What reclassify does" table earlier in this note mislabeled the dominant
bucket: a `LEFT JOIN` on non-Nullable `FixedString` fills **zero bytes, not
NULL**, so "Other/wasm (drop)" was wrong. Correct split (verified per-contract):

| bucket                         | nfts_pending        | ownership_pending    | contracts | fate                          |
| ------------------------------ | ------------------- | -------------------- | --------- | ----------------------------- |
| **NO deploy/wasm link at all** | 48,566,065 (99.41%) | 111,762,439 (99.52%) | 4,461     | **STAYS** (TRUNCATE decision) |
| Fungible (resolvable)          | 278,985             | 521,884              | 1,151     | drop                          |
| **Nft (resolvable)**           | **11,023**          | **19,451**           | 34        | promote                       |
| Other (resolvable)             | 246                 | 355                  | 61        | stays by policy               |

**NEW BUG (follow-up): deploy-linkage gap.** 4,461 contracts emit events
across the whole 12M-ledger window but have **no deploy record / wasm_hash**
ever (top: `CDP5RUMSC7YJ…` = 4.86M pending rows). Deploy extraction works in
every 1M-ledger bucket, so a _class_ of deploys is systematically missed
(`extract_contract_deployments` takes only `contract_data/created` instance
entries — candidates: restored entries, meta-unavailable ledgers). Until fixed
(or supplemented by RPC instance fetch), **drop-outright stays unsafe** and
pending cannot shrink below ~99%.

Also: **late-WASM case is empirically empty** — 0 resolvable pending rows have
`event ledger < wasm_uploaded_at_ledger`. The resolvable slice exists ONLY
because the stage can't see prior-ledger verdicts → inline lookup eliminates
it entirely going forward.

### A REAL violation of the user's principle (separate from quarantine)

**294,963 SAC skeleton rows (92% of `soroban_contracts`) are exposed via
`/v1/contracts`** — `queries_ch.rs:129-141` has no skeleton filter, while only
2,065 SACs actually exist as deployed assets. Phantom rows of certain class
but speculative existence, user-visible. Follow-up: filter or flag.

### Design intent verdict: no-reads is DELIBERATE, the gap was KNOWN

- Stage purity + cityhash ids documented as design: _"the writer is still
  single-pass and fully deterministic across replays"_ (`persist.rs:14-16`);
  _"CH has no DB access in the stage, so prior-ledger classifications are not
  visible here"_ (`stage.rs:891-893`). ADR 0044/0048: RMW/UPDATE
  _"avoided project-wide"_ on CH; RMT merge substitutes upserts.
- ADR 0046 promised the compensation — _"re-emission on next observation"_ —
  which was **never implemented** (0283 root cause #2). So: purity =
  deliberate; missing bridges = known trade-off with vaporware compensation.
  An inline post-stage fix does NOT violate the principle if reads live in
  the **writer** (which already holds a Client), keeping the stage pure.

### FULL live-gap inventory (all reclassifications, not just NFT)

| #   | PG mechanism                           | CH live today                                                                                                                                                                                                                                                           | CH batch                             | LIVE GAP?                                       | trigger freq                     |
| --- | -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------ | ----------------------------------------------- | -------------------------------- |
| G1  | reclassify_contracts_from_wasm         | none (same-ledger only)                                                                                                                                                                                                                                                 | rebuild (planned)                    | **YES**                                         | ~0.17% ledgers (WASM)            |
| G2  | insert_assets_from_reclassified        | none                                                                                                                                                                                                                                                                    | assets-backfill (planned)            | **YES**                                         | subset of G1                     |
| G3  | promote_pending_nfts_to_hot            | none (re-emission vaporware)                                                                                                                                                                                                                                            | nft-reclassify (no-op until rebuild) | **YES**                                         | only on real flip (107 in 14 mo) |
| G4  | SAC skeleton overrides                 | **covered** (`prepare_with_sac_overrides`, version-0 sentinel)                                                                                                                                                                                                          | —                                    | no                                              | —                                |
| G5  | contract name writes                   | **PARTIAL + NEW HAZARD**: name-only row (`stage.rs:403-415`) outversions the deploy row under RMT(version=wasm_uploaded_at_ledger) → **clobbers wasm_hash/deployer/contract_type to NULL**, undermining the rebuild's wasm_hash join; `assets.name` mirror never ported | repair-tier1 partially repairs       | **PARTIAL**                                     | ~deploy-class (~0.2%)            |
| G6  | asset aggregates (holder_count/supply) | none — by design (_"cannot run that style of recompute economically"_, `asset_aggregates.rs:1-17`)                                                                                                                                                                      | asset-aggregates                     | yes, but **stays batch** (every-ledger trigger) | every ledger                     |
| G7  | orphan-pool FK sentinels               | obsolete (no FKs on CH)                                                                                                                                                                                                                                                 | —                                    | no                                              | —                                |
| G8  | first_seen-class watermarks            | RMT drifts to latest                                                                                                                                                                                                                                                    | repair-tier1 (MIN from facts)        | yes, but **stays batch**                        | every ledger                     |
| G9  | routing vs prior-ledger verdicts       | quarantine-first                                                                                                                                                                                                                                                        | reclassify drains                    | optional — acceptable once G1+G3 live           | common                           |

**Minimal inline scope = G1+G2+G3 (+G5 hazard fix), all gated to ≤0.2% of
ledgers.** Measured frequencies on the snapshot: deploy-bearing ledgers
22,067/12.07M = **0.183%**, WASM-bearing 20,003/12.07M = **0.166%**.

### Per-ledger cost of inline (the user's ONLY real constraint)

- **99.8% of ledgers: +0 ms** (no deploy, no WASM — step not entered).
- **~0.2% of ledgers: one batched SELECT** (~10 ms compute + ~30 ms RTT) +
  re-INSERT of a few rows (~10 ms) ≈ **+50 ms**, vs current persist ~20–80 ms
  and a 4–5 s budget.
- **Real Nft flip: 107 times in 14 months** (~once per 4 days): promote
  INSERT ~0.1–1 s + targeted drop. Worst single ledger stays ≲1–2 s.
- **Expected average: ~0.1 ms/ledger.** Fail-open: if the lookup errors, the
  row routes to pending exactly as today — quarantine becomes the safety net,
  zero correctness loss, batch backstop drains later.

### Final comparison (live mechanism), devil's advocate each

| criterion                | **inline port (writer)**                                           | CH dict                                                                 | 3rd Lambda                                                 | cron only                                       |
| ------------------------ | ------------------------------------------------------------------ | ----------------------------------------------------------------------- | ---------------------------------------------------------- | ----------------------------------------------- |
| added per ledger (99.8%) | 0 ms                                                               | 0 ms                                                                    | 0 ms                                                       | 0 ms                                            |
| added on trigger ledgers | ~50 ms                                                             | ~0 ms                                                                   | 0 ms                                                       | 0 ms                                            |
| dev effort               | **~2–3 d** (PG reference)                                          | +1–2 d on top (DDL/refresh/wiring)                                      | ~4–6+ d (infra)                                            | ~1 d                                            |
| new infra                | none                                                               | dict DDL + dict_reader                                                  | Lambda+cron/SQS+DLQ+cert                                   | cron entry                                      |
| freshness                | immediate                                                          | immediate-ish (LIFETIME lag)                                            | minutes                                                    | N minutes                                       |
| failure mode             | lookup fails → pending (safety net)                                | dict stale → Other → pending                                            | mutation overlap on node                                   | growing window                                  |
| devil's advocate         | +1 read couples persist to CH (mitigated: fail-open, 0.2% ledgers) | machinery for a lookup that fires on 0.2% of ledgers — over-engineering | most moving parts, eventual window, still needs same logic | longest window; two recurring mutations forever |

**Winner: inline port + quarantine-as-safety-net + scheduled `ch-maint` as
backstop.** The dict is dead (its value assumed a hot-path lookup; the lookup
fires on 0.2% of ledgers). The 3rd Lambda buys async isolation nobody needs at
these frequencies and costs the most dev. The runner crate stays REGARDLESS of
the live choice: one-shot history rebuild + the by-design-batch gaps (G6, G8)

- TRUNCATE tooling — it is complementary to inline, not an alternative.

(Diagram artifacts were generated for the CTO review and removed afterwards —
operator decision; the flows they showed are described in README Step 5.)

## Addendum 3 (2026-06-11 night) — "do we even need pending?" — the elimination ladder

User's fundamental question: if classification happens inline in the indexer,
why write pending rows at all? Analysis:

**Protocol ordering guarantees the answer is "we don't, on the happy path".**
WASM upload → deploy → events is enforced by the chain (can't deploy
unobserved code, can't call an undeployed contract). The "WASM arrives after
the event" case measured **0 rows** across 12.07M ledgers. So with verdicts
resolvable at write time, NO event ever NEEDS to wait in quarantine.

**But G9 is required for full elimination — and my earlier "G9 optional" was
wrong.** G1 alone (verdict at deploy time) fixes `soroban_contracts`, but
`route_for` still can't see prior-ledger verdicts — so events from a
correctly-classified Nft contract would STILL route to pending forever,
needing perpetual drain runs. The complete inline fix = **G1+G2+G3+G5+G9**,
where G9 = verdict resolution at event-routing time.

**G9 cost (measured):** NFT-shaped events appear in **79.9% of ledgers**
(9.65M/12.07M) — but from only **5,707 distinct contracts** over 14 months.
That's the ideal shape for the PG path's own `ClassificationCache` pattern
(`persist/classification_cache.rs` — lazy per-key memoization, never cache
unknown so late flips re-resolve; an Nft/Fungible verdict never changes once
set → safe to cache forever). Steady state ≈ **0 ms** (HashMap hit); one
batched `IN (...)` SELECT only when a never-seen contract appears. No cold
bootstrap needed (lazy fill). This is NOT the rejected bootstrap-cache option:
keyspace is 5.7k, no bootstrap, miss = one batched read.

**What pending becomes: a DLQ, not a pipeline stage.** With G1+G9 live, the
only inflow is events from contracts whose deploy the parser never observed —
today exactly the deploy-linkage bug population (4,461 contracts, 160M rows).
Rows arriving in pending = bug signal (alarm-worthy), not normal operation.

**Elimination ladder (the fundamental fix the user wants):**

1. Inline G1+G2+G3+G5+G9 → pending inflow drops to unknown-deploy contracts only.
2. Fix the deploy-linkage gap (follow-up) → inflow ≈ 0.
3. Then: TRUNCATE pending (0217 Part 2), optionally drop the tables and route
   unknown → drop + metric/alarm. Full "classify once, correctly" achieved.
   Eliminating pending BEFORE step 2 would silently destroy data from the
   4,461 broken-linkage contracts (some may be real NFTs — unverifiable until
   the linkage is fixed). Same reason ADR 0046 rejected drop-outright.

Promote (G3) note: with G9 live, promote-at-flip only matters for rows that
accumulated while a contract was unknown — after the history rebuild +
linkage fix, that converges to nothing.

## Addendum 4 (2026-06-11 night) — devil's advocate verification of the latency claims

Attacked every number from the Slack summary. Results:

| claim                                         | attack                                                                                                                                        | verdict                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| --------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| "59 deploys = 1 hash" (factory)               | is it luck? Measured max **unique** wasm hashes per deploy-ledger over the whole history                                                      | **CONFIRMED as a rule**: max = 3, p99 = 2, avg 1.03 — even stronger than claimed                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| RTT 30 ms assumption                          | is the Lambda transatlantic? (source bucket aws-public-blockchain lives in us-east-2)                                                         | **CONFIRMED conservative**: indexer Lambda + its own LedgerBucket + SQS all in **eu-central-1** (`infra/envs/production.json:3`; deliberate 0239 cutover "for Hetzner CH proximity"). us-east-2 is backfill-only. Frankfurt→Hetzner DE typically 5–15 ms                                                                                                                                                                                                                                                                                                                                                 |
| "~9 cache-misses/day"                         | recency bias (historical mean)? Last 30 days of snapshot window                                                                               | **HOLDS, even better**: 3.5 new contracts/day recently (vs 8.9 historical avg)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| "~0.3% deploy/WASM ledgers"                   | recency bias                                                                                                                                  | **UNDERSTATED ~2×**: last 30 days = **0.374%** deploy-ledgers. Corrected weighted avg ~0.3 ms/ledger (vs 0.15) — still ~0.008% of budget, immaterial                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| "4–8 ms server-side, flat"                    | measured on idle freshly-merged table; prod has constant inserts → more parts → FINAL costlier                                                | **2× under fragmentation**: with extra unmerged parts the same lookups run **10–18 ms**. Restated bucket: "+15–50 ms" on trigger ledgers. Still immaterial vs 4 s. Verify on prod in Step 0                                                                                                                                                                                                                                                                                                                                                                                                              |
| "flip ≤1 s"                                   | implementation could naively do a synchronous `ALTER DELETE` of the flipped contract's pending rows (mutation on a 48M/112M table under load) | **GUARDRAIL added**: the inline flip does promote-INSERT only; pending cleanup stays **async or deferred to the batch backstop**. Never `mutations_sync=1` on the live path                                                                                                                                                                                                                                                                                                                                                                                                                              |
| "verdicts immutable once set" (cache forever) | **Soroban contracts can UPGRADE their WASM** — verdict could change                                                                           | **Pre-existing hole, NOT a regression**: the parser only processes `created` instance entries (`state.rs:59`); instance `updated` entries are dropped on BOTH paths, so `soroban_contracts.wasm_hash` permanently holds the deploy-time hash and even PG's bridge (matching on the stored hash) can't re-classify an upgraded contract. PG's ClassificationCache shared the same forever-cache assumption. The cache adds nothing worse — but the upstream gap is a real correctness hole → **follow-up: handle contract-instance `updated` entries** (refresh wasm_hash + verdict + cache invalidation) |

Net: all Slack claims survive with two numeric corrections (deploy-rate ~0.4%,
server-side lookup 10–18 ms under fragmentation — both still ~0.008% of
budget), one strong confirmation (eu-central-1 → RTT assumption conservative),
one implementation guardrail (no sync mutations on the live path), and one
new pre-existing gap discovered (WASM upgrades never re-classified — parity
with PG, separate follow-up).
