---
id: '0050'
title: 'Separate ClickHouse tables for indexer-owned vs externally-enriched columns (no two-writer column mixing)'
status: accepted
deciders: [karolkow]
related_tasks: ['0231', '0243']
related_adrs: ['0044', '0047']
tags: [clickhouse, enrichment, schema, write-strategy, side-table]
links:
  - lore/1-tasks/active/0231_FEATURE_clickhouse-sep1-nft-enrichment/notes/R-clickhouse-enrichment-write-strategy.md
history:
  - date: '2026-06-08'
    status: accepted
    who: karolkow
    note: >
      Created post-research (task 0231). Three simulation rounds on live CH 26.3
      + three adversarial reviews. Decision: enrichment columns live in separate
      side tables, never mixed with indexer-owned columns in one table.
---

# ADR 0050: Separate ClickHouse tables for indexer-owned vs externally-enriched columns

**Related:**

- [Task 0231: ClickHouse SEP-1 + NFT enrichment](../1-tasks/active/0231_FEATURE_clickhouse-sep1-nft-enrichment/README.md)
- [Task 0243: API read path PG→CH](../1-tasks/active/0243_FEATURE_api-feature-flag-pg-to-ch-per-module.md)
- Full measured evidence: [research note](../1-tasks/active/0231_FEATURE_clickhouse-sep1-nft-enrichment/notes/R-clickhouse-enrichment-write-strategy.md)

---

## Context

Some columns on `assets` / `nfts` are **off-chain enrichment**:
`assets.{icon_url, name}` come from the issuer's SEP-1 `stellar.toml`;
`nfts.{name, media_url, collection_name}` come from a contract `token_uri()` +
IPFS. They are fetched over HTTP by the enrichment worker — **not derivable from
the ledger**, so the indexer cannot fill them.

This means **two independent writers target the same row**:

1. the **indexer** (AWS Lambda, live, continuous) — writes the on-chain columns;
2. the **enrichment worker** (AWS Lambda, SQS-driven, concurrent) — writes the
   off-chain columns.

On Postgres this was harmless: a per-column `UPDATE … SET icon_url = …` touches
only its own columns, and MVCC + row locks let the two writers compose. The
indexer never even names `icon_url`, so it cannot overwrite it.

ClickHouse breaks this (problems P1–P9 in the research note). The load-bearing
ones:

- **No cheap per-column UPDATE.** Data lives in immutable parts; the only way to
  "change" a row is to INSERT a new one and let a background merge reconcile
  same-key rows. (`ALTER … UPDATE` mutations rewrite whole parts and are avoided
  project-wide.)
- **Whole-row replace, no per-column merge** on the default engine
  (`ReplacingMergeTree`). A partial write NULLs out the other writer's columns.
- **The indexer is not append-only and re-writes rows continuously.** It
  re-emits the whole asset/NFT row (with the enrichment columns NULL, because it
  has no off-chain data) every ledger the entity is active — `assets` is
  `ReplacingMergeTree` with **no version column**, so the latest insert wins.

**Concrete failure if enrichment is written into the shared table (measured):**
the enrichment worker writes USDC's logo into the `assets` row; within seconds
USDC is transferred, the indexer re-inserts the USDC row with `icon_url = NULL`,
the merge keeps the newest row → **the logo disappears**. Simulation round 1
reproduced exactly this (`supply = 200, icon = NULL`). For high-activity tokens
the logo would flicker/vanish constantly.

---

## Decision

**Externally-enriched columns live in their own ClickHouse tables, never mixed
into the indexer-owned table.** Concretely:

- `asset_enrichment` (keys = `assets` ORDER BY tuple + `icon_url, name, version`)
  and `nft_enrichment` (keys = `nfts` ORDER BY tuple + `name, media_url,
collection_name, version`), both `ReplacingMergeTree(version)`, in the **same
  database**, sitting next to `assets` / `nfts`.
- Written **only** by the enrichment worker / batch runner; the indexer never
  touches them.
- The API joins them at read time (`assets a LEFT JOIN ( … argMax(_, version) …
GROUP BY key ) ae`, the sub-aggregate collapsing the RMT to one latest row per
  key so the join can't multiply rows). The shipped name composition is the
  evolved **Option C** — a single owner per `asset_type`, composed disjointly:
  `coalesce(nullIf(ae.name,''), nullIf(sc.name,''), if(asset_type=0,'Stellar
Lumen',NULL))` (classic/SAC → `asset_enrichment.name`; soroban →
  `soroban_contracts.name`; native → literal). NOTE: the read no longer falls
  back to the indexer-owned `assets.name`/`assets.icon_url` at all — those
  columns are dropped (0231 step 8 / 0301), so a populated `asset_enrichment` is
  a hard prerequisite of the `ASSETS=ch` read flip (see 0301 ordering gate).

**Generalised rule:** on ClickHouse, when two independent writers own **disjoint
columns** of the same logical entity, give each writer its **own table** keyed
identically and join at read — do **not** share one table. Mixing two-writer
columns in one CH table is an anti-pattern, because CH cannot merge per-column
and the continuous writer clobbers the other.

---

## Rationale

The side table is the only option that satisfies every constraint **by
construction**, with measured evidence (research note §5–§7):

- **No clobber.** The indexer never writes the enrichment table, so a concurrent
  whole-row rewrite of `assets`/`nfts` cannot touch enrichment. (P5/P6 removed by
  construction.)
- **Order-safe.** The enrichment table carries its **own** `version` clock,
  isolated from the indexer's merge — so a retried / out-of-order insert cannot
  regress a value (the failure mode that disqualified the in-place engines).
- **Clearable.** A later run can reset a value to NULL with a higher `version`
  (e.g. an issuer removes their logo) — the in-place engines cannot clear.
- **Zero core-engine migration.** `assets`/`nfts` keep `ReplacingMergeTree`,
  consistent with the project's "RMT everywhere / no mutations" convention.
- **Cost is bounded and local:** one read-time `LEFT JOIN` (trivial on the
  ~13k-row `assets`; measurable but adequate on the 1M+ `nfts`, optimisable later
  with a dictionary / refreshable MV).

---

## Alternatives Considered

### Alternative 1: Write enrichment in-place via `ReplacingMergeTree` re-insert (naive PG port)

**Description:** keep enrichment on `assets`/`nfts`; "update" by re-inserting the
row.

**Cons:** measured clobber — the indexer's continuous whole-row re-insert (icon
NULL) wins the merge; `assets` has no version column so the winner is even
nondeterministic.

**Decision:** REJECTED — loses enrichment for any active entity (the USDC example).

### Alternative 2: `ALTER TABLE … UPDATE` mutations / lightweight UPDATE

**Description:** use CH mutations to set the columns.

**Cons:** rewrite whole parts (heavy at scale), async, avoided project-wide
(task 0228 rejected them); and the next indexer re-insert clobbers the result
anyway.

**Decision:** REJECTED — clobbered + heavy.

### Alternative 3: Migrate `assets`/`nfts` to `CoalescingMergeTree` (or `AggregatingMergeTree(anyLast)`)

**Description:** a partial-merge engine that keeps the latest non-NULL value per
column, so both writers' columns survive. **This WOULD fix the clobber example.**

**Pros:** in-place (one table), cheapest reads (no join), solves the immediate
clobber.

**Cons (measured by the adversarial panel):** (a) **block-order
non-determinism** — without a version, a retried / out-of-order indexer insert
regresses non-version columns (e.g. `total_supply` reverts); adding a version arg
corrupts the NFT ownership clock. (b) **Cannot clear** a column to NULL. (c)
**Predicate DELETE resurrects** partial-column rows; **PROJECTIONs are blocked**
on CMT. (d) Requires **migrating a core table onto a newer engine** and aligning
the whole indexer write + aggregate pipeline.

**Decision:** REJECTED as the default — solves clobber but trades it for
order-unsafety + can't-clear + a risky core migration that the side table avoids.
(Reserved as a fallback only if read-join latency ever proves unacceptable.)

### Alternative 4: Batch rebuild via staging table + `EXCHANGE TABLES`

**Description:** the original task 0231 plan — rebuild the table with enrichment
folded in, then atomically swap (reusing the `repair_tier1` / `asset_aggregates`
pattern).

**Cons:** measured to **lose every row inserted during the build→swap window**
(5000/5000 under concurrency). Correct only for a frozen, no-concurrent-ingest
batch — not for live enrichment.

**Decision:** REJECTED for steady state (the live indexer never pauses).

### Alternative 5: Dictionary (`dictGet`) / refreshable materialized view

**Description:** serve enrichment from an in-RAM dictionary or a materialised
denormalised view.

**Cons:** both are **read-optimisation layers over a source table** (still need
the side table underneath); both carry a staleness window; the dictionary needs
RAM + server-side config.

**Decision:** NOT a write strategy — optional read accelerators over the side
table, deferred until profiling shows the join is too costly.

### Alternative 6: Stream pre-merge (Kafka/Redpanda + Flink)

**Description:** join the two event streams before ClickHouse and write complete
rows.

**Cons:** net-new infra; enrichment is an async HTTP fetch (often slow/retried
for hours), so a short windowed join cannot reliably have the value ready.

**Decision:** REJECTED — over-engineering + latency-model mismatch.

---

## Consequences

- **Read path change (task 0243).** `crates/api/src/assets/queries_ch.rs`
  `ASSET_CH_SELECT` (and the nfts read) gain the enrichment `LEFT JOIN` +
  `COALESCE`. Until enrichment is populated, those columns read NULL — this gates
  the `ASSETS=ch` production flag flip on task 0231.
- **Enrichment write path** is an INSERT into the side table (`version = now_ms`,
  RMT keeps the latest) — reusing the existing AWS SQS + Lambda trigger (the
  indexer already publishes the messages); no CH-side work queue.
- **Single owner per value (provenance audit, code + Stellar protocol).** The
  indexer can only derive ONE of the enrichment-adjacent values from ledger XDR:
  the **soroban** token name (SEP-41 `name` in a `CONTRACT_DATA` instance-storage
  entry, no RPC), already stored in `soroban_contracts.name`. Everything else —
  classic/SAC names, all icons, all per-token NFT metadata — is **off-chain only**
  (SEP-1 TOML / `token_uri` JSON). So the indexer's `assets.{name,icon_url}` and
  `nfts.{name,media_url,collection_name}` are **always-`None` placeholders** and
  are dropped (the side tables own those values; `soroban_contracts.name` owns the
  soroban name; native is an API constant). The read composes the disjoint
  single-owner sources — no column lives in two tables, so there is no redundancy
  and no two-writer column anywhere.
- **Reusable precedent.** Any future "two independent writers, disjoint columns,
  same entity" case on ClickHouse should follow this rule (separate table +
  read-join), not share a table.
