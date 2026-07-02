---
id: '0342'
title: 'FEATURE: supply display convention — exclude XLM burn-void + flag spam/ceiling balances'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0331']
tags:
  [
    clickhouse,
    assets,
    data-quality,
    frontend,
    phase-future,
    effort-small,
    priority-low,
  ]
links:
  - crates/api/src/assets/queries_ch.rs
history:
  - date: '2026-07-02'
    status: backlog
    who: claude
    note: >
      Spawned from the 0331 OPS run — surfaced when summing account balances into the
      unified `balances` / `balance_aggregates`. Both issues are REAL on-chain data
      (Horizon-verified), not migration bugs; this is a read/display-layer convention.
---

# FEATURE: supply display convention — exclude XLM burn-void + flag spam/ceiling balances

## Summary

The 0331 unified `balances` sum is faithful to on-chain state, which makes the displayed
`total_supply` diverge from the figures users expect (CoinMarketCap / StellarExpert) for two
reasons — **both REAL on-chain data, not bugs**: (1) the XLM burn-void account, and (2) spam
tokens holding i64-ceiling balances. Decide + implement a display convention. Raw `balances`
data stays chain-faithful; this is read/display only.

## Context

Spawned from the 0331 OPS run (2026-07-02). Summing `account_balances_current` → `balances` →
`balance_aggregates` surfaced two data-quality display issues. Both were verified REAL on-chain
(Horizon-confirmed) and faithfully migrated — the OLD `asset_aggregates` carried the same:

- **XLM burn-void** — account `GALAXYVOIDAOPZTDLHILAJQKCVVFMD4IKLXLSZV5YHO7VY74IWZILUTO`
  (home_domain "If you stare into the abyss...") holds **55.44B XLM** (Horizon-confirmed EXACT:
  `55442115247.4347086`). So the on-chain native sum = **~104.78B** (100B genesis + inflation;
  the 2019 "burn" sent ~55B to this void address rather than protocol-destroying it).
  CMC/StellarExpert cite **~50B circulating** by EXCLUDING this account. Our `balance_aggregates`
  native `total_supply` will read ~104.78B unless we exclude the void.
- **Spam ceiling** — ~5,823 classic balances at exactly the i64 ceiling (`922337203685.4775807`
  = `2^63 − 1` stroops) on fake tokens (fake "USDC"/"XRP"/"GOLD"/metals). Real Circle USDC
  (561k holders) has ZERO ceiling rows — the spam lands on its own spam `asset_id`, so real
  assets are unaffected, but the spam assets show absurd supply (trillions/quadrillions),
  polluting any supply-sorted asset list.

Neither is a migration bug — both are faithful on-chain data.

## Implementation

- Decide the "supply" definition per surface: on-chain-total vs circulating (exclude burn-void).
- **Native XLM:** exclude the burn-void account (`GALAXYVOID…`) from the displayed supply to match
  CMC/StellarExpert circulating (~50B); keep the on-chain total available if useful.
- **Spam/ceiling:** heuristic or curated flag to hide/deprioritize scam tokens (e.g. i64-ceiling
  balances, or an allow/deny list) in supply-sorted asset lists. Coordinate with existing
  asset-enrichment / spam-flag work if any.
- Do NOT alter `balances` (chain-faithful) — read/display layer only.

## Acceptance Criteria

- [ ] Native XLM displayed supply matches the chosen convention (documented: circulating ~50B or on-chain ~105B).
- [ ] Spam/scam tokens with ceiling balances no longer pollute the top of supply-sorted lists.
- [ ] `balances` / `balance_aggregates` raw data unchanged (chain-faithful).
- [ ] The chosen convention is documented (which surfaces show which figure).
