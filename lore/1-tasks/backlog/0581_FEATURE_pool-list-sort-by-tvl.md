---
id: '0581'
title: 'FEATURE: sort the pool list by TVL — stored per pool, one priced leg is enough for x·y=k'
type: FEATURE
status: backlog
related_adr: ['0053']
related_tasks: ['0374', '0199', '0401', '0562']
tags: [layer-clickhouse, layer-api, priority-medium, effort-medium]
links: []
history:
  - date: '2026-09-24'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0374 (PR 4b review, decision 89 A). Activity ordering fixes
      the buried Soroban pools but is a weak ranking; value first is the
      default users expect. Starts after 0374 PR 4c, which reads Soroban
      reserves.
---

# FEATURE: sort the pool list by TVL

## Summary

Order `GET /v1/liquidity-pools` by the pool's USD value, most valuable first,
with the activity ledger (0374 PR 4b) as the tie-break and the order for pools
with no value. TVL has to be stored per pool to page on it, and a constant
product pool can be valued from one priced leg, which lifts coverage from
about half of the pools to most of them.

## Context

- Activity ordering (0374 PR 4b) is a proxy: the order reshuffles every few
  seconds, and a dust pool that swapped a minute ago outranks a large pool
  that swapped two minutes ago.
- TVL is computed at read for the page's ~20 pools only (task 0199, ADR 0053),
  so it cannot be a keyset paging key. The same gap is why `filter[min_tvl]`
  answers 400 today.

### Coverage today (measured 2026-09-24, production, classic pools)

|                                        | pools           | share |
| -------------------------------------- | --------------- | ----- |
| all classic pools                      | 53,552          | 100%  |
| every leg priced → TVL today           | 26,421          | 49%   |
| exactly one leg priced                 | 19,208          | 36%   |
| no leg priced                          | 7,923           | 15%   |
| pools with a participant / of them TVL | 27,201 / 13,737 | 51%   |

What the prices service covers (verified 2026-09-24, production). It indexes
the trades: of the 4,573 assets that swapped in a classic pool in the last
24 h, 3,450 carry a USD price from the last 48 h (97% of the swap legs),
1,088 have candles but no USD price, and 35 had their last candle just
outside the 24 h window (e.g. `XRPBANK20`, last daily candle the day
before). Its 1-minute candles trail the chain by about a minute.

So a missing TVL is not missing trades. Two mechanisms:

- **No trade, no new candle — and the USD value is frozen at trade time.** A
  candle's `close_usd` is the trade price times the quote asset's USD rate AT
  THAT MOMENT. An asset that last traded against XLM a year ago still has
  the same market price in XLM terms, but its stored USD value used the XLM
  rate of then: $0.375 on 2025-09-24 against $0.202 on 2026-09-23, so it
  would read 1.86× too high. The 48 h cap in `usd_analytics.rs` guards
  exactly that (and the 2026-07-21..08-03 provider freeze). The price did
  not go stale; its USD conversion did.
- **One-hop USD conversion.** The service converts through the quote asset
  of each trade. `H2` (issuer `GBGRBCUB…`) had 11,964 swap legs in 24 h, all
  against other unpriced assets of the same issuer, so every candle carries
  `close_usd = 0`.

Both vanish when the pool prices its own legs: its reserve ratio IS the
current price of one leg in the other, whatever the age of the last
external candle, and only the anchor leg (almost always XLM or USDC, both
priced every minute) needs a USD rate.

### Split of the work (decided 2026-09-24, 91 B)

| classic pools (53,552)              | pools        | who            |
| ----------------------------------- | ------------ | -------------- |
| every leg has a fresh USD price     | 26,366 (49%) | nothing to do  |
| one leg fresh (usually XLM or USDC) | 19,255 (36%) | this task (2×) |
| no leg fresh                        | 7,931 (15%)  | stays unvalued |

The last group is asset pricing (a last trade re-expressed at today's rate of
its quote, routed through intermediate pairs), which belongs to the prices
service, not to us: computing it here would make us a second price producer
reading the service's raw candle tables. No request is made for now; those
pools keep `tvl = null` and sort after the valued ones.

## Implementation

1. **Value from one priced leg for constant product pools.** A classic pool is
   x·y=k, where both sides are worth the same at the pool's own price, so
   `TVL = 2 × reserve × price` of any priced leg. Exact at the pool price, not
   an estimate, and no staleness question for the other leg (see above). Two priced legs keep today's sum. Soroban: only constant
   product families qualify (stable and concentrated pools do not). Expected
   classic coverage ≈ 85% (26,421 + 19,208 — estimate from the table).
2. **Store it.** A per-pool TVL table refreshed on a schedule (house precedent:
   refreshable MVs `accounts_recent_mv`, `balance_aggregates_mv`), keyed by
   pool, with the price bucket it used.
3. **Order the list** by stored TVL (NULLs last), then activity ledger, then
   pool id; the keyset carries all three.
4. **`filter[min_tvl]`** becomes answerable from the same table (today 400).
5. Detail and list keep computing the displayed TVL at read, or read the
   stored one — decide which, one source for both.

## Acceptance Criteria

- [ ] One-priced-leg valuation for constant product pools, with a unit test
      against a hand-computed pool
- [ ] Coverage measured after the change, classic and Soroban separately
- [ ] List ordered by TVL with a stable keyset; two real pages repeat nothing
- [ ] `filter[min_tvl]` served or its 400 kept with the reason updated
- [ ] Read cost of a list page measured against the 0374 PR 4b baseline
- [ ] Docs: canonical SQL 18, database schema docs (ADR 0032)
