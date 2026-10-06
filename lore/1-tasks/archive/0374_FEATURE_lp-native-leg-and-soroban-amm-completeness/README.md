---
id: '0374'
title: 'LP completeness: native XLM leg match + Soroban-AMM union + share% recompute'
type: FEATURE
status: done
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-medium, layer-api, liquidity-pools]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/405'
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker. Bundles F-B/K2-2, K3-5, K4-6.'
  - date: 2026-08-14
    status: backlog
    who: karolkow
    note: >
      Linked issue 405 (add Soroban AMM protocols). Rewrote K3-5: the union is
      the last step, not the work — no Soroban pool state is indexed today.
  - date: '2026-08-21'
    status: active
    who: karolkow
    note: >
      Activated. First-protocol scope confirmed reachable from data already in
      `soroban_events`; the backfill is an in-DB INSERT ... SELECT, not an
      S3 re-parse.
  - date: '2026-09-07'
    status: active
    who: karolkow
    note: >
      Pool write path DEPLOYED to production — release PR 452 merged
      (`098bef9d`), tag `production-2026.09.07-1`, one combined window with
      task 0540. `pool_state_changes` and `pool_instance_state` created and
      verified byte-identical to init.sql; both take live rows. No indexer
      pause was needed: the three `liquidity_pool_snapshots` columns the new
      writer drops were given DEFAULT NULL first (metadata-only), which makes
      old and new writers simultaneously valid, so the DROPs move to after the
      backfills and a rollback stays free. Backfills, closure layers and the
      read half remain.
  - date: '2026-10-02'
    status: done
    who: karolkow
    note: >
      Read half shipped in production-2026.10.02-1 (release #601): Soroban
      pool activity (#574, #586-#588), volume and fees for every pool
      including three- and four-leg ones (#589, #598), Phoenix auto-unbonded
      skip (#571). Verified on production: Aquarius, Soroswap, Phoenix and a
      three-leg pool serve reserves, TVL, 24h volume, a volume chart and
      activity. Volume within +0.034 % of the protocol's per-pool hourly
      statistics over 356 Aquarius pools; two-leg pools unchanged by #598
      (768/768 identical, detail and charts). Leftovers live in 0523, 0530,
      0590, 0598, 0599, 0606, 0607, 0611, 0612.
---

# LP completeness

## Summary

Make liquidity-pool activity complete: match the native XLM leg (currently
unmatchable → 21.7% of pools invisible), union Soroban-AMM pools into
`/liquidity-pools`, and recompute stale `share_percentage`.

## Acceptance Criteria

- [x] native XLM leg matches → pools visible — F-B / K2-2 (fixed by 0440/0470 along the way; verified on prod 2026-08-29, see the worklog)
- [x] Soroswap pools indexed (reserves + volume) and unioned — K3-5 (verified on production after `production-2026.10.02-1`: reserves, TVL, activity, 24h volume and a 91-day volume chart; 212 days of protocol volume within +1.78 % of DefiLlama)
- [x] Aquarius pools indexed and unioned — K3-5 (code complete + verified; ships with the final-phase deploy/backfills)
- [x] Classic / Soroban filter on the pool list — K3-5 (filter[pool_kind] + FE dropdown)
- [x] share_percentage correct (or confirmed already correct) — K4-6 (confirmed correct; the real gap was holder coverage — snapshot-seed-lp built; the seed run moved to task 0523)

## Close-out (2026-10-02)

### Shipped last (production-2026.10.02-1)

- Activity for Soroban pools: one row per pool event, `event_index` in the
  cursor, amounts raw with each leg's `decimals` (#574 structure, #586 move,
  #587 one `PoolEvent` and one decimals rule, #588 behaviour).
- Volume and fee revenue for Soroban pools, detail 24h and chart (#589).
- Three- and four-leg pools: each trade counted on its **traded leg**, the
  lowest-index leg the trade wrote a row for, scaled and priced as that leg
  (#598). Two-leg pools: identical before and after on all 768.

### Design decisions

#### Emerged

1. **No "volume not priced" state for multi-leg pools.** #591 proposed an
   API flag `volume_priceable = (legs == 2)`; a devil's-advocate pass showed
   18,295 of 18,296 multi-leg trades move exactly one leg in and one out, so
   the flag encoded a limit of the SQL, not of the data. #591 closed;
   prototype kept on `prototype/0374-w211-volume-empty-state`.
2. **Lowest traded leg (A1) over the average of both sides (A2).** Indexers
   price swaps by the average (Uniswap v2, Messari/Curve) or stablecoin side
   first (Balancer). Against the protocol's hourly statistics every rule was
   within 0.0–0.7 %; A1 keeps two-leg and classic pools unchanged, A2 needs a
   classic `gross_volume_b` column and a backfill — task 0612.
3. **Review fix inside #598:** an idle Soroban pool whose leg A has no
   decimals keeps reporting unknown volume, not `$0.00`.

### Issues encountered

- The 779-pool old-vs-new comparison exhausted the `dev_read` hourly
  read-bytes quota (4 TiB) at six threads; rerun at two. The chart's TVL
  query, not the volume query, carried most of the bytes (0612 review lead).
- Spawned ids 0605 and 0608 collided with tasks opened the same day;
  renumbered to 0611 and 0612.
- The protocol's own daily total runs 5.9 % below the sum of its per-pool
  statistics (recorded in 0607).

### Future work (each a task)

- 0523 — snapshot seed run for classic pool holders.
- 0530 — drop the pair columns (PR 6).
- 0590 — `total_shares` into `pool_state_changes`.
- 0598 — classic pools join `pool_movements`.
- 0599 — Soroban pools missing from the registry (RaumFi).
- 0606, 0611 — activity paging edge cases.
- 0607 — protocol counts and volume vs `pool_movements`.
- 0612 — liquidity-pool module review, then average-of-both-sides volume.

## Notes

- [S-worklog-2026-07-13-to-2026-10-02.md](notes/S-worklog-2026-07-13-to-2026-10-02.md) — the full working record: research, step-by-step decisions, verification passes, reviews.
- [R-aquarius-first-research.md](notes/R-aquarius-first-research.md), [R-w1-amounts-vs-asset-transfers.md](notes/R-w1-amounts-vs-asset-transfers.md)
