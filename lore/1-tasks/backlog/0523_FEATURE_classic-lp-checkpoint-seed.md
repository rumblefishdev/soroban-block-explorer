---
id: '0523'
title: 'Classic LP checkpoint seed — pre-floor positions and pools, one snapshot-seed flow'
type: FEATURE
status: backlog
related_adr: ['0057']
related_tasks: ['0374', '0463', '0468', '0499', '0502']
tags:
  [
    backend,
    clickhouse,
    liquidity-pools,
    data-quality,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: '2026-08-29'
    status: backlog
    who: karolkow
    note: >
      Re-scoped: this id first held "Soroban AMM batch 2", which duplicated
      the existing 0516 umbrella + 0518 Soroswap adapter (search-the-concept
      rule violated; caught same day). Now holds the classic LP seed built
      and then deliberately reverted out of the 0374 branch (Aquarius-only
      scope) — resurrect from feat/0374 commits a3bc8e63 + cee04d5a.
---

# Classic LP checkpoint seed

## Summary

The LP half the 0463 checkpoint seed deliberately deferred: pool-share
trustlines and `LiquidityPoolEntry` records are CLASSIC ledger entry types
older than our ingest floor, so a holder (or a whole pool) untouched since
the floor has no row on our side. Seed them from a checkpoint snapshot —
**inside the ONE `snapshot-seed` flow** (fold into `seed_command`; no side
subcommand, no second decode — decision karolkow 2026-08-29).

## Measured gap (2026-08-29, chain-validated via raw getLedgerEntries)

- 2,681 pools know <50% of their shares' owners (1,164 live); 597 miss
  1–50%; ZERO overcounts — the defect is one-sided.
- `share_percentage` itself is CORRECT (denominator chain-exact) — the K4-6
  verdict lives in 0374's record.
- Pre-floor pools with no post-floor entry change have no `liquidity_pools`
  row at all — invisible pools, count to be reported by the dry-run.
- 94.8% of `lp_positions` already carry `first_deposit_ledger = 0` (task
  0468's "Since ledger 0" bug) — this task's read-side `nullIf` fix plus the
  documented sentinel semantics close 0468's data half; its FE half (don't
  render a dead `/ledgers/0` link) rides along.

## Implementation (built once — resurrect, then fold)

`feat/0374` commits `a3bc8e63` (decoder `NetPool` + corrections + the then-
subcommand) and `cee04d5a` (sentinel read: `nullIf(first_deposit_ledger,0)`
→ null on the wire). Reshape per the one-flow decision: `seed_command`
builds LP corrections from the SAME decoded `NetworkState`, one summary,
one `--execute`, one refusal gate. Keep: versioning on each entry's own
ledger (0492 rule), ghosts REPORTED not corrected (no RPC-probe evidence
yet), the protocol-identity decode check
(`sum(pool_shares_trust_line_count) == decoded live shares`), self-heals
restating the pair's real first deposit (whole-row RMT).

Mind 0499 (merge lp_positions into balances, parked): this seed writes the
CURRENT table; if 0499 ever lands first, the corrections target moves.

## Acceptance Criteria

- [ ] one `snapshot-seed` run covers balances + accounts + LP world (no
      separate subcommand)
- [ ] dry-run summary reviewed; invariant check OK; `--execute` run [K]
- [ ] coverage re-measured after: positions-sum vs snapshot total agree
      (modulo documented ghosts), pre-floor pools visible
- [ ] 0468 closed by the sentinel read + FE dead-link fix
- [ ] docs: backfills runbook row + lp_positions sentinel comment
