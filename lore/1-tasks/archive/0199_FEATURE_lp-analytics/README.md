---
id: '0199'
title: 'LP analytics: TVL + volume + fee_revenue (per-op extraction + USD)'
type: FEATURE
status: completed
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
  # #371 dropped 2026-08-17 — the ownership overlap this task recorded as open
  # was settled by events, not by argument: 0279 shipped the per-tx amounts
  # that were this task's Phase B, and they are live and verified on
  # production. See the history entry of that date.
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
  - date: '2026-08-04'
    status: active
    who: stkrolikiewicz
    note: >
      GATE 1 EXECUTED — read-cost box-measured on the hottest pool
      (a01fce81, 1.87M snapshots, yXLM/WGUARDIAN); full table in the PR
      #380 comment + ADR 0053 body. Verdicts: 1h/7d 227ms/2.0M rows OK;
      1d/90d 721ms/10.3M rows acceptable (+6.0M vs 442ms/4.2M baseline =
      exactly 2x the view scan); detail 112ms/1.6M OK; **1w/104w
      4.6s/70.7M rows/2.1GiB NOT acceptable**. Mechanism proven: the
      views' bucket range DOES prune price_ohlcv (1.89M of 19.6M for 90d)
      but identity CANNOT push down (computed columns) — long windows scan
      every asset's candles x2 legs. Fix is prices-side, and their
      views.sql SS6 pre-authorized it: "promote to a materialized table
      only if measured read latency demands it" — demand now measured,
      request drafted for Oskar. Interim: 1w stays correct but expensive;
      MEDIUM cache + Cloudflare in front; NOT a merge blocker per se —
      karolkow's call at review. TWO MORE box findings: (a)
      current_price_usd is live (3,316 assets, updater ticking) but
      price_usd=0 SENTINEL for native XLM itself → detail switched from
      spot to argMax(close_usd) over price_usd_series_1h 48h lookback
      (commit 4919ca78; ~2h max staleness, same cost, real data verified
      on both legs, consistent with chart's last bucket) — reported to
      Oskar, revisit when their 0039 prices native; (b) syntax + decode
      of every new query validated against live CH in the same run.
  - date: '2026-08-04'
    status: active
    who: stkrolikiewicz
    note: >
      SHIP DECISION (human): ship the whole Phase A including 1w; Oskar
      draft sent by Stanisław. Gates re-verified under PROD conditions —
      ran everything again AS api_reader (the real API user): 1d 688ms /
      1h 272ms / detail 132ms (no profile penalty on default paths); 1w
      completes under the read_only profile caps (2.11 GiB < 4 GiB, no
      timeout) at 13.7s cold (3x the default-user run — thread cap bites
      only at that scale). GATE 2 DISSOLVED: api_reader has no <grants>
      block in services.xml, so its read_only profile already reads
      prices.* — verified empirically; no ansible/users.d change, no
      compose recreate needed to ship. PR #380 body corrected (the grant
      claim was wrong), CI fully green. Remaining to done: karolkow
      review -> merge -> manual deploy (make deploy-production-compute)
      -> post-preroll AC validation vs Horizon -> /issues sweep closes
      #367 at deploy. Side observation: current_price_usd began pricing
      WGUARDIAN mid-session but still not native XLM — consistent with
      the reported 0039 gap.
  - date: '2026-08-04'
    status: active
    who: stkrolikiewicz
    note: >
      SELF-REVIEW of the branch found 6 issues; 5 fixed in d0efb496, CI
      green. The one that mattered: the chart NULLed its NEWEST bucket for
      any pool with an illiquid leg, because a price candle only exists
      once the asset trades in that bucket and we joined on exact bucket
      equality. Reproduced on prod (pool a01fce81 had today's yXLM candle
      but none for WGUARDIAN) - and the detail endpoint, which uses a 48h
      lookback, happily returned a TVL at the same moment, so the page
      contradicted itself on its most-read number. Fix: ASOF LEFT JOIN
      taking the last close at or before the wanted bucket, CAPPED at 48h
      (MAX_PRICE_CARRY_SECONDS, shared with detail). The cap is the whole
      point - uncapped carry-forward would have rendered the 07-21..08-03
      freeze as live TVL priced off a 12-day-old candle (verified: the ASOF
      match for 07-28 IS a 07-21 candle). ASOF needs an equi-join column
      hence the constant k - a real column, not ON 1=1 (that pins
      join_algorithm to hash). Post-fix on prod as api_reader: today's
      bucket reports 25.59, freeze window stays NULL, and 1h got CHEAPER
      (204ms/2.5M vs 272ms/2.8M - ASOF merges instead of hashing).
      Also fixed: chart and detail emitted different wire shapes for the
      same field (CH toString(round(x,2)) gives "25"/"1.5"/"0" vs Rust
      "25.00") - SQL now returns raw Nullable(Float64) and one Rust helper
      formats both, with fee_revenue derived from the volume float (removed
      a fragile positional bind); detail reported "$0.00 traded" on a parse
      failure (only SQL NULL means zero now); and two comments asserted
      falsehoods (the prices grant gate, and detail/chart price
      consistency). DELIBERATELY NOT FIXED, needs karolkow's call: the
      per-bucket volume veto still discards a whole 1w bucket on one
      unpriced ledger - an unmarked partial sum reads as a real number, so
      that wants a coverage field, not a silent change.
      DECODE: Nullable(Float64) -> Option<f64> is exactly the wire-type
      contract decode_smoke guards, and SSH forwarding to the box is
      administratively prohibited (AllowTcpForwarding no - do not work
      around it), so it is now covered by a schema-free test against a
      local CH 26.3 docker; NULL survives as None, not 0.0.
      UPSTREAM (for Oskar, not our bug): the IN-PROGRESS candle is
      unstable - yXLM's 13:00 hourly close read 1.3085 USD against ~0.170
      in every neighbouring hour (7.7x), briefly quadrupling the newest 1h
      TVL point while reserves stayed flat. We report it faithfully and do
      NOT apply our own outlier filter (prices owns that; diverging would
      make our numbers disagree with theirs).
  - date: '2026-08-04'
    status: active
    who: stkrolikiewicz
    note: >
      CORRECTION to the entry above: "the in-progress candle is unstable" was
      the symptom, not the cause. Read the raw price_ohlcv_1h rows and the
      mechanism is PARTIAL ENRICHMENT — price_usd_series volume-weights
      close_usd across a bucket's per-source/per-quote rows but only over
      rows passing `close_usd > 0`, and close_usd is baked by a LATER
      enrichment pass, so a fresh bucket's average runs over an arbitrary
      subset. On yXLM's 13:00 hour the single surviving row was a 0.764-unit
      dust print at 1.3085 (vs ~0.170 real); by 14:13 all five rows read 0
      and the bucket had vanished from the view entirely. The weighting is
      sound — the same print sits in 12:00 beside 42,038 units of real
      volume and moves the close by nothing. It is the filter, not the maths.
      Full write-up + evidence table in
      notes/R-prices-freeze-incident-and-current-price-usd-v13.md §3 (the
      note is retitled: it is now the collected prices.* READ TRAPS).
      Also captured there: current_price_usd 0-sentinels native XLM itself,
      the concrete reason detail reads the 1h series. Sent to the prices
      owner with two suggested fixes (hold a bucket out of the view until
      fully enriched, or weight the unenriched rows in once they land).
      Our stance unchanged: no outlier filter of our own — prices owns that,
      and diverging would defeat reading their views as one source of truth.
      Recorded in the branch too (7182b45f) since the SQL doc carried the
      same imprecise wording.
  - date: '2026-08-05'
    status: active
    who: stkrolikiewicz
    note: >
      SCOPE CLARIFICATION from re-reading issue #367 (its whole body is one
      screenshot): the reporter shows the pools LIST page (search 'usdc';
      columns Pool / Reserves / Total shares / Participants) and asks for a
      USD TVL estimate THERE, to compare pools at a glance. No mention of
      charts or intervals. Two consequences: (a) the expensive 1w chart
      window is entirely our own SCF-scope surface, not the reporter's ask
      — the prices-side materialization can ride behind cache at its own
      pace; (b) PR #380 (detail + chart) does NOT close #367's literal
      request — the list TVL column is exactly the deliberately deferred
      Phase A2. At the post-deploy /issues sweep: either keep #367 open
      until Phase A2 lands (recommended — the literal ask is the list) or
      close with an explicit 'detail+chart shipped, list column tracked
      separately' comment. Do not auto-close on deploy.
  - date: '2026-08-05'
    status: active
    who: stkrolikiewicz
    note: >
      Phase A2 added to the plan (human decision, stkrolikiewicz): list-side
      TVL column — the literal #367 ask. Checked first that no dedicated
      task exists (#367 lives only here; 0215 is the adjacent DOCS
      catalogue, obsolete per its own 2026-07-22 note -> archived with
      this task when done; 0401 is unrelated lplist keyset perf). Kept
      inside 0199 rather than a new task: 0199 owns the #367 link and all
      the machinery A2 reuses (price_leg, MAX_PRICE_CARRY_SECONDS,
      usd_str, prices traps), and the 0199/0279 dual-claim on #371 shows
      what split ownership costs. Scope pinned: display-only, one batched
      price lookup per page; min_tvl/sort-by-TVL stay out (page-membership
      problem) until the prices-side materialization. Next: implement on a
      branch stacked on feat/0199_lp-analytics + local FE/BE stack over
      prod data slices to eyeball and test before shipping.
  - date: '2026-08-05'
    status: active
    who: stkrolikiewicz
    note: >
      Phase A2 IMPLEMENTED + tested end-to-end on production data
      (branch feat/0199_lp-list-tvl, stacked on the PR #380 branch). No
      data import was needed: new `cargo run -p api --bin local` serves
      the lib's register_routes over plain axum and talks mTLS to prod
      CH with the developer cert (CN -> dev_shared, read-only by
      construction); Vite dev proxy points at it. List TVLs render from
      prod (XLM/GOLD $6.4K, GOLD/yXLM $2.3K, dashes for unpriceable);
      detail $130.41 vs chart last bucket $130.38 agree (the carry-
      forward + shared staleness rule working); charts verified on all
      intervals incl. weekly toMonday buckets and the freeze window
      rendering as missing points. TWO DISCOVERIES the local run caught:
      (a) FE had a pre-0199 CHARTS_ENABLED=false kill-switch hiding the
      whole chart card (its own comment said flip when 0199 ships) —
      flipped + pending-oracle placeholder deleted; my earlier "FE needs
      nothing" claim was wrong. (b) PRICES FINDING #5: canonical USDC
      has ZERO rows in price_usd_series across ALL history — it only
      appears as the QUOTE side of candles (10.5k quote rows in 48h) and
      the views emit base assets only, so every USDC-leg pool reads NULL
      TVL. Fix is prices-side (synthetic base row under their own
      USDC=$1 peg assumption); deliberately NOT hardcoded on our side —
      add to the Oskar report. Dev-env note: dual-React crash from stale
      libs/ui/dist + vite dep cache; fix = trash dists + nx reset + rm
      .vite caches (matches the stale-dist memory). Ship path open:
      fold A2 into PR #380 (one review) or stack a second PR.
  - date: '2026-08-05'
    status: active
    who: stkrolikiewicz
    note: >
      A2 FOLDED into PR #380 (human decision): fast-forward 769a6e2c onto
      feat/0199_lp-analytics, stacked branch deleted, PR body rewritten
      (now lists chart+detail+list, the local mTLS verification, and all
      five prices-side gaps incl. the USDC quote-only finding). One review
      cycle for karolkow. CI green post-fold.
  - date: '2026-08-17'
    status: active
    who: stkrolikiewicz
    note: >
      OWNERSHIP OVERLAP CLOSED — issue #371 unlinked from this task. The
      2026-07-31 entry left it open ("claimed by both this task and 0279,
      to resolve before Phase B starts"); events resolved it. 0279's
      lp_operation_amounts shipped in production-2026.08.17-1, 0489 fixed the
      credit12 leg it dropped in -2, and the per-tx amounts are verified live
      on the pool page in all three forms (deposit `A + B`, trade `A → B`,
      one line per operation). That WAS Phase B, so this task no longer owns
      #371 and should not hold it open — it keeps #367 (TVL), which is its
      actual subject. Phase B is struck from this task's scope; nothing else
      changes. The residual UX work on that view is 0490 and 0491.
  - date: '2026-08-19'
    status: completed
    who: stkrolikiewicz
    note: >
      DONE and archived. Shipped as PR #380 (merged 2026-08-12, 5b77f7e8;
      +close_usd guard 4ba9424e; develop-merge 2030b7e4), deployed to
      production and verified live (#367 closed with links; #366 rode the
      same deploy). Scope as re-cut 2026-08-06: compute-at-read USD
      (ADR 0053) — TVL on list+detail, 24h volume+fee_revenue on detail,
      TVL/volume/fees charts; no Lambda 2, no SQS, no snapshot-column
      writes (original Phase 1/2 ACs superseded). AC validation
      2026-08-19: gross volume vs Horizon trades on canonical XLM/USDC
      over closed day 08-18 = EXACT match (526,863.8801956 XLM both
      sides, 0.0000000 percent vs 1 percent tolerance, 1,209 swap
      ledgers vs 3,595 trades); live reserves and total_shares also
      exact to the stroop. Coverage at close (pinned 08-19): 99.1
      percent priceable-ever, 71.0 percent 90d, ~41 percent 48h overall
      (69 percent of active pools), ceiling ~96 percent of active pools
      pending the prices-side pivot step. The month-long prices.*
      collaboration (freeze pre-roll, USDC base-row structural fix,
      USDT depeg correction, duplicate-identity mechanism, 0171
      sentinel) is recorded in
      notes/R-prices-freeze-incident-and-current-price-usd-v13.md.
      Deferred, NOT spawned as tasks yet (awaiting explicit go, see
      Future Work): nullable chart points, detail query
      parallelisation, honest min_tvl filter, Phase 3 Soroban-DEX TVL
      (gated on 0331 follow-up), forming-bucket guard revisit when
      their coverage gate ships.
---

# LP analytics: TVL + volume + fee_revenue

## Summary

Populate `liquidity_pool_snapshots.{tvl, volume, fee_revenue}` correctly. All three are existing schema columns (migration 0006); this task makes them **USD-denominated** per ADR 0027's forward-looking intent. Zero schema delta.

Two concerns:

1. **Per-op volume** — gross volume from PathPayment `claimedOffers[].amount_sold`, not snapshot-delta `ABS(reserve_post − reserve_pre)`. Snapshot-delta nets opposite swaps inside one ledger.
2. **USD denomination** — multiply on-chain values (reserves for TVL, gross_volume_a for volume) by per-asset USD price from team-built price API (Oskar). All three columns end as USD.

This task **consumes** the price API; it does not build it.

Subsumes 0125 (original LP analytics) and absorbs 0195 §2b (LP TVL).

### ~~Also owns~~ → moved to 0279: per-row trade amounts on the pool page (issue #371, triaged 2026-07-31)

> **Ownership resolved 2026-08-11**: #371 belongs wholly to task 0279
> (activated same day, with measured sizing, pinned row shape and a run
> plan). The section below is kept as the original triage record; 0279
> supersedes it as the plan of record.

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

### Phase A2 — list-side TVL column (issue #367's literal ask)

Added 2026-08-05 after re-reading #367: the report's screenshot is the pools
LIST (raw reserves, no USD column) — the list column IS the ask; detail+chart
(PR #380) are adjacent value. Scope:

- **Display only.** `GET /liquidity-pools` populates `tvl` per row, computed
  at read like the detail endpoint: latest reserves x each leg's last
  `price_usd_series_1h` close (shared `MAX_PRICE_CARRY_SECONDS` rule), NULL
  unless both legs price. One batched price lookup per PAGE (distinct leg
  identities of <=100 pools -> one bounded 48h window scan), never per row.
- **Out of scope, documented:** `filter[min_tvl]` semantics and sort-by-TVL.
  Both change PAGE MEMBERSHIP, so they cannot ride a per-page computation —
  they need TVL for ALL pools per request (the old full-scan CTE problem) or
  the prices-side materialized table. Keep `filter[min_tvl]` returning empty
  as today; revisit after Oskar's materialization lands.
- **FE:** render the TVL column in the pools list (PoolItem.tvl already on
  the wire).
- **Cleanup:** archive 0215 (DOCS, blocked-on-0199 FE-impact catalogue) in
  the same motion — its own 2026-07-22 note asks for exactly that.

### Phase 3 — Soroban DEX adapters

Soroswap, Phoenix, etc. emit `swap` events with explicit per-swap amounts; no PathPayment. Per-DEX adapter scaffolding. Out of Phases 1+2; gates on Phase 1+2 landing.

## Acceptance Criteria

> Re-cut 2026-08-06 (ADR 0053 compute-at-read): Lambda 2 / SQS /
> snapshot-column writes were removed from the design, so the Phase 1/2
> criteria below are marked per what actually shipped, with superseded
> items named as such rather than ticked.

**Phase 1 (Indexer):**

- [x] PathPayment ops yield `gross_volume_a` extraction during XDR parse — shipped earlier on the 0261/0266 shared re-parse; validated 2026-08-19 (exact Horizon match below).
- [ ] ~~Insert-hook emits one `LpAnalytics` SQS message per snapshot row~~ — SUPERSEDED by ADR 0053: nothing consumes snapshot inserts; USD is computed at read.
- [x] `gross_volume_a = None` on no-swap ledgers; deposit/withdraw do not contribute (1,218 snapshot rows vs 1,209 swap ledgers on the validation day).

**Phase 2 (compute-at-read, formerly Lambda 2):**

- [x] `tvl`, `volume`, `fee_revenue` computed from `reserves + gross_volume_a + fee_bps + prices` — at read (PR #380), never written to snapshots.
- [x] Permanent price miss → NULL fields (dash in UI); prices errors degrade to NULL with logging, never a 500 — the superseded oracle-fail matrix, same intent, new form.
- [ ] ~~`liquidity_pools.tvl` not overwritten by Lambda 2~~ — SUPERSEDED: no writer exists at all.
- [x] Sample query: non-NULL `tvl` and `volume` on production pools — verified live 2026-08-18/19 (list, detail; e.g. XLM/USDC $4.2M).
- [x] **VALIDATED 2026-08-19, exact match:** gross volume vs Horizon `/liquidity_pools/{id}/trades` on canonical XLM/USDC (`a468d41d…0088`), closed day 2026-08-18 UTC — 526,863.8801956 XLM on BOTH sides (0.0000000% drift vs 1% tolerance; compared in asset-A units, which removes the price-timing drift source). Live reserves + total_shares also exact to the stroop. Original phrasing: The "≈" comparison is intentional — Horizon and the explorer both use per-operation extraction (Horizon parses `claimedOffers[]` the same way), so the only sources of drift are (a) USD price-snapshot timing (Horizon uses asset units, we multiply by `price_a` then divide back), (b) rounding in `NUMERIC(28,7)` arithmetic, (c) Soroban DEX swap events not yet covered in Phase 1 (Phase 3 scope). Drift > 1% on a classic-only pool indicates an extraction bug and blocks the AC.
- [x] `GET /liquidity-pools/:id/chart` returns non-null time series — verified live (XLM/USDC chart headline $4,166,296; TVL/volume/fees tabs).

**Phase 3:**

- [ ] Soroswap adapter scaffolding — DEFERRED out of this task: Soroban-LP reserves are contract-held `Balance(pool)` entries, not indexed; gated on the 0331 SAC ContractData-balance follow-up (see 2026-06-30 history entry).

**Common:**

- [ ] ~~Permanent / transient `EnrichError` mapping~~ — SUPERSEDED: no enrichment worker in the shipped design; CH errors degrade to NULL fields.
- [x] Tests: 232 unit tests green on the merged branch (price-leg mapping, formatters, filters; decode smoke vs CH 26.3) in place of the mock-price-API integration test the Lambda design called for.
- [ ] ~~CDK DepthAlarm thresholds~~ — SUPERSEDED: no queue exists.
- [x] Docs: ADR 0053 + endpoint-queries 18/19/21 + database-schema-overview + backend-overview + technical-design updated in the PR (ADR 0032 rule).
- [x] API types regenerated (CI freshness gate green throughout).
- [ ] ~~0196 backfill dedup ownership~~ — SUPERSEDED: the three columns are never written, so there is nothing to dedup.

## Future Work

None of these are spawned as tasks yet — surfaced for an explicit decision,
per the no-auto-task convention:

1. **Nullable chart points** — let the TVL line BREAK over unpriceable
   stretches instead of connecting across them (deferred review finding).
2. **Parallelise the two detail queries** (price context + USD analytics
   run serially; measured cheap, deferred with reason in PR #380 review).
3. **Honest `filter[min_tvl]`** — needs a materialized per-pool TVL
   (prices-side identity-keyed series or our own rollup); the API answers
   400 until then.
4. **Phase 3: Soroban-DEX TVL** — gated on the 0331 SAC
   ContractData-balance ingestion follow-up (contract-held reserves).
5. **Forming-bucket guard revisit** — when the prices-side coverage gate +
   raw coverage share ship, the unconditional one-bucket cut can become
   conditional.
6. **Prices-side watch items** (theirs, tracked in the R-prices note):
   duplicate-identity fix (5.5% of priced pools tainted), second pivot
   step (coverage 69→96 percent of active pools), 0171 omit-the-row,
   stale-but-real close.

## Notes

- **Required consultation with Oskar** — owns the price API. Phase 2 designed jointly: endpoint schema, freshness/cache contract, no-price-for-asset behavior, latency budget.
- **Blocked on price API.** Phases 1+2 ship atomically — Phase 1 alone produces no DB writes (SQS messages with no consumer until Phase 2 live).
- **USD rationale** — single comparable metric across mixed-asset pools; on-chain LP-graph traversal (peg-rooted hops) considered + rejected as too complex vs team-built price API.
