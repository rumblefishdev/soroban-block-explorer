---
title: 'R: prices.* read traps — OHLCV freeze, current_price_usd v13 sentinels, partial-enrichment skew'
type: research
status: mature
spawned_from: ../README.md
spawns: []
tags: [prices-api, clickhouse, contract-change, incident, sentinels]
links: []
history:
  - date: '2026-08-04'
    status: mature
    who: stkrolikiewicz
    note: >
      Recorded from Oskar's 2026-08-03 message (prices owner). Two items: the
      coarse-OHLCV freeze incident (answers the staleness question pinned in
      the 2026-07-22 activation note) and the additive current_price_usd
      contract change with its sentinel semantics. Mature on arrival — this is
      the provider's own account, not our hypothesis.
  - date: '2026-08-04'
    status: mature
    who: stkrolikiewicz
    note: >
      Added §3 (partial-enrichment skew) and retitled: this file is now the
      collected prices.* READ TRAPS, not just the freeze incident. §3 is ours,
      diagnosed from the raw price_ohlcv_1h rows during the 0199 self-review,
      and is a standing property rather than an incident — close_usd is baked
      by a later pass, so the views' `close_usd > 0` filter makes a fresh
      bucket's volume-weighted average run over an arbitrary subset of its
      rows; on yXLM's 13:00 hour the surviving row was a 0.764-unit dust print
      at 1.3085 vs ~0.170 real, briefly quadrupling the newest 1h TVL point.
      Weighting is sound (the same print is a no-op against 42,038 units at
      12:00) — it is the filter. Also recorded under §2 that
      current_price_usd 0-sentinels native XLM itself, which is why 0199
      detail reads the 1h series instead. Reported to the prices owner.
---

# R: prices.\* read traps — freeze, sentinels, partial-enrichment skew

Collected traps for anyone reading the `prices.*` views in-cluster.

- **§1 + §2** — from the prices owner's message of 2026-08-03 (an incident
  and a contract change; their account, not our inference).
- **§3** — ours, diagnosed 2026-08-04 from the raw `price_ohlcv_1h` rows
  during the 0199 self-review. Not an incident: a standing property that
  recurs on every freshly-landed bucket.

## 1. Coarse OHLCV tables were frozen 2026-07-21 → 2026-08-03

All six coarse tables (`price_ohlcv_15m`, `_1h`, `_4h`, `_1d`, `_1w`, `_1M`)
stopped accepting rows at **2026-07-21 02:44 UTC** and were recovered
**2026-08-03 09:57 UTC**. `price_ohlcv_1m` was never affected. Cause:
per-table background merge/mutation scheduler inside ClickHouse stopped being
scheduled; 8 of 9 rollup MVs reported success while rolling up stale input,
so nothing alarmed. Recovery was per-table DETACH + ATTACH scoped to
`prices.*`; verified against pre-incident backups, delta zero.

**This closes the open question from the 2026-07-22 activation note.** The
`price_usd_series` bucket stuck at `2026-07-21 00:00` was this incident, not
steady state. No stale-price discriminator needs designing for normal
operation. Oskar's side is adding a freshness alarm (on data freshness, not
MV status — the gap that let it run silently for ten days).

**The 07-21 → 08-03 window does NOT backfill itself.** Coarse tables are
current from 08-03 forward; the ~12-day hole is being closed by an
incremental pre-roll on their side — they will ping when done.

Consequences for 0199:

- **Nothing to recompute on our side.** ADR 0053 compute-at-read means the
  TVL/volume charts self-heal the moment their pre-roll fills the window. This
  is precisely the scenario Decision #1 was bought for.
- **Do not run AC validation (Horizon drift, coverage measurements) over
  2026-07-21 → 2026-08-03** until their pre-roll is confirmed — the window
  will show provider-side holes, not our bugs.
- `change_7d_pct` (see below) reads 0 for every asset until ~2026-08-10
  (needs 7 days of `price_ohlcv_1h` history); sooner if the pre-roll lands
  first.
- Cheap honesty option for the API/UI contract: surface the price bucket
  timestamp in the chart/detail response. The incident proves staleness can
  be silent for ten days even on a monitored service. Optional, not a gate.

## 2. `prices.current_price_usd` goes 6 → 13 columns (additive)

Applied on their side shortly after the message. Existing six columns keep
exact names, types, and positions:

```
asset_kind, asset_code, issuer_address, contract_address, price_usd, updated_at
```

Appended:

```
price_xlm, change_24h_pct, change_7d_pct, volume_24h_usd, market_cap_usd,
vwap_24h, sources
```

Rollback on their side is a single atomic view replacement, no read-side
window.

### Hard rule for every consumer we write

**Never decode this view positionally.** No `SELECT *` into fixed-arity
tuples, no positional clickhouse-crate row structs, no
`INSERT INTO t SELECT * FROM …`. Always pin an explicit named column list.
(Checked 2026-08-04: no code in `crates/` reads `prices.*` yet, so this is a
design rule for the 0199 read path, not a live break.)

### Sentinels, not NULLs

`current_price_usd` columns are non-nullable; "unavailable" and a real value
share a type. This is the opposite convention from our own Phase-2 matrix
(which deliberately chose NULL ≠ 0) — do not let the two bleed into each
other when writing SQL.

| Column                                           | Type                              | Sentinel semantics                                                                                                                                                                                                                                                                     |
| ------------------------------------------------ | --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sources`                                        | String holding JSON               | `''` = MV never rewrote the row — **NOT valid JSON, a parser throws; guard before parsing**. `'{}'` = refreshed but no source survived (no priced 24h candle / outlier filter). Populated = JSON object keyed by source; numbers serialised as **strings** to preserve Decimal(38,14). |
| `price_xlm`                                      | Decimal(38,14)                    | `0` = unavailable (no XLM market or un-enriched tip); indistinguishable from a true 0.                                                                                                                                                                                                 |
| `change_24h_pct` / `change_7d_pct`               | Decimal(10,4), ±999999.9999 clamp | `0` means BOTH "unavailable" AND "genuinely flat" — treat 0 as "no signal". `change_7d_pct` is all-0 until ~2026-08-10 (freeze aftermath).                                                                                                                                             |
| `volume_24h_usd` / `market_cap_usd` / `vwap_24h` | Decimal(38,14)                    | `0` = unavailable. `market_cap_usd` is 0 whenever circulating supply is absent (best-effort join).                                                                                                                                                                                     |

### Relevance to the pinned TVL scope

The TVL read path goes through `price_usd_series` (structured `asset_kind`,
NULL + discriminator semantics), so the sentinel table above mostly does not
touch it. It bites the moment anything reads `current_price_usd` — e.g. a
"current TVL" column on the LP list, or any use of the new 24h/7d fields.
Recorded now so the trap is documented before the SQL exists, like the
`prices.assets` empty-code trap before it.

**Follow-up 2026-08-04:** `current_price_usd` turned out unusable for the
0199 detail path for a reason the table above predicts but does not spell
out — `price_usd = 0`, the unavailable-sentinel, covers **native XLM
itself**. Every XLM-leg pool (the majority) would have read NULL TVL. Detail
reads the last `price_usd_series_1h` close instead. Revisit when the
prices-side Current Price Updater (their 0039) prices native.

## 3. Partial enrichment can make a dust print the whole bucket price

Diagnosed 2026-08-04 while reviewing the 0199 chart. Distinct from the two
items above and **not** an incident — it is a standing property of how the
series views are built, so it recurs on every freshly-landed bucket.

`price_usd_series` / `_1h` collapse a bucket's many rows — one per source
and quote pair — into one number, volume-weighted:

```
sum(close_usd * volume_base) / sum(volume_base)   WHERE close_usd > 0
```

`close_usd` is **not** written when the candle lands; a separate enrichment
pass bakes it (the design's "USD materialized write-time", ADR 0053 §2). So
a fresh bucket is **partly enriched**, and because the view filters
`close_usd > 0`, the weighted average is taken over an **arbitrary subset**
of the bucket's rows.

Measured on prod, yXLM (`GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55`):

| time  | what the view returned            | why                                                                         |
| ----- | --------------------------------- | --------------------------------------------------------------------------- |
| 13:29 | `1.30853` for bucket 13:00        | the only row with `close_usd > 0` was a **0.764-unit dust print** at 1.3085 |
| 14:13 | bucket 13:00 **absent**           | by then all five 13:00 rows read `close_usd = 0` again                      |
| —     | ~0.170 in every neighbouring hour | the real price (yXLM tracks XLM)                                            |

That 7.7× price briefly quadrupled the newest 1h TVL point (109 USD against
~25) while the pool's reserves sat flat — so the error is entirely
price-side.

**The volume weighting itself is sound.** The identical dust print sits in
the 12:00 bucket next to 42 038 units of real volume and moves the weighted
close by nothing (0.16979). It is the `close_usd > 0` filter changing the
population, not the maths.

Consequences for us:

- **Do not add our own outlier filter.** Prices owns that (their views
  already claim an inter-source one), and diverging would make our numbers
  disagree with their API — the opposite of the single-source-of-truth
  reason we read their views in-cluster at all.
- **The 0199 chart carry-forward does not cover this.** `ASOF` substitutes a
  _missing_ candle, not a _wrong_ one. (Once the 13:00 bucket vanished
  entirely, carry-forward did kick in and served the correct 12:00 close —
  but that is luck, not protection.)
- Reported to the prices owner 2026-08-04, with the suggestion to either
  hold a bucket out of the view until all its rows are enriched, or include
  the unenriched rows in the weighting once they land.
