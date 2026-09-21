---
id: '0562'
title: 'Closed classic pools — a lifecycle column, and the API stops listing them as live'
type: FEATURE
status: backlog
related_adr: ['0055', '0057']
related_tasks: ['0210', '0374', '0523', '0401']
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
  - date: '2026-09-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from task 0210's first snapshot-seed dry-run. The parser fix there
      makes an erased pool's snapshot zero; whether the pool is shown at all is
      this task.
  - date: '2026-09-18'
    status: backlog
    who: karolkow
    note: >
      Measured the closed set and split it by how the pool died: 11,382 of
      14,171 ended normally, 2,789 were erased indirectly. A lifecycle ledger
      alone cannot tell the two apart on a page, and the cause is not
      recoverable after the fact — the parser has to emit it. Scope gains a
      reason column.
---

# Closed classic pools — lifecycle column and API

## Summary

A classic pool (`LiquidityPoolEntry`) is erased when its last pool-share
trustline goes. Nothing in our tables says so: `liquidity_pools` keeps the row
and the pool API lists every pool with its newest snapshot. Production
(2026-09-17) has 12,607 classic pools whose newest snapshot is 0 reserves and 0
shares — closed normally — shown as live, empty pools. Task 0210 zeroes the
snapshots of another 1,381 erased by an authorization revocation, which today
show their old reserves and TVL.

Horizon marks an erased pool `deleted = true`, filters it from every read and
purges it after 100 ledgers; stellar-etl emits `deleted = true`. Our holdings
already model the same lifecycle (ADR 0055: `closed_at_ledger`).

## Context

- **Meta semantics** (task 0210, 2026-09-17): a pool `removed` is always preceded
  by a `state` of the same key; the extractor now writes a zero snapshot at the
  removal ledger and a pool row from the `state` params.
- **A pool id is SHA-256 of the pair and fee**, so an erased pool can be
  re-created under the same id; a later `created` must reopen it.
- `participant_count` (from `lp_positions`) is already 0 for these pools, but
  `lp_positions` lacks holders from before our floor (task 0523), so it cannot
  decide liveness.
- The pool list sorts newest first by `created_at_ledger` (task 0401 plans to
  store it); a closed-pool filter interacts with that ordering and with the
  `min_tvl` filter.

## Measured split (2026-09-18, production, read-only)

53,459 pools have snapshots; **14,171** have a newest snapshot of 0 reserves
and 0 shares. The pool list applies no filter on shares or reserves, so all
14,171 are served as live. Split by whether any operation touched the pool in
its closing ledger (`operation_pools`):

| how the pool died                                | pools      | share |
| ------------------------------------------------ | ---------- | ----- |
| direct — an operation names the pool             | **11,382** | 80.3% |
| indirect — nothing in that ledger names the pool | **2,789**  | 19.7% |

**Direct (11,382)** is one shape: a pool withdraw and a `change_trust` in the
same transaction (11,370 and 11,228 transactions of the set). The last holder
takes everything out and drops the pool trustline. Ordinary end of life.

**Indirect (2,789, over 2,633 ledgers)** is the revocation shape — the
operation acts on the holder's trustline, the pool falls out as a side effect.
Against 2,633 ledgers sampled from the same range:

| operation              | closing ledgers | sampled ledgers | ratio     |
| ---------------------- | --------------- | --------------- | --------- |
| `allow_trust`          | 1,444           | 375             | **3.85x** |
| `set_trust_line_flags` | 659             | 527             | 1.25x     |
| `change_trust`         | 2,442           | 2,213           | 1.10x     |
| `account_merge`        | 427             | 520             | 0.82x     |

Only `allow_trust` stands out, and 0210's own census agrees in kind (1,661
`allow_trust` against 10 `set_trust_line_flags`). **This is ledger-level
co-occurrence, not per-pool attribution** — a ledger holds many transactions,
so it names the group's character, not any single pool's cause. Per-pool
attribution needs the transaction meta, the way 0210's census was built; SQL
over our tables cannot recover it.

Not yet in the 14,171: the 1,381 revocation-erased pools of 0210, which still
carry their stale reserves because that repair ships after the next deploy.
Once repaired they join the indirect group: ~15,550 pools to hide or mark, of
which ~4,170 (27%) died by revocation.

## Implementation

- [ ] `liquidity_pools.closed_at_ledger Int64 DEFAULT 0` (ALTER first, then the
      writer — ADR 0055 order): set by a pool `removed`, reset to 0 by a later
      `created`.
- [ ] `liquidity_pools.closed_reason` beside it (also `DEFAULT`, same ALTER):
      the ledger alone cannot tell an ordinary exit from a revocation, and the
      measurement above shows one in five is a revocation — a page that only
      says "closed" drops the part a reader would care about. The cause is not
      derivable after the fact, so the parser emits it at the `removed`, where
      it still has the change that erased the entry. Populated for history by
      the same pass that fills `closed_at_ledger`; a row it cannot decide stays
      at its default rather than guessing.
- [ ] Backfill: from the checkpoint (`snapshot-seed` already knows every live
      pool; a pool of ours absent from it is closed at the checkpoint) or from
      the zero newest snapshot — pick the one that states a true ledger.
- [ ] API: list and detail — hide closed pools by default or show them as
      closed. UX decision first (`/ux-expert`).
- [ ] `balance_aggregates_mv` needs nothing (a closed pool's reserves are 0);
      confirm.
- [ ] Docs: schema overview (`liquidity_pools`), API field docs.

## Acceptance Criteria

- [ ] No erased pool is listed as live by the pool API.
- [ ] A pool re-created under the same id is live again.
- [ ] Closed count matches the checkpoint: pools of ours absent from the
      snapshot = pools with `closed_at_ledger > 0`.
- [ ] Every closed pool carries a reason, or an explicit "not determined" —
      never a reason the writer guessed. The counts land near the measured
      split (~80% ordinary exit, ~20% revocation); a wide divergence means the
      parser reads the wrong change, not that the estimate was off.
