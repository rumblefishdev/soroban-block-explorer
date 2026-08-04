---
id: '0199'
title: 'LP analytics: TVL + volume + fee_revenue (per-op extraction + USD)'
type: FEATURE
status: active
related_adr: ['0027', '0031', '0043', '0053']
related_tasks: ['0125', '0194', '0195', '0247', '0261', '0266']
tags:
  [
    priority-medium,
    effort-large,
    layer-indexer,
    layer-enrichment,
    prices-api-live-2026-07-22,
  ]
milestone: 2
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/367'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/371'
history:
  - date: '2026-05-07'
    status: backlog
    who: karolkow
    note: 'Spawned from 0194 §1d (snapshot-delta volume) — flaws: opposite-swap netting + asset_a units. Absorbed 0195 §2b (LP TVL); 0125 superseded. Spec: zero schema delta — `tvl/volume/fee_revenue` already exist (migration 0006), stay USD-denominated; `gross_volume_a` carried in SQS message body. All three columns owned by Lambda 2.'
  - date: '2026-05-12'
    status: blocked
    who: stkrolikiewicz
    note: >
      Promoting `blocked-on-oracle` from tag to lifecycle status. Phases 1+2
      ship atomically and depend on the team-built price API (owned by Oskar)
      being ready for consumption — without it Lambda 2 cannot USD-denominate
      `tvl`, `volume`, `fee_revenue` and Phase 1 (indexer per-op extraction
      to SQS) produces no observable DB writes on its own. Reusable
      enrichment scaffolding from 0191 (completed 2026-05-06,
      `crates/enrichment-shared/`, `crates/enrichment-worker/`, EventBridge
      + SQS + DLQ) is in place; gating dependency is solely the price API.
      Move back to active once Oskar's API contract (endpoint shape,
      freshness, no-price-for-asset behavior, latency budget) is finalized.
  - date: '2026-06-09'
    status: blocked
    who: stkrolikiewicz
    note: >
      Converted file → directory; added notes/S-ch-tvl-enrichment-and-decision.md
      (+ G-lp-tvl-flow.svg). Daily decision: SHIP TVL ONLY for now; drop
      volume + fee_revenue from chart/detail + endpoint contract until the
      gross_volume_a backfill is feasible. Prices-API contract confirmed
      (prices-api-design-after-2nd-review.md + Oskar): 1h/1d history to
      2024-02-20, per-asset OHLCV (timeframe=all / start+end, 100 req/s),
      per-candle vwap, identifier {code}:{issuer}/{contract}/native, null for
      untracked asset → no second API needed for TVL. Recommended implementation:
      Variant B (compute-at-read) — indexer is the single writer of reserves;
      price-sync job is the only Prices-API consumer (per-asset → prices table);
      TVL computed at read via join. Avoids the ReplacingMergeTree per-row UPDATE
      race (no version column → a later plain insert can silently erase
      analytics). Deferred: volume/fee need gross_volume_a (per-pool, on-chain,
      not reconstructable from reserve-delta) → historical XDR re-parse backfill,
      gated on 0247. Provenance: LP analytics are self-imposed scope from our SCF
      submission, NOT in customer RFP 4 — which itself permits launching with
      recent history. Follow-up: ADR 0043 amendment (compute-at-read), schema ADD
      gross_volume_a when volume returns, docs/architecture chart contract = TVL
      only for now.
  - date: '2026-06-09'
    status: blocked
    who: stkrolikiewicz
    note: >
      Created ADR 0053 (proposed) — "fast-change off-chain values on ClickHouse:
      compute-at-read via local price join" — codifying the architectural part of
      this decision (amends ADR 0043's off-chain=rare-change assumption; fourth
      path in the taxonomy). Updated docs/architecture/technical-design-general-overview.md
      (§6.11 + §2.3) per ADR 0032 evergreen rule. ADR 0053 is proposed, pending a
      read-cost measurement of the read-time join + karolkow review. The TVL-only
      scope cut stays a task-level decision (here), not in the ADR.
  - date: '2026-06-09'
    status: blocked
    who: stkrolikiewicz
    note: >
      Cross-link + reframe (see 0261 Decision): gross_volume_a (the on-chain input
      to volume/fee) shares one claim-atom extractor + one historical re-parse with
      the pool_id fix (0261/0266). Decision: capture gross_volume_a NOW on that
      shared re-parse — do not re-parse twice — while USD volume/fee display stays
      deferred until the Prices API is live (read-time join, ADR 0053). The
      TVL-only launch cut is unchanged; this only ensures the volume input is not
      thrown away. Linked 0247/0261/0266.
  - date: '2026-06-12'
    status: blocked
    who: stkrolikiewicz
    note: >
      Prices-API contract finalized with Oskar (recorded in
      notes/S-ch-tvl-enrichment-and-decision.md + ADR 0053 history). Refined vs
      2026-06-09: USD materialized write-time as retention-proof close_usd per
      grain, read via prices.* named views directly in-cluster (no sync job / no
      local prices table); single-asset price_usd_at(id,ts) keyed by natural
      identity; NULL + ok/no_asset_price/no_reference discriminator +
      usd_reference(bucket). Stays blocked: API not yet shipped + two prices-side
      impl deps — native-key alignment (gates XLM legs = most pools,
      critical-path) and SAC->classic resolver (their 0061 → gates Soroban-DEX
      legs = Phase 3). Phase 1/2 (classic) unblocks when API live; Phase 3 gated
      on 0061.
  - date: '2026-06-30'
    status: blocked
    who: karolkow
    note: >
      Cross-link from the 0331 investigation (2026-06-30, contract-as-holder sweep):
      Phase 3 (Soroban-DEX TVL) needs MORE than the SAC->classic price resolver (0061).
      Soroban AMM pools (Soroswap/Phoenix/Aquarius) are CONTRACTS; their reserves are
      contract-held SAC `ContractData Balance(pool)` entries — NOT indexed today. Classic
      pools get reserves from `LiquidityPoolEntry` (on-ledger, already indexed); Soroban
      pools have no such ledger entry, so the classic reserve path cannot supply them.
      Reading them is the SAME mechanism as 0331's type-3 `balance-seed`
      (`Balance(Address)` STATE read; decode the SAC `BalanceValue.amount`). Prerequisite:
      the deferred "0331 SAC ContractData balance ingestion (types 0/1/2)" follow-up — one
      data path serves both contract-held balances AND Soroban-LP reserves. Proof: one pool
      (`CATUJXDU…`) holds ~1.2M XLM + ~194k EURC contract-held, invisible to the explorer
      today. Classic Phase 1/2 unaffected.
  - date: '2026-07-02'
    status: blocked
    who: claude
    note: >
      0331 OPS run COMPLETE in prod (2026-07-02) — the contract-held reserve data path is
      now LIVE. Soroban-DEX pool reserves live in the unified `balances` table (contract-held
      `Balance(Address)` STATE read + SAC->classic re-key, ADR 0051), validated on-chain: pool
      `CATUJXDU...` reads 1,158,166 XLM + 203,657 EURC (matches `get_reserves` within ~1%). So
      0199's Soroban-DEX RESERVE-DATA prerequisite is SATISFIED — read reserves as
      `balances WHERE holder_id = <pool contract surrogate>`. 0199 stays `blocked-on-oracle`
      for the USD price API (Oskar); that gate is separate + unaffected. Custom-storage pools
      (Soroswap/Phoenix/Comet) still need a per-protocol reserve decoder (parked in 0210 Faza-3).
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Unblocked: blocked → backlog. The oracle gate is gone, proved from prod,
      not from a promise.** The lifecycle status said "move back to active once
      Oskar's API contract is finalized"; the 2026-06-09 note already recorded it
      as confirmed, and the data is now on the cluster: the `prices` database
      holds **37 tables / 593.6M rows / 32.41 GiB, 122,706 assets, history from
      2024-02-20 17:00** — exactly the contract shape. `price_ohlcv_1m` is live
      (777,735 inserts in 24h, newest candle minutes old).
      **Two caveats, both measured, neither a full blocker.**
      (1) THE OHLCV TABLES ARE NOT ONE ROLLUP CASCADE, despite looking like one.
      Seven MVs do chain `1m → 15m → 1h → 4h → 1d → 1w → 1M` (OHLCV aggregates
      losslessly — first/max/min/last/sum — so a cascade is exact). But a CH
      materialized view is an INSERT trigger and **never backfills history**, so
      the historical import was written *straight into the wide tables*, bypassing
      the chain. The three start dates give it away: `1m` reaches back to
      2018-12-13 (17,233 assets), `1h`/`4h`/`1d` start 2024-02-20 (122,706
      assets), `15m` starts 2026-06-01 — the day the cascade was switched on.
      Consequence: **an asset present in `1h` need not exist in `1m`, and vice
      versa** — never treat one as an aggregate of the other.
      Freshness: only `1m` is current. The contract view `price_usd_series` last
      advanced to bucket `2026-07-21 00:00`, so a TVL computed today is priced
      ~1.5 days stale. Inserts into the higher tables are still arriving (96/day
      into `1h`), but carrying past-dated candles — consistent with an import
      still catching up rather than a stalled stream. Confirm with the prices
      owner before shipping; it is their service, not ours.
      (2) NATIVE-KEY ALIGNMENT IS DONE — but only through the contract view, and
      the raw tables actively mislead. Both prices-side deps shipped in their
      PR #39 (ADR 0053 update 2026-06-16). `prices.price_usd_series` carries a
      structured `asset_kind` column, and native resolves under
      `asset_kind='native'`. **Measured through the view: 39,370 of 52,288 pools
      (75.3%) have both legs priceable**, native legs included.
      RECORDED AS A TRAP, because I fell in it: joining the raw `prices.assets`
      table on `(asset_code, issuer_address)` is wrong and *fails silently*.
      `liquidity_pools` writes native as `type=0, code='', issuer_id=0` while
      `prices.assets` writes `code='XLM'`, so they never match — but the join
      still "succeeds" against one of the **153 empty-code rows** in
      `prices.assets`, silently pricing every native leg as an arbitrary asset.
      That produced a bogus 96.4% coverage figure in my first pass. Also, **249
      distinct asset_ids share the code 'XLM'** (996 rows), so `asset_code` alone
      is never a key. Use `price_usd_series` / `current_price_usd` and key on
      `asset_kind`; never touch `prices.assets` directly.
      **What this unblocks concretely:** Phase 1/2 (classic + native) over the
      39,370 priceable pools (75.3%). Phase 3 (Soroban-DEX) also has its
      SAC->classic resolver shipped, so its remaining gate is our side:
      custom-storage pools (Soroswap/Phoenix/Comet) need a per-protocol reserve
      decoder. Scope decision from 2026-06-09 stands: TVL only —
      volume/fee_revenue need `gross_volume_a`, and 0247 archived without ever
      running its Path-A latency benchmark, having found the real cost is a
      result-meta parse `xdr-parser` does not expose yet.
      **0213 and 0215 deliberately stay blocked** — both gate on 0199 *shipping*,
      not on it being unblocked ("running 0213 before 0199 ships would surface LP
      rows as drift = 100%, useless noise").
      Also repointed: ADR 0048 (compute-at-read) was renumbered to **0053** — it
      had collided with the accepted Cloudflare ADR on the same id.
  - date: '2026-07-22'
    status: active
    who: karolkow
    note: >
      Activated. **Scope pinned before starting, because this task has drifted
      three times** (June TVL-only cut, then 0247, then 0331): TVL only, read via
      the `prices.price_usd_series` contract view, compute-at-read with no
      materialization into `liquidity_pool_snapshots` (ADR 0053 Decision #1).
      Target surface is the **39,370 of 52,288 pools (75.3%)** measured as having
      both legs priceable — classic and native. `volume`/`fee_revenue` stay out:
      they need `gross_volume_a`, and 0247 archived without running its Path-A
      benchmark after finding the real cost is a result-meta parse `xdr-parser`
      does not expose. Phase 3 (Soroban-DEX) is out of this pass too — its
      reserve data landed with 0331, but custom-storage pools
      (Soroswap/Phoenix/Comet) still need a per-protocol decoder.
      **Open question to settle with the price owner before writing the read
      path, not halfway through:** the view last advanced to bucket
      `2026-07-21 00:00`. If ~1.5-day-stale pricing is the steady state rather
      than a catch-up artifact, TVL will visibly disagree with other explorers
      and the staleness has to be surfaced in the API/UI contract rather than
      hidden. The `no_asset_price`/`no_reference` discriminator already exists
      for the missing-price case; there is no equivalent for stale-price.
      Reminder for whoever writes the SQL: key on `asset_kind`, never join raw
      `prices.assets` on `(asset_code, issuer_address)` — that silently prices
      native legs off one of its 153 empty-code rows.
  - date: '2026-08-04'
    status: active
    who: stkrolikiewicz
    note: >
      Two updates. (1) Recorded Oskar's 2026-08-03 message as
      notes/R-prices-freeze-incident-and-current-price-usd-v13.md: the
      ~1.5-day staleness flagged at activation was an INCIDENT (coarse OHLCV
      frozen 07-21 02:44 → recovered 08-03 09:57), not steady state — the
      "confirm with the prices owner" gate is closed. The 07-21→08-03 hole in
      1h/1d does not backfill itself; their pre-roll pends (they ping when
      done) — do not run AC validation over that window until then.
      Compute-at-read (ADR 0053) means our charts self-heal, zero recompute.
      Also: current_price_usd goes 6→13 columns (additive) with SENTINEL
      semantics (0 = unavailable, sources='' is invalid JSON) — named column
      lists only, never positional; full trap table in the note.
      (2) Verified against schema + archive: the activation note's premise
      "volume/fee_revenue stay out — they need gross_volume_a" is STALE.
      `liquidity_pool_snapshots.gross_volume_a` exists (init.sql, 0268 ALTER),
      is written by live ingest (stage.rs gross_volume_a_by_pool), and was
      historically backfilled by the 0266 re-parse (261.32M non-NULL rows,
      verify-gates + 0267 E20 passed 2026-06). The only remaining gate for USD
      volume/fee_revenue is the same read-time price join TVL needs, per the
      schema comment itself. Scope re-cut (TVL-only vs TVL+volume+fee in one
      pass) is an OPEN question for the humans, deliberately not decided here.
      Distinct from #371: gross_volume_a is a per-(pool, ledger) SUM and
      cannot serve per-transaction amounts — that is 0279's
      lp_operation_amounts plan (per-atom rows + deposits/withdrawals from
      LedgerEntryChanges + its own backfill). #371 is claimed by both this
      task (§"Also owns", 2026-07-31) and 0279 (re-scoped 2026-07-30) —
      ownership overlap to resolve before Phase B starts.
  - date: '2026-08-04'
    status: active
    who: stkrolikiewicz
    note: >
      SCOPE RE-CUT (human decision, stkrolikiewicz): Phase A ships all three
      columns — tvl, volume, fee_revenue — computed at read via the
      prices.price_usd_series join (ADR 0053, no materialization). The
      2026-06-09 TVL-only cut is retired because both of its premises are
      gone: gross_volume_a has been backfilled since 0266 (2026-06-16,
      261.32M rows) and the Prices API is live in-cluster. Marginal cost is
      two expressions in the same price join (volume = gross_volume_a x
      price_a; fee_revenue = volume x fee_bps / 10000). The plan's Phase 1/2
      (SQS emit + Lambda 2 column writes) is OBSOLETE for these columns —
      superseded by compute-at-read; the indexer already persists every
      on-chain input. Phase B (#371 per-tx amounts) unchanged and still
      pending the 0199/0279 ownership decision.
  - date: '2026-08-04'
    status: active
    who: stkrolikiewicz
    note: >
      Phase A IMPLEMENTED — PR #380 (feat/0199_lp-analytics, Refs #367/#371).
      Chart: per-snapshot join against price_usd_series (1d/1w) /
      price_usd_series_1h (1h) at the interval's grain; TVL = last priceable
      snapshot per bucket (both legs required); volume prices each ledger's
      gross_volume_a at its own bucket, any unpriceable swap NULLs the
      bucket (honest hole, no under-sum); fee = volume x fee_bps/10000.
      0356 perf shape preserved. Detail: tuned #347 query untouched; second
      small fetch (current_price_usd spot + 24h gross_volume_a sum), USD in
      Rust, prices errors DEGRADE to NULL + error log (never 500). Emerged
      decisions: (a) Float64 USD rounded to cents — Decimal128 scale
      overflow avoided, 1% AC tolerance covers it; (b) nullIf(close_usd,0)
      instead of join_use_nulls (readonly-user rejection gotcha); (c) 1w
      interval joins the DAILY series — prices provides 1h+1d grains only;
      (d) detail volume priced at CURRENT spot, not per-trade (upgrade
      path: hourly join); (e) list endpoint deliberately untouched —
      list-side TVL + min_tvl filter is its own perf problem (lplist cr
      history). FE needs nothing: PoolCharts.tsx already renders all three
      metrics. GATES before merge/deploy: (1) box read-cost measurement of
      the prices-view join on the hottest pool (ADR 0053 gate — the views
      aggregate price_ohlcv under the hood; identity predicate may not
      push down); (2) SELECT grant on prices.* for the API CH user via
      ansible users.d + compose recreate (prices_writer lesson); (3) E2E
      numbers validation vs Horizon AC after Oskar's pre-roll closes the
      07-21..08-03 hole. Canonical SQL 19/21 + tech-design + ADR 0053
      updated in the PR; api-types regenerated.
---

# LP analytics: TVL + volume + fee_revenue

## Summary

Populate `liquidity_pool_snapshots.{tvl, volume, fee_revenue}` correctly. All three are existing schema columns (migration 0006); this task makes them **USD-denominated** per ADR 0027's forward-looking intent. Zero schema delta.

Two concerns:

1. **Per-op volume** — gross volume from PathPayment `claimedOffers[].amount_sold`, not snapshot-delta `ABS(reserve_post − reserve_pre)`. Snapshot-delta nets opposite swaps inside one ledger.
2. **USD denomination** — multiply on-chain values (reserves for TVL, gross_volume_a for volume) by per-asset USD price from team-built price API (Oskar). All three columns end as USD.

This task **consumes** the price API; it does not build it.

Subsumes 0125 (original LP analytics) and absorbs 0195 §2b (LP TVL).

### Also owns: per-row trade amounts on the pool page (issue #371, triaged 2026-07-31)

An external report asks the pool's "Recent transactions" table to say what
each trade actually moved, instead of a bare `Trade` chip — stellar.expert
shows the amounts on the same view.

Verified against the live page and the schema, not assumed:

- the endpoint returns `hash, source_account, operation_types, fee_charged,
created_at` and no amounts, so the table renders what it has;
- the Event chip is derived correctly from `operation_types` (Deposit /
  Withdrawal / Trade) — no bug there;
- **per-pool fill amounts are not indexed at all**: `operation_pools` is a
  (pool, ledger, tx) index with no values, and `operations_appearances.amount`
  is the operation's own folded amount, not the pool's side of the fill.

The amounts live in the same place this task already needs them — the
per-op `claimedAtoms` extraction (`amountSold`/`amountBought` per fill,
`operation.rs::append_pool_claims`). That makes #371 a **presentation
consumer of Phase 1**, not separate work: once the extraction persists
per-fill amounts, the table gains an "Amount" column reading
`12,059.29 XLM → 38.5M KALE`, and no second pipeline is introduced.

Size: **large / needs-backfill** — it is an ingestion change plus historical
re-parse, exactly the gating this task already carries. Nothing shippable
from the frontend alone; a "Trade" row cannot invent the amounts it never
received (see the no-misleading-fallbacks rule).

Sequencing note: 0460 #12 found that `claim_atoms()` already returns the
ORDER-BOOK atoms too and only `claim_lp_atoms` filters them out — emitting
both with a `source: orderBook|pool` marker is the cheaper root fix and
serves #371 and the route-strip labels at once.

## Per-lambda ownership

USD denomination → off-chain prices → ADR 0043 forces all three column writes to Lambda 2. Indexer contributes on-chain inputs only (reserves already written; `gross_volume_a` passed in SQS message body, no staging table).

| Column        | Inputs                                                                      | Written by   | Formula                                           |
| ------------- | --------------------------------------------------------------------------- | ------------ | ------------------------------------------------- |
| `tvl`         | `reserve_a, reserve_b` (Indexer, on-chain) × USD prices (oracle, off-chain) | **Lambda 2** | `tvl = reserve_a × price_a + reserve_b × price_b` |
| `volume`      | `gross_volume_a` (Indexer, in SQS msg) × `price_a` (oracle)                 | **Lambda 2** | `volume = gross_volume_a × price_a`               |
| `fee_revenue` | `volume` × `fee_bps` (Indexer, `liquidity_pools.fee_bps`, on-chain)         | **Lambda 2** | `fee_revenue = volume × fee_bps / 10000`          |

## Plan

### Phase 1 — Indexer-side extraction

- XDR parser: extract `claimedOffers[]` from PathPayment ops (types 2 + 13). Sum `amount_sold` per `(pool_id, ledger_sequence)`. Naturally excludes `LiquidityPoolDeposit` / `LiquidityPoolWithdraw` (capital flow, not volume — different op types, no claimed_offers).
- **Bidirectional swap normalization.** `claimedOffers[].amount_sold` is denominated in the asset that was sold, so an A→B swap reports in A while a B→A swap reports in B. Both directions count toward gross pool volume; the parser must convert B-denominated amounts into the A-denominated accumulator `gross_volume_a` before summing. The conversion uses the trade's effective price (`amount_bought_a / amount_sold_b`), not a snapshot reserve ratio (which would suffer the same opposite-swap netting flaw the snapshot-delta approach hit). Verification queries (see Acceptance Criteria below) compare USD-denominated totals **after** this normalization, so any direction mismatch surfaces as a Horizon-vs-explorer drift on a known mixed-direction pool.
- Producer hook in `enrichment_publish.rs`: insert-hook on each new `liquidity_pool_snapshots` row. Emit `EnrichmentMessage::LpAnalytics { pool_id: [u8; 32], snapshot_id: i64, gross_volume_a: Option<NumericString> }`. `None` for ledgers without swap activity on this pool. Exactly-once per snapshot insert.
- No DB write to `liquidity_pool_snapshots.volume` from indexer side. Phase 1 is parse + emit only.

### Phase 2 — Lambda 2 consumer

- New variant in `EnrichmentMessage` (defined above with `gross_volume_a` field).
- New module `crates/enrichment-shared/src/enrich_and_persist/lp_analytics.rs` exposing `enrich_pool_analytics(pool, msg, oracle)`.
- Worker reads `reserve_a, reserve_b, fee_bps` from DB (already populated by indexer), fetches USD prices from price API, computes:
  - `tvl = reserve_a × price_a + reserve_b × price_b`
  - `volume = gross_volume_a × price_a` (skip when `gross_volume_a IS NONE` → leave `volume = NULL`)
  - `fee_revenue = volume × fee_bps / 10000` (skip when volume NULL)
- **Atomicity model.** A single UPDATE statement carries all three column writes (transactional atomicity). **Semantic atomicity is per-column, not all-or-nothing**: a partial-oracle outcome can leave one column populated while another stays NULL — the column's input is the discriminator, not a global "all or none" gate. Decision matrix:

  | Inputs available                                           | `tvl`                                             | `volume`            | `fee_revenue` |
  | ---------------------------------------------------------- | ------------------------------------------------- | ------------------- | ------------- |
  | `price_a` + `price_b` + `gross_volume_a`                   | computed                                          | computed            | computed      |
  | `price_a` + `price_b`, `gross_volume_a IS NONE` (no swaps) | computed                                          | NULL                | NULL          |
  | `price_a` only (price_b permanent fail)                    | NULL (need both legs)                             | computed            | computed      |
  | `price_b` only (price_a permanent fail)                    | NULL (need both legs)                             | NULL (no `price_a`) | NULL          |
  | both prices permanent fail                                 | NULL                                              | NULL                | NULL          |
  | any input transient                                        | (no write — `EnrichError::Transient` → SQS retry) |                     |               |

- **Sentinel.** Permanent oracle failure writes `NULL` (not `0`) for any column whose required inputs are unavailable, and emits a WARN log carrying `pool_id`, `snapshot_id`, per-leg oracle error. NULL preserves the "fetch attempted, no value" semantics without conflating with legitimate zero-volume snapshots (a pool with no swaps in a ledger genuinely has `volume = 0`). `liquidity_pools.tvl` (latest, if column exists) is NEVER overwritten by Lambda 2 — only by indexer reserves recompute. If operational distinction between "permanent fail" and "pending" becomes valuable, surface via metrics / log filters or a future `oracle_status` enum, not via numeric sentinels.
- Transient (price-API 5xx, network, rate limit) → `EnrichError::Transient` → SQS retry → DLQ.
- Backfill of pre-existing snapshots (NULL because they predate this task) → owned by 0196.

### Phase 3 — Soroban DEX adapters

Soroswap, Phoenix, etc. emit `swap` events with explicit per-swap amounts; no PathPayment. Per-DEX adapter scaffolding. Out of Phases 1+2; gates on Phase 1+2 landing.

## Acceptance Criteria

**Phase 1 (Indexer):**

- [ ] PathPayment ops yield `gross_volume_a` extraction during XDR parse (unit + integration tests with mocked claimed_offers).
- [ ] Insert-hook emits exactly one `LpAnalytics` SQS message per new `liquidity_pool_snapshots` row (test).
- [ ] `gross_volume_a = None` on no-swap ledgers; deposit/withdraw ops do not contribute to the sum.

**Phase 2 (Lambda 2):**

- [ ] `tvl`, `volume`, `fee_revenue` populated by Lambda 2 from `reserves + gross_volume_a + fee_bps + oracle prices`.
- [ ] Permanent oracle fail → per-column NULL (matrix in Phase 2 §Atomicity) + WARN log carrying pool_id, snapshot_id, per-leg oracle errors; transient → SQS retry.
- [ ] `liquidity_pools.tvl` (if column exists) is not overwritten by Lambda 2 under any oracle outcome (only by indexer reserves recompute).
- [ ] Sample query: non-NULL `tvl` and `volume` on production-region pools with valid oracle data.
- [ ] Sample query: `volume / price_a` agrees with Horizon `/liquidity_pools/{id}/trades` gross volume **within 1% tolerance** on a known mixed-direction pool. The "≈" comparison is intentional — Horizon and the explorer both use per-operation extraction (Horizon parses `claimedOffers[]` the same way), so the only sources of drift are (a) USD price-snapshot timing (Horizon uses asset units, we multiply by `price_a` then divide back), (b) rounding in `NUMERIC(28,7)` arithmetic, (c) Soroban DEX swap events not yet covered in Phase 1 (Phase 3 scope). Drift > 1% on a classic-only pool indicates an extraction bug and blocks the AC.
- [ ] `GET /liquidity-pools/:id/chart` returns non-null time series (originally 0125 AC).

**Phase 3:**

- [ ] At least Soroswap adapter scaffolding.

**Common:**

- [ ] Permanent / transient `EnrichError` mapping documented + unit-tested.
- [ ] Integration test (mock price API).
- [ ] CDK DepthAlarm thresholds reviewed.
- [ ] Docs: ADR 0043 per-kind matrix amendment + schema doc + indexing-pipeline doc + endpoint-queries comments.
- [ ] API types regenerated if any DTO surfaces these fields.
- [ ] 0196 backlog updated to capture backfill dedup ownership for the three columns.

## Notes

- **Required consultation with Oskar** — owns the price API. Phase 2 designed jointly: endpoint schema, freshness/cache contract, no-price-for-asset behavior, latency budget.
- **Blocked on price API.** Phases 1+2 ship atomically — Phase 1 alone produces no DB writes (SQS messages with no consumer until Phase 2 live).
- **USD rationale** — single comparable metric across mixed-asset pools; on-chain LP-graph traversal (peg-rooted hops) considered + rejected as too complex vs team-built price API.
