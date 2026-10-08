---
id: '0632'
title: 'BUG: config-factory pool fee is frozen at creation — 13 of 20 pools show a stale fee'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0584']
tags: [indexer, liquidity-pools, data-integrity, priority-medium, effort-medium]
links: []
history:
  - date: '2026-10-07'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0584 (its fee-drift criterion). Measured on production:
      13 of 20 config-factory pools store a fee the chain no longer has.
---

# Config-factory pool fee frozen at creation

## Summary

`liquidity_pools.fee_bps` for a config-factory Soroban pool is written once,
from the pool's `CONFIG.total_fee_bps` at registration
(`crates/db-clickhouse/src/persist/stage/soroban_pools.rs:164-195`), and never
refreshed. The pool admin can rewrite `CONFIG`, so the stored fee goes stale
and every fee figure built on it (pool detail, fee revenue) is wrong.

## Measurement (2026-10-07, read-only)

Population: `pool_kind = 1 AND pool_type_raw = '0'` — 20 pools from 6
factories; for every one `last_updated_ledger` equals its creation ledger.

```sql
SELECT hex(pool_id), fee_bps, last_updated_ledger, deployment_id
FROM liquidity_pools FINAL
WHERE pool_kind = 1 AND pool_type_raw = '0'
```

On-chain fee: the pool's persistent `Symbol("CONFIG")` entry via
`getLedgerEntries` (`stellar contract read --key CONFIG --durability
persistent`, mainnet RPC, ledger ~64,815,830); one pool cross-checked by
simulating `query_config`.

| Stored → on chain       | Pools |
| ----------------------- | ----- |
| 1000 → 50               | 3     |
| 300 → 100               | 2     |
| 300 → 50                | 1     |
| 100 → 50                | 6     |
| 1000 → no fee in CONFIG | 1     |
| equal                   | 7     |

All 13 stale pools belong to one factory (`deployment_id
2808977402438572953`); its admin rewrote `CONFIG` around ledgers 58.43M and
63.25–64.20M. `CAZ6W4WHVGQBGURYTUOLCUOOHW6VQGAAPSPCD72VEDZMBBPY7H43AYEC` is no
longer a pool: its code was upgraded to a sweep contract and its `CONFIG`
rewritten to another shape (ledger 63,767,534).

## Implementation

- Indexer: when a config-factory pool's `CONFIG` entry changes, re-read
  `total_fee_bps` and write a new `liquidity_pools` version. Decide what a
  pool whose `CONFIG` no longer parses becomes (the sweep case).
- Backfill the 20 pools once (re-read `CONFIG` at the tip or replay the
  changes); name the `docs/backfills.md` path.
- Decide whether fee history matters (fee revenue over a period priced with
  the fee in force then) or only the current fee.

## Acceptance Criteria

- [ ] Every config-factory pool's stored fee equals its on-chain `CONFIG` fee
      (the query above, re-run against RPC)
- [ ] A `CONFIG` rewrite after deploy updates the stored fee without a manual step
- [ ] The no-longer-a-pool case has a recorded outcome
- [ ] **Docs updated** — `docs/architecture/**` where pool fees are described

## 2026-10-08 — why running the pool's own functions does not fix it

The contract executor (task 0620) is given a contract's instance and code
only. A config-factory pool keeps nothing in its instance: the newest stored
instance of `C2D85230…B16F` decodes to 0 storage keys, so `CONFIG` and the
reserves are persistent entries, and `query_config` would answer "needs
contract data" (task 0633). The fix stays a re-read of `CONFIG` whenever a
ledger writes that persistent entry; the write itself is in the ledger's
changes, which is also when the current metadata trigger (an instance
change) would never fire.
