---
title: 'R: prices.* coarse-OHLCV freeze 07-21→08-03 + current_price_usd 6→13 columns'
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
---

# R: prices.\* coarse-OHLCV freeze 07-21→08-03 + current_price_usd 6→13 columns

Source: message from Oskar (prices owner), received 2026-08-03. Two parts.

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
