---
id: '0669'
title: 'Report TPS as operations per second'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0045', '0553']
tags: [priority-medium, effort-small, layer-api, layer-frontend-pages]
links: []
history:
  - date: '2026-11-10'
    status: backlog
    who: stkrolikiewicz
    note: >
      SDF asked why our TPS is about half of Chainspect's. Chainspect counts
      operations, not transactions; the team decided our TPS should too.
  - date: '2026-11-10'
    status: active
    who: stkrolikiewicz
    note: >
      Activated to start implementation (Step 1 query first).
  - date: '2026-11-12'
    status: completed
    who: stkrolikiewicz
    note: >
      Shipped in production-2026.11.12-1 (Compute + SPA) and on testnet by
      hand. PRs #728, #729, release #730. TPS equals Horizon at the first
      head served on each (prod 107.8833, testnet 3.5667). The release diff
      showed 0553's CloudWatch change undeployed (indexer failure alarm blind
      since 2026-11-06); shipped as production-2026.11.12-2-CloudWatch.
---

# Report TPS as operations per second

## Summary

The home KPI and the top nav show "TPS" as transactions per second over the
last 60 s, about 60–67 on mainnet. Chainspect, the number people compare us
with, shows about 130 for Stellar because it counts operations. Our TPS will
mean the same: operations per second, including the operations of failed
transactions. The window stays 60 s (team, 2026-11-12); Chainspect's is ~1 h.

## Stan teraz

- Done: live on production and testnet since 2026-11-12. Nothing open.
- In force: label "TPS"; value = operations/s incl. failed transactions;
  window = ledgers closed in the 60 s before head, first one not summed.

## Context

- `GET /v1/network/stats` → `tps_60s` is `sum(ledgers.transaction_count)`
  over the ledgers closed in the last 60 s, divided by max − min `closed_at`
  (`crates/api/src/network/queries.rs`). `transaction_count` is
  `tx_processing.len()`, so failed transactions are included.
- Chainspect puts the ledger range behind its Stellar figure in the page data
  (`tps.start` / `tps.finish`, `txns`). For `65384312..65385031` (720
  ledgers, 3,595 s) it reported 130.7805 "tx/s" and 470,156 "txns". Horizon
  for the same range: Σ `tx_set_operation_count` = 470,156 exactly (130.78/s);
  transactions incl. failed = 237,301 (66.0/s). Its "Max TPS (100 blocks)"
  of 218.088 also matches `tx_set_operation_count`, not transactions.
- Our ClickHouse agrees. Over the 720 ledgers before head on 2026-11-09:
  `count()` = 60.7 tx/s, `sum(operation_count)` = 132.85 op/s.
  `transactions.operation_count` is the envelope's operation count (inner tx
  for fee-bumps) and is set for failed transactions too, the same population
  as Horizon's `tx_set_operation_count`.
- The window length is not what makes the 2× gap: within one hour the 60 s
  values had a median of 66.6 tx/s against an hourly average of 61.7.
- The 60 s window over-counts by ~8%: it sums ~12 ledgers but divides by ~11
  inter-ledger intervals. Over 720 ledgers the same effect is 0.14%, and
  Chainspect divides the same way (470,156 / 3,595 s).

## Implementation

The plan's 720-ledger window shipped in #729 first and was replaced by the
60 s window before merge (Emerged 3).

- `fetch_stats`: the window is the ledgers closed in the 60 s before the head
  ledger closed, bounded to the 200 ledgers below head. TPS is
  `sum(transactions.operation_count)` over the window's ledgers after its
  first, divided by the first-to-head `closed_at` span. Pinned to head; no
  `now64()`.
- `NetworkStats.tps_60s` → `tps`; API types regenerated.
- `ChainOverview.tsx` and `TopNav.tsx` read `tps`; caption "Last 60s";
  tooltip "Operations per second, including failed transactions".
- PRs: #728 (cache tests moved), #729, release #730.

## Verification

- Prod ClickHouse vs Horizon `tx_set_operation_count`, same window and span:
  65438237 → 113.3667, 65437700 → 101.0333, 65000000 → 188.6667,
  60100720 → 159.5714 before merge; 65440965 → 107.8833, the first head the
  deployed API served. All equal. Testnet 5693724 → 3.5667 on both sides.
- Cost on prod: p50 27 / p95 29 ms over 40 heads, against 25 / 29 ms for the
  old statement; 30.5 ms on the first live runs.
- Deployed code: the API Lambda binaries (prod 13:37:52 UTC, testnet
  14:03:20 UTC) carry `AS tps_window` and no `tps_60s`; both SPA bundles
  carry the tooltip and no `tps_60s`.
- Not checked: the page in a browser. Turnstile never resolves in the in-app
  pane, and the Chrome extension was not connected.

## Acceptance Criteria

- [x] For a range Chainspect publishes, the Step 1 query returns its `txns`
      exactly and its `tps.value` to two decimals (the 720-ledger version;
      the shipped 60 s window was checked against Horizon).
- [x] Home KPI and top nav show the new value and caption, checked on prod
      data through `cargo run -p api --bin local`.
- [x] **Docs updated** — `GET /network/stats` in
      `docs/architecture/backend/backend-overview.md` and
      `docs/architecture/technical-design-general-overview.md`.
- [x] **API types regenerated** — the `NetworkStats` field changes.
- [x] Live on production and testnet; the served value equals Horizon.

## Issues Encountered

- **Indexer failure alarm blind on production.** The release's `cdk diff`
  listed 0553's CloudWatch change as undeployed: the indexer has logged
  `alarm = "indexer_failure"` since the Compute deploy of 2026-11-06
  13:26 UTC, while the live filter matched `ch_write_failure`. It missed 5
  failure lines on 2026-11-06 15:54–15:55 UTC. Shipped as
  `production-2026.11.12-2-CloudWatch`; the new filter matches that line.
- **Stale `libs/ui/dist`** failed the pre-commit `web:typecheck` (TS6305).
  `trash libs/ui/dist` + `pnpm nx reset` fixed it; not a code problem.

## Design Decisions

### Emerged

1. **`transactions FINAL`, not `LIMIT 1 BY`**: same TPS at 10 heads; the whole
   statement took p50 25 ms vs 95 ms on prod (2026-11-10). On a key range
   `FINAL` merges only overlapping parts, and head parts do not overlap.
   Reasoning in the `fetch_stats` comment.
2. **Cache tests moved** to `network/cache/tests.rs` in their own commit: the
   fixture changes, and CLAUDE.md forbids inline tests.
3. **Window 60 s, not 720 ledgers** (team decision, daily 2026-11-12). The
   first ledger is left out of the sum: N ledgers span N-1 intervals, which
   over-stated TPS by +7.2% on a 13-ledger window (prod, 2026-11-12).

## Notes

- Other Stellar tools, checked 2026-11-09: StellarChain.io shows
  transactions per second (incl. failed, last 50 ledgers); StellarExpert
  shows no TPS, only per-ledger transactions and operations.
