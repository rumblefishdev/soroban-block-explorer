---
prefix: R
title: 'ClickHouse enrichment write-strategy — how to fill assets/nfts enrichment columns on CH without a cheap UPDATE'
status: mature
spawned_from: '0231'
date: '2026-06-07'
who: karolkow
tags:
  [
    clickhouse,
    enrichment,
    replacingmergetree,
    coalescingmergetree,
    aggregatingmergetree,
    write-strategy,
    research,
  ]
---

# R — ClickHouse enrichment write strategy

How to persist the SEP-1 asset enrichment (`assets.{icon_url, name}`) and the
NFT `token_uri` enrichment (`nfts.{name, media_url, collection_name}`) on
ClickHouse, given that CH has no cheap per-row `UPDATE` and the enrichment runs
**live, concurrently with an indexer that re-writes the same rows**.

This note captures the full research behind task 0231: the precise problem
definition, every option considered (the whole ClickHouse MergeTree engine
family plus the non-engine alternatives), and a single reproducible simulation
that exercises every option against every problem on a live ClickHouse
**26.3.10.60** (the production-pinned version). Every "works / fails" verdict
below is a **measured** result, not an opinion (raw output in the Appendix).

---

## 1. Why this research was needed

The Postgres enrichment is a plain `UPDATE` of two columns:

```sql
-- crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs:172
UPDATE assets
SET icon_url = COALESCE(NULLIF($1, ''), icon_url, $1),
    name     = COALESCE(NULLIF($2, ''), name,     $2)
WHERE id = $3
```

The naive read of task 0231 is _"just translate this UPDATE into ClickHouse
SQL"_ (Postgres is being retired per ADR 0047; API reads already migrated per
task 0243). That read is wrong, and that is the entire reason for this research:
**ClickHouse has no cheap equivalent of this `UPDATE`**, and the enrichment is a
_second_ writer fighting a _continuous_ indexer over the _same_ rows. Porting the
enrichment is therefore not a syntax translation — it is choosing the correct
ClickHouse write architecture.

---

## 2. The problem, defined precisely

These are the constraints any solution must satisfy. The simulation in §4 is
built to reproduce each one.

- **P1 — Two independent live writers on the same row.** The **indexer** writes
  on-chain facts; the **enrichment runner** fills off-chain columns
  (logo / name / media). They run **concurrently, continuously, and
  uncoordinated**, both targeting the same primary key.

- **P2 — "Update" works fundamentally differently in ClickHouse.** Data lives in
  **immutable parts**. There is no cheap per-row/per-column `UPDATE`. The native
  way to "change" a row is to **INSERT a new one and let a background merge
  reconcile** same-key rows. The classic `ALTER TABLE … UPDATE` (a _mutation_)
  rewrites whole parts on disk, runs async, and is **avoided project-wide**
  (task 0228 plan explicitly rejected mutations in favour of staging+EXCHANGE;
  grep shows zero mutations in `crates/db-clickhouse/src`).

- **P3 — The indexer is NOT append-only — it re-writes existing rows.** The same
  key is re-emitted many times over a row's life (e.g. `total_supply`,
  `holder_count`, NFT ownership change). Every such re-write is a fresh whole-row
  insert.

- **P4 — The indexer's write depends on existing stored state
  (read-modify-write).** It does not write purely "from the blockchain": it
  recomputes aggregates from current DB state — PG inline per-ledger from
  `account_balances_current`
  (`crates/indexer/src/handler/persist/write.rs:2659`); CH as a separate batch
  staging pass (`crates/backfill-runner/src/asset_aggregates.rs`) because the
  per-ledger join is too expensive. That batch pass already reads and **preserves
  existing `a.icon_url` / `a.name`** (`asset_aggregates.rs:97,104`) — but the
  _live_ per-ledger path does **not** read the prior row (reading on the hot path
  would drop ingest below the ledger cadence — ADR 0043), so it re-emits the row
  with the enrichment columns `NULL`.

- **P5 — Race condition: the indexer can evict what enrichment added.** Because
  the indexer is continuous and its writes are "newer", a whole-row re-write with
  the enrichment columns `NULL` can win the merge and **wipe the logo/name** the
  enricher just stored. This is the crux.

- **P6 — The default engine replaces the WHOLE row — no per-column merge.**
  `ReplacingMergeTree` keeps one entire row per key. A partial writer that sets
  only its own columns (rest `NULL`) **NULLs out the other writer's columns**.
  There is no native "merge indexer's columns with enricher's columns" on the
  default engine.

- **P7 — No quiet window.** Enrichment runs live, concurrent with the live
  indexer. Any batch approach that rebuilds-and-swaps the table loses every row
  the indexer inserts during the build→swap window.

- **P8 — Reads run under a readonly profile.** The API reads as `api_reader`
  (`readonly = 1`, RBAC profile `read_only`): per-query `SETTINGS` overrides are
  rejected; `FINAL`, `argMax`, and `dictGet` are operators/functions and are
  allowed. **Measured** (sim v2) under a real `readonly=1` user: `FINAL`+join,
  `argMax`, and `dictGet` all run; a `SETTINGS max_threads=…` override is rejected
  with `Code 164 … READONLY`.

- **P9 — NFT twist: the version column IS the ownership clock.** `nfts` is
  `ReplacingMergeTree(current_owner_ledger)`; every transfer re-emits the row
  with a higher version and metadata `NULL`. Enrichment cannot hijack that clock
  to win without corrupting ownership history.

**In one sentence:** two uncoordinated live writers must each persist their own
disjoint columns of the same continuously-rewritten row, in a database whose
default mechanism replaces the whole row and whose only true UPDATE is too heavy
to run live.

### Why Postgres never had this problem (and which problems are ClickHouse-only)

The exact same two live writers exist in Postgres — the indexer upsert and the
enrichment `UPDATE`, concurrent — yet the clobber never happens, because
**Postgres `UPDATE` is per-column and MVCC serialises concurrent writes to a
row**:

- The indexer's assets upsert lists and `SET`-updates only its own columns —
  `asset_type, contract_id, name, total_supply, holder_count`
  (`crates/indexer/src/handler/persist/write.rs:1540-1552`); **`icon_url` is
  never written by the indexer at all** (absent from the INSERT column list and
  from `DO UPDATE SET`).
- The enrichment runs `UPDATE assets SET icon_url=…, name=…` — touching only its
  own columns, leaving `total_supply`/`holder_count` intact.
- Row-level locks + MVCC make the two writers **compose** (each lands its own
  columns) rather than overwrite the whole row.

Concretely, the worried scenario — _indexer wrote entity X, enrichment enriches
it, and a concurrent indexer update nulls the enrichment_ — **cannot happen in
Postgres**: `ON CONFLICT DO UPDATE SET` writes only the **listed** columns, so an
unlisted column like `icon_url` is physically untouched no matter how many times
the indexer re-upserts the row; the row lock merely serialises the two writers
and each keeps its own columns (`name`, the one shared column, is written by both
via `COALESCE(…, existing)`, so it is never nulled either — only value
precedence). This is the exact inverse of ClickHouse: there the indexer must
insert the **whole** row (a full `RowBinary` struct), so `icon_url = NULL` is an
unavoidable part of **every** indexer write — which is precisely what produces
the clobber (P5/P6).

So P1 (two writers), P3 (indexer re-writes rows) and P4 (read-modify-write) all
existed in Postgres too — but were **benign**, which is exactly why nobody ever
had to think about them. The painful problems are ClickHouse-only artefacts of
"no per-column UPDATE / whole-row replace / merge-on-read":

| Problem                                      | Postgres                     | ClickHouse |
| -------------------------------------------- | ---------------------------- | ---------- |
| P1 two live writers                          | exists — benign              | exists     |
| P2 no cheap UPDATE                           | no (cheap per-column UPDATE) | **yes**    |
| P3 indexer re-writes existing rows           | exists — benign              | exists     |
| P4 read-modify-write                         | exists — benign              | exists     |
| P5 one writer clobbers the other             | **no** (per-column UPDATE)   | **yes**    |
| P6 whole-row replace, no per-column merge    | **no**                       | **yes**    |
| P7 no quiet window (rebuild/swap loses rows) | N/A                          | **yes**    |
| P8 readonly read profile                     | same                         | same       |
| P9 version-clock collision (NFT)             | N/A                          | **yes**    |

This whole research exists because porting PG→CH turns a **benign** two-writer
pattern into a **clobbering** one — the price of ClickHouse's append-only,
whole-row, merge-on-read model.

---

## 3. Environment facts (verified in-repo)

- **Engine convention:** ReplacingMergeTree everywhere, with a version column as
  the "latest wins" clock (`crates/db-clickhouse/schema/init.sql`): accounts
  `RMT(last_seen_ledger)`, soroban_contracts `RMT(wasm_uploaded_at_ledger)`,
  nfts `RMT(current_owner_ledger)`, … — **except `assets`, which is `RMT` with NO
  version column** (the problem table).
- **Indexer writes enrichment columns as `NULL`** on every write
  (`crates/indexer/src/handler/persist/staging.rs:834`,
  `crates/xdr-parser/src/state.rs`; design rationale in
  `docs/architecture/indexing-pipeline/enrichment.md`, ADR 0043).
- **Two enrichment modes share one persist library.** `enrichment-shared`
  (`enrich_and_persist/*`) is reused by both `backfill-enrichment-runner` (bulk)
  and `enrichment-worker` (live SQS). The HTTP fetchers (`Sep1Fetcher`,
  `NftTokenUriFetcher`) are storage-agnostic — reused verbatim; only the CH write
  path is new.
- **Dictionaries are an established pattern** here: `transaction_hash_dict`
  (`init.sql:411`), with a known gotcha that a `CLICKHOUSE`-sourced dictionary
  opens an inner CH→CH client connection needing its own auth (`users.d/dict.xml`).
- **Mutations are avoided project-wide** (task 0228 approved plan: _"mutations
  are heavy … Plan uses staging copy + EXCHANGE TABLES instead"_).
- **CH version 26.3.10.60** (`docker-compose.yml`).

---

## 4. Methodology — one simulation, every option, every problem

A single script (`/tmp/ch_enrich_sim.sh`, full text + raw output in the Appendix)
runs against the live local CH 26.3 and exercises each option through the
problem-mirroring scenarios:

- **Scenario A — the core race (P1, P3, P5, P6):** indexer inserts
  `(supply=100, icon=NULL)` → enricher inserts `(supply=NULL, icon='LOGO')` →
  indexer re-inserts `(supply=200, icon=NULL)`; force the merge; read. **Pass =
  `supply=200` (latest) AND `icon='LOGO'` survives.**
- **Scenario B — concurrent rebuild (P7):** snapshot 5000 rows into a staging
  table, 5000 _more_ rows arrive in the live table, then `EXCHANGE TABLES`; count
  survivors. **Pass = 10000 (none lost).**
- **Scenario C — NFT twist (P9):** ownership re-writes with a bumped
  `owner_ledger` and metadata `NULL`, interleaved with an enrichment write; read.
  **Pass = latest owner AND metadata survives.**
- **Read layer (P8):** dictionary `dictGet` exercised under the function path.
- **P4** is the _cause_ (why the indexer re-writes whole rows at all) and is
  reflected by Scenario A's indexer re-insert step.

Every MergeTree-family engine is tested (Replacing ±version, Aggregating,
Coalescing, Collapsing, Summing, plain) plus the non-engine alternatives
(side-table+join, staging+EXCHANGE, mutation, dictionary).

---

## 5. Decision matrix — all options, measured

| Option                                                        | Mechanism                            | Scenario A (`supply,icon`) | Other measured                | Verdict                                      |
| ------------------------------------------------------------- | ------------------------------------ | -------------------------- | ----------------------------- | -------------------------------------------- |
| RMT (no version) — real `assets`                              | whole-row re-insert, dedup           | `200, NULL`                | —                             | ❌ icon clobbered (P5/P6)                    |
| RMT (+version)                                                | whole-row re-insert, highest version | `200, NULL`                | —                             | ❌ indexer version newer → clobbered         |
| Mutation `ALTER…UPDATE`                                       | rewrite parts in place               | `200, NULL`                | heavy part rewrite            | ❌ clobbered by next re-insert; avoided (P2) |
| CollapsingMergeTree                                           | sign +1/−1 cancel pairs              | `200, NULL`                | —                             | ❌ whole-row; needs read-prior to cancel     |
| VersionedCollapsingMergeTree                                  | Collapsing + version                 | (same class)               | —                             | ❌ whole-row; only adds ordering             |
| SummingMergeTree                                              | sum numeric per key                  | `300, NULL`                | supply **summed** (100+0+200) | ❌ wrong for non-additive + strings          |
| plain MergeTree                                               | no dedup                             | 3 rows for one key         | —                             | ❌ no merge at all                           |
| **AggregatingMergeTree + `SimpleAggregateFunction(anyLast)`** | per-column last-non-NULL             | **`200, LOGO`**            | plain INSERT works            | ✅ in-place finalist (mature engine)         |
| **CoalescingMergeTree**                                       | per-column last-non-NULL             | **`200, LOGO`**            | NFT: `owner=200, name kept`   | ✅ in-place finalist (modern engine)         |
| **Side table + read `LEFT JOIN`**                             | two tables, join on read             | **`300, LOGO`**            | main untouched                | ✅ isolation finalist                        |
| Dictionary + `dictGet`                                        | in-RAM read layer                    | `dictGet → LOGO`           | needs source-conn auth        | ⚠️ read-optimization over the side table     |
| Staging + `EXCHANGE TABLES`                                   | rebuild whole table, swap            | —                          | **5000/10000 lost**           | ❌ batch-only; unsound live (P7)             |
| Kafka/Flink pre-merge                                         | join streams before CH               | —                          | not simulable (no infra)      | ❌ infra overkill + latency mismatch         |

---

## 6. Per-option analysis

### Disqualified — whole-row / no-merge engines (fail P6)

- **ReplacingMergeTree (±version), CollapsingMergeTree,
  VersionedCollapsingMergeTree.** All keep/replace the **whole** row per key, so a
  partial enricher insert (`icon` set, `supply` NULL) is beaten by the indexer's
  next whole-row insert (`supply` set, `icon` NULL). Measured `200, NULL` — icon
  wiped. Collapsing/Versioned additionally need the writer to emit a canceling
  `-1` row, which requires **reading the prior full row** — exactly what the live
  indexer path avoids (P4).

- **SummingMergeTree.** Built to **sum** numeric columns on merge: measured
  `supply = 300` (100+0+200) instead of the latest `200`, and it cannot carry
  text columns (logo/name) as "latest non-NULL". Wrong tool.

- **plain MergeTree.** No same-key merge at all — all three inserts coexist
  (measured 3 rows for one key). Only used as the _target_ of the streaming
  option, where the merge happens before the DB.

### Disqualified — non-engine mechanisms

- **Mutation (`ALTER … UPDATE`) / lightweight UPDATE (P2).** Set the logo, but
  the next indexer re-insert (whole row, `icon=NULL`) clobbered it under FINAL
  (measured `200, NULL`) — **the clobber is the decisive disqualifier, not the
  speed.** On heaviness: at 200k rows an `ALTER UPDATE` of one row measured ~0.07 s,
  the _same_ as a single INSERT (sim v2) — the cost is a **scale** property
  (a mutation rewrites whole parts, so it bites at multi-GB/TB part sizes, which
  is why task 0228 rejected mutations at multi-TB scale), not visible at 200k.
  Avoided project-wide regardless.

- **Staging rebuild + `EXCHANGE TABLES` (P7).** This is what task 0231's current
  plan proposes (reusing `repair_tier1` / `asset_aggregates`). Measured: with
  5000 rows arriving during the build, `EXCHANGE` left **5000 of 10000** rows —
  the concurrent inserts were destroyed. Correct **only** for a frozen,
  no-concurrent-ingest batch (why `asset_aggregates` runs post-ATTACH), **never**
  for live enrichment.

- **Kafka/Redpanda + Flink/Benthos pre-merge.** Buffer both streams, join per key
  in a time window, write complete rows to a plain `MergeTree`. Rejected on
  architecture (not simulable — no such infra exists): adding Kafka/Flink would be
  a net-new streaming tier (the stack already has AWS SQS+Lambda for the
  trigger — no need for a second queue technology), and enrichment is an async HTTP
  fetch (SEP-1 TOML / IPFS) that is often slow / retried / fails for hours, so a
  short windowed join frequently won't have the value ready. Reserved for
  extreme-throughput enterprise systems.

### Viable — the partial-merge engines and the side table

- **AggregatingMergeTree + `SimpleAggregateFunction(anyLast, …)`.** Each writer
  does a **plain INSERT** with its own columns set, the rest `NULL`; `anyLast`
  keeps the **last non-NULL** value per column. Measured `200, LOGO` — both
  writers' columns survive. Correction to an earlier assumption: `SimpleAggregate`
  columns accept raw values on INSERT, so **the indexer does NOT need to emit
  special aggregate states** (that caveat applies to full `AggregateFunction`,
  not `SimpleAggregateFunction`). Cost: declare the columns as
  `SimpleAggregateFunction(anyLast, T)`, migrate the table engine
  `RMT → AggregatingMergeTree`, and read with `FINAL`/`GROUP BY`. Upside vs
  CoalescingMergeTree: a **mature, long-established** engine.

- **CoalescingMergeTree (CH 25.6+, present in 26.3).** Same behaviour with
  **plain `Nullable` columns** (no aggregate wrapper) — purpose-built for partial
  updates. Measured `200, LOGO` for assets and `owner=200, owner_ledger=80,
name='CoolNFT'` for the NFT case — i.e. it resolves **P9** too: the ownership
  columns and the enrichment columns coalesce **independently**, so the
  ownership clock is never hijacked. Cost: migrate the core table engine onto a
  **newer** engine.

- **Side table + read-time `LEFT JOIN`.** `assets`/`nfts` stay untouched (RMT);
  the enricher only INSERTs into a separate `*_enrichment RMT(version)`; the API
  joins it on read. Measured: after the indexer re-wrote `main` three times
  (`supply` 100→200→300), the join still returned `icon='LOGO'` — the enrichment
  is in a table the indexer never touches, so **P5 cannot happen by
  construction**, and the enrichment table can carry its own `version`, so it is
  also **order-safe and clearable** (§7). The read-path change lands in task
  0243's `ASSET_CH_SELECT` (`crates/api/src/assets/queries_ch.rs:73`). **Read cost
  (corrected by the adversarial panel):** an early sim showed a lucky `16,384`-row
  page, but on a _realistic_ list-page keyset the join reads **~400–452k rows /
  ~110 ms** (and a manual keyset bound did **not** rescue the range shape) — vs
  ~249k for a CMT single table. So the join is the heaviest read of the finalists;
  **trivial on the ~13k-row `assets`**, adequate on the 1M+ `nfts`, and
  optimisable later (dictionary / refreshable MV). Use `FINAL`/`argMax`
  explicitly — a bare join over a non-dedup right side fans out duplicate rows.

- **Dictionary + `dictGet` (read-optimization, not a write strategy).** Verified
  on 26.3: `CREATE DICTIONARY … LAYOUT(…)` + `dictGet` returns `'LOGO'` once the
  source `CLICKHOUSE(... USER … PASSWORD …)` connection auth is supplied (the
  inner CH→CH connection that initially failed `AUTHENTICATION_FAILED`). It still
  needs a **source table underneath** (i.e. the side table), refreshes on a
  `LIFETIME` interval (**staleness window** — unsuitable for instantly-visible
  enrichment; measured in sim v2: after updating the source, `dictGet` returned
  the **stale** value until `SYSTEM RELOAD DICTIONARY`), holds the dict in **RAM**
  (fine for ~thousands of assets, heavy
  for 1M+ NFTs), and is **server-side config** (not managed by the readonly
  `api_reader`). **Role: an optional RAM-speed read layer over the side table**,
  same bucket as a refreshable materialized view; never the primary mechanism.

---

## 7. The finalists — head-to-head (revised after the adversarial panel, §7b)

Three designs survive the happy-path tests. But a three-reviewer adversarial
panel (§7b) broke the in-place engines on correctness axes the early sims never
probed, and corrected the read-cost figures. The revised picture:

|                                                   | CoalescingMergeTree                                       | AggregatingMergeTree(anyLast)                        | **Side table + read-join**                                                              |
| ------------------------------------------------- | --------------------------------------------------------- | ---------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Merge                                             | per-column last-non-NULL (plain cols)                     | per-column last-non-NULL (`SimpleAggregateFunction`) | none — separate tables                                                                  |
| Survives clobber (P5/P6)                          | ✅                                                        | ✅                                                   | ✅ (by construction)                                                                    |
| **Order-safety** (out-of-order / retried inserts) | ❌ **block-order wins → stale regression**                | ❌ same staleness                                    | ✅ **order-safe** (own version clock)                                                   |
| **Can clear a column → NULL**                     | ❌ (NULL ignored; needs `''` sentinel + read translation) | ❌ same                                              | ✅ (new version row)                                                                    |
| **Same-column precedence** (`name`, both writers) | ❌ arrival-order, non-deterministic                       | ❌ same                                              | ✅ deterministic `COALESCE(e,m)`                                                        |
| NFT (P9)                                          | ⚠️ plain CMT only; `CMT(version)` **corrupts P9**         | ⚠️ same                                              | ✅ clean (separate table)                                                               |
| Delete safety                                     | ❌ predicate DELETE resurrects partial rows               | ❌ same                                              | ✅ (RMT whole-row)                                                                      |
| Projections                                       | ❌ blocked on CMT (Code 344)                              | ✅                                                   | ✅                                                                                      |
| Read cost (50-row list page, measured)            | ~249k rows / 27 ms (single table, no join)                | ~ same                                               | ~400–452k rows / 111 ms (join) — but `assets` is ~13k rows in reality, so trivial there |
| Core-table migration                              | ⚠️ `RMT → CMT` (newer engine, 25.6)                       | ⚠️ `RMT → AggregatingMergeTree` + column types       | ✅ **none**                                                                             |
| Writer isolation                                  | ✗ enricher writes the core table                          | ✗ same                                               | ✅ total isolation                                                                      |

**The early "all three equivalent / CMT cleanest" framing was wrong.** Only the
side table is order-safe, clearable, and P9-clean _by construction_; the in-place
engines win only on read latency (and only materially for the 1M+ `nfts` table —
for the ~13k-row `assets` the join is trivial).

### 7b. Adversarial panel — three independent reviewers (all measured on CH 26.3)

- **Reviewer 1 (CMT production-safety) — broke in-place determinism.** Plain
  CMT (and `SimpleAggregateFunction(anyLast)`) resolve per-column by **part/block
  insertion order**, not by a data clock: `INSERT(supply=200)` then a
  later-but-stale `INSERT(supply=100)` → `FINAL` returns **100** (a retried /
  out-of-order indexer insert regresses `total_supply`). The escape — a version
  arg `CoalescingMergeTree(version)` — **corrupts P9** (NFT `name` wiped / owner
  stale, measured). `assets` has **no version column** (`init.sql:166`) to lean
  on. The side table is immune (its RMT(version) clock and the enrichment never
  share a merge). Also proven: PROJECTIONs blocked on CMT (Code 344), predicate
  `DELETE` resurrects partial-column rows, and a non-NULL `''` clobbers a real
  value (live trap if the indexer ever emits `Some("")`). Survived: replication
  exists (`ReplicatedCoalescingMergeTree`), no-FINAL split doesn't bite (assets
  read uses `FINAL`), clobber-survival and the batch staging+EXCHANGE migration
  hold.
- **Reviewer 2 (semantics) — "NULL can't clear" + non-deterministic
  same-column precedence.** In-place engines cannot reset a column to NULL (stale
  logo immortal when an issuer removes it); the side table can. Both are roughly
  **parity with PG** (PG's `COALESCE(NULLIF(…))` is also sticky, and PG's `name`
  precedence is also last-writer), so this is a _capability_ gap, not a
  regression — but the side table is strictly more capable. Confirmed
  `anyLast` reliably skips NULL; confirmed `LEFT JOIN … FINAL` / `argMax` don't
  duplicate rows (but a **bare** join over a non-dedup right side fans out — the
  read MUST use `FINAL`/`argMax` explicitly).
- **Reviewer 3 (premise) — partly a CATEGORY ERROR, partly valid.** Reviewer 3
  argued the live race is "self-inflicted" because _"the CH stack has no
  SQS/Lambda, so live per-row enrichment has no trigger — only batch is
  buildable."_ **That claim is WRONG** and was corrected post-review: SQS + Lambda
  live on **AWS**, not on the ClickHouse database. The indexer is already an
  **AWS Lambda that writes to ClickHouse-on-Hetzner via mTLS**
  (`crates/indexer/src/main.rs:4`), and the `enrichment-worker` is **already a
  Lambda** (SQS event source) — today pointed at PG
  (`crates/enrichment-worker/src/main.rs`), and the indexer **already publishes
  the enrichment SQS messages** (currently stubbed post-CH-cutover,
  `indexer/src/handler/enrichment_publish.rs`). So **live per-row enrichment IS
  fully buildable on CH** — task 0231 is exactly "repoint the enrichment-worker's
  persist path PG → CH". Reviewer 3 conflated the Hetzner DB with the AWS compute
  layer. → The live race (P5/P6/P7) is **real and the intended model**, not an
  artefact; staging+EXCHANGE is genuinely the wrong fit for it (works only in a
  frozen window). What **survives** from Reviewer 3: (1) the **read-cost
  correction** — the round-2 "16,384 / cheap" was a non-representative shape; on a
  realistic list-page keyset the side-table join reads **400–452k rows** (the
  manual keyset bound did **not** rescue the range shape) vs **249k** for a CMT
  single table — heavier, but adequate for a paginated API and trivial on the
  small `assets`; (2) **0231's `ch_enrichment_queue` table is likely redundant** —
  the existing AWS SQS publish + Lambda is the trigger, so a CH-side work-queue
  re-implements something SQS already provides.

---

## 8. Recommendation

_(Revised after the adversarial panel — the earlier "in-place CMT preferred"
ranking was overturned.)_

1. **Adopt the side table as the DEFAULT** — a separate `*_enrichment`
   `ReplacingMergeTree(version)`, written only by the enrichment runner, read via
   `LEFT JOIN … FINAL` / `argMax`. It is the **only** finalist that is
   order-safe under retried/out-of-order inserts, can clear a column, gives
   deterministic same-column precedence, is P9-clean, and needs **zero
   core-engine migration** (consistent with the project's RMT-everywhere /
   no-mutations conservatism). Its only real cost is a heavier read-join — trivial
   on the ~13k-row `assets`, adequate (≈450k rows / ~110 ms per list page) on the
   1M+ `nfts`, and optimisable later with a dictionary / refreshable MV if needed.
2. **In-place partial-merge (CoalescingMergeTree / AggregatingMergeTree) is NOT
   recommended** unless read latency proves unacceptable AND the team explicitly
   accepts: block-order non-determinism (stale `total_supply`/owner under retries,
   no version fix that also keeps P9), `''`-sentinel clearing + read translation,
   predicate-DELETE resurrection, the CMT projection block, and a core-table
   engine migration. If forced, prefer `AggregatingMergeTree(anyLast)` (mature)
   over CMT (newer) — but the determinism trap applies to both.
3. **The intended model is live per-row, and it IS buildable on CH.** Live
   enrichment runs as an **AWS Lambda** (SQS-triggered) writing to
   ClickHouse-on-Hetzner via mTLS — exactly as the indexer Lambda already does
   (`indexer/src/main.rs`); the `enrichment-worker` Lambda exists today (on PG)
   and the indexer already publishes the SQS messages (currently stubbed). So 0231
   = repoint that worker's persist path PG → CH. This means the live race
   (P5/P6/P7) is **real**, so the storage MUST be concurrency-safe → the side
   table. `staging + EXCHANGE` is genuinely **unsound for the live model** (works
   only in a frozen window); a pure-batch alternative is possible but is not the
   design. (Earlier text claimed "CH has no trigger / only batch buildable" — that
   was a category error confusing the Hetzner DB with the AWS compute layer; now
   corrected.)
4. **0231's `ch_enrichment_queue` table is likely redundant.** The existing AWS
   **SQS** publish (indexer) + the enrichment-worker Lambda **are** the trigger and
   the work queue — a CH-side queue table with attempt-counters/backoff
   re-implements what SQS already provides. Reuse the SQS path; the side table
   holds only the enriched values, not the work-queue state.
5. **One mechanism for both modes**, **reuse the HTTP fetchers verbatim**; only
   the CH write path (INSERT into the side table) is new.

### Integration / follow-ups

- **0243 coupling:** until enrichment is populated, CH `assets.{icon_url,name}`
  read as NULL → the `ASSETS=ch` prod-flip stays gated on 0231 (already noted in
  task 0243). The side-table option additionally changes `ASSET_CH_SELECT`.
- **Local seeding for scale testing:** local PG is seeded (ledgers
  50944000–50967331) but the downloaded partitions are not cached. Seeding local
  CH with real data needs a partition re-download + `backfill-runner
--datasource clickhouse`. The simulation above needed no real data; real data
  is only required to confirm the two scale caveats below.
- **Scale caveats to confirm on prod-shaped data:** (a) the side-table join read
  is **~400–452k rows / ~110 ms per 50-row list page** on a realistic keyset
  (the manual bound did not help) — trivial on the ~13k-row `assets`, but
  **measure it on the 1M+ `nfts`** under the read quota before committing the join
  shape (consider a dictionary / refreshable MV if it bites); (b) `FINAL` cost at
  production part-count (the sims used few parts).

---

## 9. Summary

The task looked like "rewrite the enrichment UPDATE from Postgres to ClickHouse
SQL." It is not. **ClickHouse has no cheap per-column UPDATE**, and the
enrichment is a _second_ writer setting _disjoint_ columns of a row that a
_continuous, read-modify-write_ indexer keeps re-writing whole — with the
enrichment columns `NULL`. That single fact (problems P1–P9) disqualifies every
"edit the row in place on the default engine" approach, all measured:

- **ReplacingMergeTree (±version), Collapsing, Versioned, mutations, lightweight
  UPDATE** → the next indexer write clobbers the logo (`200, NULL`).
- **SummingMergeTree** → sums instead of replacing (`300`); **plain MergeTree** →
  no merge (3 rows).
- **Staging + EXCHANGE** (0231's current plan) → loses every row inserted during
  the swap (**5000/10000**); correct only for a frozen batch.
- **Kafka pre-merge** → infra overkill + async-fetch latency mismatch.

Three designs survive the clobber test — but the adversarial panel (§7b) then
separated them on correctness:

- **Side table + read-join — the DEFAULT.** A separate `*_enrichment
RMT(version)`, written only by the enricher, joined on read. The **only**
  finalist that is order-safe (retries/out-of-order can't regress it), clearable
  (can reset a column to NULL), P9-clean, and needs **zero core-engine
  migration**. Cost: the heaviest read-join (~450k rows / ~110 ms per list page)
  — trivial on the ~13k-row `assets`, adequate on 1M+ `nfts`, optimisable later.
- **CoalescingMergeTree / AggregatingMergeTree(anyLast) — NOT recommended.** Both
  do per-column "latest non-NULL" with plain inserts and read cheaper (single
  table, no join), but both are **block-order non-deterministic** (a retried
  indexer insert regresses `total_supply`/owner; the `(version)` fix corrupts the
  NFT P9 case), **cannot clear a column**, resurrect rows on predicate DELETE, and
  require a core-table engine migration. Reserve only for a proven read-latency
  problem the side table can't meet.

The arc of this research: a naive "rewrite the UPDATE" → measured that every
in-place / whole-row / mutation approach is clobbered → an external consult
surfaced the partial-merge engines (CMT / Aggregating) which looked "preferred"
→ a three-reviewer adversarial panel then **overturned that**, proving the
in-place engines are order-unsafe, can't-clear, and DELETE-unsafe. (One panel
claim — "live enrichment can't even be triggered on CH" — was itself a category
error and is retracted: the live worker is an **AWS Lambda + SQS** writing to
CH-on-Hetzner via mTLS, exactly like the indexer, so **live per-row enrichment is
fully buildable** and is the intended model — which makes the concurrency race
real and the concurrency-safe side table the right call.) Net: **a side table
(separate `*_enrichment RMT(version)`), written by the live AWS Lambda over the
existing SQS path, read via a FINAL/argMax join, is the recommended design**;
task 0231's `staging + EXCHANGE` plan is **unsound for this live model** (it works
only in a frozen window), and 0231's CH-side queue table is likely **redundant**
given AWS SQS already provides the trigger and work queue.

---

## Appendix — simulation script and raw output

Script: `/tmp/ch_enrich_sim.sh` (creates `enrich`-prefixed sandbox tables on the
local CH, runs each scenario, drops everything). Run against
`http://localhost:8125` (local docker CH 26.3, user `default`).

Measured output (2026-06-07, CH 26.3.10.60):

```text
SCENARIO A (race): indexer(supply100,icon NULL) -> enricher(icon,supply NULL) -> indexer(supply200,icon NULL); want supply=200 AND icon survives
  D1 RMT(no version)        supply,icon = 200   \N      ❌ icon lost
  D1 RMT(version)           supply,icon = 200   \N      ❌ icon lost
  D5 Aggregating(anyLast)   supply,icon = 200   LOGO    ✅
  CMT CoalescingMergeTree   supply,icon = 200   LOGO    ✅
  CollapsingMergeTree       supply,icon = 200   \N      ❌
  SummingMergeTree          supply,icon = 300   \N      ❌ supply summed
  plain MergeTree           rowcount for k=1 = 3        ❌ no dedup
D3 side+join                supply,icon = 300   LOGO    ✅ (main re-written 3x, icon survives)
D2 staging+EXCHANGE         live rows = 5000 (expected 10000)  ❌ 5000 concurrent rows lost
D4 mutation then reinsert   supply,icon = 200   \N      ❌ clobbered
DICT dictGet(icon,k=1)      = LOGO                      ✅ (read layer; source-conn auth required)
NFT CMT owner,ledger,name   = 200  80  CoolNFT          ✅ ownership latest + metadata survives
```

### Round 2 — sim v2 (`/tmp/ch_enrich_sim_v2.sh`): concurrency, readonly, scale, staleness

Added to close the gaps in round 1 (true concurrency vs sequential; the real
readonly profile; scale/read-cost; dictionary staleness; mutation timing; NFT
for all finalists; read-modify-write). Measured output (2026-06-08, CH 26.3):

```text
(3) read cost 200k, 50-row page:  unbounded join read_rows = 16384 ; keyset-bounded = 16384
    -> CH pushed the page predicate into the subquery; bound is a safety net, not a rescue
(2) readonly=1 user: FINAL+join OK (200000) ; argMax OK (200000) ;
    `SETTINGS max_threads=2` -> Code 164 READONLY (rejected)   ✅ P8 confirmed
(5) mutation heaviness: ALTER UPDATE 1 row / 200k = 0.070 s  ==  single INSERT 0.070 s
    -> not heavy at 200k; heaviness is a scale property; the clobber is the real disqualifier
(4) DICT staleness: initial 'logoV1' -> source updated, dictGet still 'logoV1' (STALE)
    -> after SYSTEM RELOAD DICTIONARY -> 'logoV2'                ✅ staleness window real
(1) true concurrency: 1500 background inserts during staging build+EXCHANGE -> survived 1498, LOST 2
    -> EXCHANGE loses rows under genuine concurrency; magnitude = rows landing in the snapshot↔swap window
(7) NFT P9 beyond CMT: side-table owner,name = 200,CoolNFT ; AggregatingMergeTree = 200,CoolNFT  ✅
(6) P4 read-modify-write: supply = 110 (read 100 + delta 10) via sub-SELECT  -> CH can RMW but needs a read
```

**Corrections from round 2 (intellectual honesty):** the side-table join is
**cheap** here (predicate pushdown — not the "452k-row scan" seen on a different
shape in round 1), and a **mutation is not visibly heavier than an insert at
200k** (its cost is a scale property) — so the mutation's disqualifier is the
clobber, not the speed. The decisive verdicts are unchanged and now measured
end-to-end (clobber, EXCHANGE loss under real concurrency, readonly
compatibility, P9 for all three finalists).
