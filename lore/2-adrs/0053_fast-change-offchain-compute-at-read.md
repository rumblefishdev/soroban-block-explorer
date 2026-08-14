---
id: '0053'
title: 'Fast-change off-chain values on ClickHouse: compute-at-read via local price join (amends 0043)'
status: proposed
deciders: [stkrolikiewicz]
related_tasks: ['0199', '0211', '0231', '0247']
related_adrs: ['0043', '0047', '0029']
tags: [enrichment, clickhouse, schema, prices, read-path, milestone-2]
links:
  - lore/1-tasks/backlog/0199_FEATURE_lp-analytics/notes/S-ch-tvl-enrichment-and-decision.md
history:
  - date: '2026-06-09'
    status: proposed
    who: stkrolikiewicz
    note: >
      ADR created (proposed). Amends ADR 0043 for the fast-change off-chain
      category on the ClickHouse primary store. Pending a read-cost measurement
      of the read-time join at chart/list and review by karolkow (0043 author)
      before proposed → accepted.
  - date: '2026-06-12'
    status: proposed
    who: stkrolikiewicz
    note: >
      Prices-API contract finalized with Oskar. Decision §2 refined: no local
      prices table and no sync job — we read prices.* named views directly
      in-cluster, and the USD pivot is materialized write-time by prices as a
      retention-proof close_usd per grain (read-time computes only the
      TVL/volume multiply). Compute-at-read core unchanged. Detail + two
      prices-side implementation deps (native-key alignment, SAC->classic
      resolver = their 0061) in the linked S-note. Still proposed (read-cost
      measure + karolkow review pending).
  - date: '2026-06-16'
    status: proposed
    who: stkrolikiewicz
    note: >
      Contract final code-side (prices PR #39). Body "Update 2026-06-16" added:
      view name price_usd_series (not price_usd_at), interop pinned (asset_code
      trimmed String, grain-floored DateTime bucket, Decimal(38,14)), live-spot
      view current_price_usd, SAC seam via identity_by_contract, both deps
      shipped. Flags an open item for this ADR: live-band ingestion-lambda
      write-back vs the no-version RMT race partially revisits Decision #1 (no
      write-back) for live only — choice open in 0199. Still proposed.
  - date: '2026-07-22'
    who: karolkow
    note: >
      **Renumbered 0048 → 0053.** This ADR was created 2026-06-09 on an id that
      `0048_cloudflare-edge-over-aws-waf.md` had already taken on 2026-06-02, so
      every bare "ADR 0048" in the repo was ambiguous between an edge-security
      decision and a read-path one. The older claim wins the number: Cloudflare
      is `accepted`, shipped, cited from `crates/api` and from the external SCF
      diagram, and an accepted ADR's id is immutable — this one was still
      `proposed`. 28 references repointed across 13 files (init.sql, stage.rs,
      2 architecture docs, tasks 0199/0261/0266/0283/0297, ADR 0043). Cloudflare
      references verified untouched. Status unchanged: still `proposed`, still
      pending the read-cost measurement + karolkow review.
  - date: '2026-08-04'
    status: proposed
    who: stkrolikiewicz
    note: >
      IMPLEMENTED for the LP chart + detail read paths (0199 Phase A, branch
      feat/0199_lp-analytics): chart joins price_usd_series/_1h at the
      interval's grain, detail prices latest reserves at the last 1h close
      (price_usd_series_1h, 48h lookback) + last-24h volume — NOT
      current_price_usd, whose price_usd=0 sentinel covers native XLM
      (box-verified), which would NULL every XLM-leg pool. Scope extended
      beyond TVL-only to volume/fee_revenue (both premises of the June cut
      are gone — gross_volume_a backfilled by 0266, prices live in-cluster).
      READ-COST MEASURED same day on the hottest pool (see body Update):
      1h/1d/detail acceptable; 1w/104w = 4.6s / 70.7M rows — identity does
      not push into the views; prices-side identity-keyed materialization
      requested (their views.sql §6 trigger). Still proposed pending
      karolkow review. Deploy gate: API CH user needs SELECT on prices.*.
---

# ADR 0053: Fast-change off-chain values on ClickHouse — compute-at-read via local price join

**Related:**

- [Task 0199: LP analytics (TVL + volume + fee_revenue)](../1-tasks/backlog/0199_FEATURE_lp-analytics/README.md)
- [Task 0211: Asset USD price exposure](../1-tasks/backlog/0211_RESEARCH_asset-usd-price-exposure/README.md)
- [Task 0231: CH port of SEP-1 / NFT enrichment](../1-tasks/backlog/0231_FEATURE_clickhouse-sep1-nft-enrichment.md)
- [Task 0247: LP per-tx amounts / `gross_volume_a` source](../1-tasks/active/0247_RESEARCH_lp-per-tx-amounts-xdr-fetch-viability/README.md)
- [ADR 0043: field-allocation rule](./0043_field-allocation-rule.md) — amended
- [ADR 0047: ClickHouse as primary API datastore](./0047_clickhouse-primary-api-datastore.md)

---

## Context

[ADR 0043](./0043_field-allocation-rule.md) routes off-chain, API-visible fields
to a typed column written by the type-1 enrichment Lambda. Its Rationale #2
explicitly assumes off-chain data is **rare-change per row** (SEP-1 TOML, NFT
metadata, LP-at-snapshot). USD price violates that assumption: it changes
per-second, so a stored value is stale within seconds of the write. This gap was
already flagged repeatedly (task 0211 — four pulled attempts to add
`assets.usd_price`).

Two things changed since 0043:

1. **[ADR 0047](./0047_clickhouse-primary-api-datastore.md)** moved the primary
   API datastore to ClickHouse. CH has no row-level `UPDATE`;
   `liquidity_pool_snapshots` is `ReplacingMergeTree` with no version column. A
   write-back "Lambda 2 column" becomes a per-row **read-modify-write of the
   whole row**, with a non-deterministic merge winner — a later plain insert
   (replay / backfill) can silently erase the analytics.
2. The team **Prices API** is confirmed as a sufficient single price source:
   per-asset OHLCV with 1h/1d history back to 2024-02-20, per-candle `vwap`,
   `null` on unknown asset (distinct from transient 5xx).

## Scope

Fast-change off-chain values that are **USD denominations of on-chain
quantities**, surfaced on the ClickHouse primary store: LP `tvl` (and, when they
return, `volume` / `fee_revenue`); asset USD price exposure (0211).

This ADR does **not** change ADR 0043's handling of **rare-change** off-chain
data (SEP-1 TOML, NFT `token_uri` metadata) — those remain type-1 enrichment
columns.

## Decision

For fast-change off-chain values on ClickHouse:

1. **Do not materialize** the USD value into the entity row via a write-back
   worker.
2. Sync the price source once **per asset** into a dedicated local
   **`prices(asset, time_bucket) → usd`** table — the only Prices-API consumer,
   per-asset OHLCV, never per-row.
3. **Compute the USD value at read time** by joining the entity's on-chain
   quantity against `prices` (mapping `ledger → closed_at → price candle`), in
   SQL.

This adds a **fourth path** to ADR 0043's taxonomy — _indexer column_ /
_type-1 enrichment column_ / _runtime type-2 fetch_ / **compute-at-read via local
price join** — specific to fast-change off-chain values on the CH store. The
entity table keeps a **single writer** (the indexer, on-chain price-independent
inputs).

**Launch application (task 0199):** LP **TVL only** =
`reserve_a·price_a + reserve_b·price_b`, computed at read. `volume` /
`fee_revenue` are deferred — they require a per-pool `gross_volume_a` extracted
from PathPayment `claimedOffers` (on-chain), whose historical backfill (XDR
re-parse over 273M snapshots) is tracked in task 0247.

> **Update 2026-08-04 (scope re-cut + implementation).** The TVL-only cut is
> retired: `gross_volume_a` has been live-written since 0261 and historically
> backfilled by 0266 (261.32M rows), so `volume = Σ gross_volume_a·price_a`
> and `fee_revenue = volume·fee_bps/10000` are the same read-time join TVL
> needs. Implemented for the LP chart (grain-matched `price_usd_series`/`_1h`
> join) and detail (last 1h close via `price_usd_series_1h` + last-24h
> volume — NOT `current_price_usd`, whose 0-sentinel covers native XLM as of
> 2026-08-04) in 0199 Phase A. Read-cost box-measured 2026-08-04 on the
> hottest pool (1.87M snapshots): 1h/7d 227 ms / 2.0M rows, 1d/90d 721 ms /
> 10.3M rows (vs 442 ms / 4.2M pre-prices baseline), detail 112 ms / 1.6M
> rows — acceptable; **1w/104w 4.6 s / 70.7M rows / 2.1 GiB is NOT** — the
> views' bucket range prunes the `price_ohlcv_*` scan but the identity
> predicate cannot (computed columns), so long windows scan every asset's
> candles ×2 legs. Their views.sql §6 names the fix: promote the series to
> an identity-keyed materialized table when measured latency demands it —
> that demand is now measured; requested from the prices owner. Still
> `proposed` pending karolkow review + the prices-side materialization for
> the long-window path.

> **Update 2026-06-12 (contract finalized — see [S-note](../1-tasks/backlog/0199_FEATURE_lp-analytics/notes/S-ch-tvl-enrichment-and-decision.md)).**
> Decision §2 is refined: there is **no local `prices` table and no sync job**.
> We read the prices service's `prices.*` **named views directly in the same CH
> cluster**. The asset→quote→USD pivot is **materialized write-time** by prices
> as a retention-proof `close_usd` per grain (tiered oracle-in-window /
> USDC-peg-out-of-window), so read-time computes only the on-chain-quantity ×
> `close_usd` multiply. The compute-at-read core (no entity-row materialization,
> single-writer table, read-time join) is unchanged. The series view is
> **`price_usd_series(identity, bucket)`** keyed by structured natural-identity
> columns, returns NULL + a `ok`/`no_asset_price`/`no_reference` discriminator,
> plus a companion `usd_reference(bucket)` and a live-spot view
> `current_price_usd(identity)`. Two prices-side deps gated coverage: `native`-key
> alignment (XLM legs) and a SAC→classic resolver (their task 0061 → our 0199
> Phase 3).
>
> **Update 2026-06-16 (final — prices PR #39).** Both deps **shipped**. Interop
> pinned: `asset_code` trimmed `String`, `bucket` grain-floored `DateTime`
> (`toStartOfHour`/`toStartOfDay`), `Decimal(38, 14)`. SAC seam: a SAC leg's price
> lives under the **classic** identity, so resolve `contract_address →
prices.identity_by_contract → natural identity → price_usd_series` (the series
> has no SAC-keyed row). **Open for this ADR:** the **live band** may enrich the
> tip via an **ingestion-lambda write-back** — this partially revisits Decision #1
> (no write-back) for live only; version-column / side-table / stay-compute-at-read
> is open (task 0199). Operational, not code: their 0039 live-spot writer +
> production backfill to ledger 50,457,424.

## Rationale

1. **Staleness.** A stored USD value is wrong within seconds; compute-at-read is
   always current to the latest synced candle.
2. **CH engine fit.** No row `UPDATE`; a write-back is a racy read-modify-write on
   `ReplacingMergeTree` (whole-row replacement, no version column → merge-winner
   race can erase reserves/analytics). Single-writer + read-time join removes the
   race class entirely.
3. **Throughput.** Per-asset OHLCV pull + local join avoids N-per-row API calls
   (273M snapshots → tens of thousands of asset-series fetches, cached locally).
4. **Backfill-free for TVL.** Reserves are already stored; only the `prices`
   table is new. No 273M-row materialization.
5. **Generalizes.** The same shape serves asset USD price exposure (0211) and any
   future fast-change off-chain value.

## Alternatives Considered

- **A1 — Materialize via write-back worker (ADR 0043 literal).** REJECTED for CH:
  stale values, `ReplacingMergeTree` read-modify-write race, 273M-row backfill.
  May be revisited as an _optional_ optimization to materialize **historical**
  USD once a day's price is final, if the read-time join cost proves too high —
  non-blocking, and it would carry a version column.
- **A2 — Per-row runtime type-2 fetch (ADR 0043 detail path).** REJECTED: breaks
  list endpoints (N HTTP fetches per request) and prevents sort/filter on TVL.
- **A3 — A second / external price API.** REJECTED: the team Prices API covers
  live + the full history (to 2024-02-20); no second source needed.

## Consequences

### Positive

- Entity tables stay single-writer; no analytics-clobber race.
- No USD backfill; price corrections propagate automatically.
- One pattern for all fast-change off-chain values (LP TVL, asset price, …).

### Negative

- Heavier read path (join + compute per query) — mitigated because chart/list
  endpoints already aggregate. **Must be measured** at chart/list before flipping
  proposed → accepted (the CH read quota applies).
- ~~New `prices` table + a price-sync job.~~ Superseded 2026-06-12 — direct
  in-cluster `prices.*` view reads, no local table / sync job (see the Decision
  update note above).

### Neutral

- **Amends, does not supersede** ADR 0043. Rare-change off-chain handling is
  unchanged; this names a new category and path.

## Open / Pending (gates proposed → accepted)

- Read-cost measurement of the read-time join at `chart` / `list`.
- Review by karolkow (ADR 0043 author).
- Schema ADD `gross_volume_a` + Phase 1 `claimedOffers` extractor when
  `volume` / `fee_revenue` return (0199 / 0247).

## Delivery Checklist (per ADR 0032)

- [x] `docs/architecture/technical-design-general-overview.md` — updated (§6.11
      LP snapshots: CH compute-at-read note; §2.3 LP chart: TVL-only launch
      scope), linked to this ADR.
- [ ] `docs/architecture/indexing-pipeline/enrichment.md` — deferred to the 0199
      implementation PR (this ADR is `proposed`; no price-sync job / read-time
      join code lands yet). Same independence pattern ADR 0043 used for its
      consumer tasks.
- [ ] `docs/architecture/database-schema/database-schema-overview.md` — the
      `prices` table + the "not materialized on CH" note land with the 0199
      implementation PR.
- [ ] `docs/architecture/backend/backend-overview.md` — N/A — no backend behaviour
      change until 0199 implements the read-time join.
- [ ] `docs/architecture/frontend/frontend-overview.md` — N/A — no frontend
      contract change (chart already exists; launch payload is TVL-only).
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md` — N/A — the
      price-sync job reuses existing scheduled-job surface; no new infra topology.
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` — N/A — parser
      responsibility unchanged for TVL; the `gross_volume_a` extractor (deferred)
      will carry its own update.

## References

- [ADR 0043: field-allocation rule](./0043_field-allocation-rule.md)
- [ADR 0047: ClickHouse as primary API datastore](./0047_clickhouse-primary-api-datastore.md)
- [Task 0199 note: TVL-only decision + Prices-API contract](../1-tasks/backlog/0199_FEATURE_lp-analytics/notes/S-ch-tvl-enrichment-and-decision.md)
