---
id: '0648'
title: 'BUG: a classic pool whose only participant is the issuer gets no `assets` row for its credit leg'
type: BUG
status: backlog
related_adr: ['0051']
related_tasks: ['0571', '0530', '0374']
tags:
  ['clickhouse', 'indexer', 'data-integrity', 'priority-low', 'effort-small']
links: []
history:
  - date: '2026-10-16'
    status: backlog
    who: karolkow
    note: >
      Split out of 0571 (part 1), which keeps only the SAC half. The two
      parts share a symptom, not a cause or a fix. Text moved as written on
      2026-09-22; nothing re-measured.
---

# BUG: issuer-only classic pool legs have no `assets` row

## Summary

A classic pool whose only participant is the issuer of its credit asset
stores a leg surrogate that joins to nothing in `assets`. Split from 0571,
which keeps the SAC half.

**Cause.** A classic credit `assets` row is produced only from a trustline
change (`detect_classic_credit_assets`, `crates/xdr-parser/src/state.rs:1112`)
or a SAC sighting. The issuer of an asset may open a pool-share trustline
without holding a trustline to its own asset — stellar-core's
`ChangeTrustOpFrame` skips the check when the source `isIssuer` — so a pool
whose only participant is the issuer changes no asset trustline, and its
credit leg's surrogate has no row.

**Measured 2026-09-16:** 2 of 52,974 classic pools — `XLM/PIF`
(`C305DB2D401608D9732783120AE9BE061B95FCEB6A8E3EAD45E24D1686AA7252`) and
`XLM/SUR810` (`92415CA8B582ECF78B31CAFD17CDEC19E30005411570D882EA1F5F399CF0F1C6`).
Both confirmed on chain (`getLedgerEntries`): empty pools, one pool-share
trustline, held by the issuer. No balances, no deposits.

**Repaired 2026-09-22** before the pair columns can be dropped (PR #455 —
after the drop the code and issuer of these legs would exist nowhere in the
database): two rows inserted into `assets`, `id` = the value already in the
pool's `legs`, code and issuer from the pair columns. Orphaned classic legs
after the insert: 0 of 107,044.

**Fix still owed:** when a classic pool row is staged, emit an `assets`
identity row for each credit leg, the way the SAC deploy path does
(`push_asset`). Both callers of `classic_pools::build_pool_rows` need it —
the indexer (`crates/db-clickhouse/src/persist/stage.rs:1113`) and the
checkpoint seed (`crates/backfill-runner/src/snapshot/pools.rs:79`).
**Sequenced after PR #455 merges, as a separate PR** (decided 2026-09-22):
PR #455 rewrites the same builder while dropping the pair columns.

## Acceptance Criteria

- [ ] A classic pool whose only participant is the issuer stages an `assets`
      row for its credit leg — unit test on `build_pool_rows`'s callers, both
      the indexer and the seed
- [ ] After deploy, orphaned classic legs stay 0 (query below), on `default`
      and `testnet`
- [ ] **Docs updated** — `docs/architecture/**` section on where an `assets`
      row comes from, or `N/A — reason`

## Measurement query

```sql
-- classic legs with no assets row (expect 0)
WITH p AS (SELECT pool_id, argMax(legs, last_updated_ledger) legs,
                  argMax(pool_kind, last_updated_ledger) kind
           FROM liquidity_pools GROUP BY pool_id)
SELECT countIf(leg NOT IN (SELECT id FROM assets))
FROM p ARRAY JOIN legs AS leg WHERE kind = 0;
```
