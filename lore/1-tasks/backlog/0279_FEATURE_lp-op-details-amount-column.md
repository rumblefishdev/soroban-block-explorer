---
id: '0279'
title: 'LP per-pool amounts: persist what the indexer already computes, un-hide the Amount column'
type: FEATURE
status: backlog
related_adr: ['0029']
related_tasks: ['0274', '0247', '0199', '0261', '0365', '0393']
tags:
  [
    phase-future,
    effort-medium,
    priority-medium,
    layer-indexer,
    layer-api,
    layer-frontend,
    milestone-2,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/371'
history:
  - date: '2026-06-03'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0274 future work (gap #2). 0274 closed with #2 deferred
      after a deep-dive confirmed per-op LP amounts are not in the DB and
      cannot be served cheaply today. Blocked on the path decision from
      research 0247 (read-time XDR fetch vs ingest-side extraction).
  - date: '2026-06-03'
    status: backlog
    who: stkrolikiewicz
    note: >
      0247 concluded → path decision = **Path B (ingest-side extraction)**.
      Measured on prod CH: per-op collision rate 25% (5.75% per-group), which
      quantifies the "reserve-delta unreliable" note above — a pure-CH-SQL
      snapshot-delta approach (0247 "Path E") caps at ~75% per-op coverage.
      Product requires 100% per-tx amounts, so read-time XDR (Path A) as a
      25%-of-rows hot-path fallback is rejected. Ingest-side extraction reads
      each op's own (non-collapsed) LedgerEntryChanges → 100% per-op, no
      collision, no hot-path S3. Now unblocked. See 0247 notes
      (R-clickhouse-snapshot-delta, S-recommendation).
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Re-scoped against the code, because three tasks landed after this was
      written and its central premise had gone stale. The parser DOES extract
      claim-atom amounts now (0261), and the indexer already resolves them per
      pool in `gross_volume_a_by_pool` — it just sums them away. So the trade
      half is nearly built and only the write granularity is wrong. Also
      deleted the Path A build step, which contradicted the Path decision
      directly above it, and dropped `?expand=lp_op_details` from the title —
      that parameter was a Path A artefact. Re-tested the "maybe 0393 already
      has it" shortcut on prod and killed it: 86.7% of pool transactions cross
      more than one pool, so the per-transaction `net_settled` cannot be
      attributed to a pool. Linked issue #371, which is this feature reported
      from outside.
---

# LP per-pool amounts: persist what the indexer already computes

## Summary

Gap #2 of the FE→API audit (task 0274). `GET /v1/liquidity-pools/{pool_id}/transactions`
must return per-pool amounts so the pool-tx table's **"Amount"** column
(present in Figma, deliberately hidden since the MVP) can show
`deposit 5,000 XLM + 2,000 USDC` / `trade 100 XLM → 40 USDC` / withdrawals.

Reported from outside as **issue #371** — "you can get a lot of info on
stellar.expert's view right from the get go, instead of having to go to any
details view". Our four columns are Event / Hash / Account / Time: who and
when, never what.

## State of the code — re-checked 2026-07-30

The 2026-06 write-up said the parser has no extraction and this is "a real
feature, not a field add". Half of that is no longer true.

**Already built:**

- `xdr_parser` attaches `claimedAtoms` to path-payment / offer ops with a
  `poolId` and per-atom amounts (`crates/xdr-parser/src/operation.rs:215-261`,
  task 0261).
- The indexer already walks those atoms **and already resolves them per pool**:
  `gross_volume_a_by_pool` (`crates/db-clickhouse/src/persist/stage.rs:70-96`)
  reads `(poolId, amountA)` per atom, keyed by the raw 32-byte pool id — then
  `+=` them into one number per pool for `liquidity_pool_snapshots.volume`.
  **The per-transaction attribution exists for one line and is discarded.**
- `operations_appearances.pool_ids` already carries the crossed-pool list per
  operation (`init.sql:601-606`, tasks 0261/0268).
- `operation_pools` (task 0365) indexes `(pool_id, ledger_sequence,
transaction_id)` — pool-leading, so a pool page seeks it cheaply.

**Still missing:** somewhere to put `(pool, transaction, asset, amount)`, the
write that fills it, the backfill, the read, and the column.

So this is no longer "build an extractor". It is **stop throwing the extracted
value away**.

## What does NOT work — checked, so nobody re-proposes it

0393 added `operation_asset_appearances.net_settled` and it is live and
populated (13.2M rows over the last 20k ledgers, 99.999% non-null, measured
2026-07-30). It is tempting: join `operation_pools` to it on
`(ledger_sequence, transaction_id)` and the Amount column falls out with no
new ingest work.

**It is wrong for 6 rows out of 7.** `net_settled` is keyed
`(asset_id, ledger_sequence, transaction_id)` — no pool dimension and no
operation dimension. Joining on the transaction fans _every_ asset the
transaction touched onto _every_ pool it crossed. Measured on prod
(last 20k ledgers):

| pool transactions         |             |           |
| ------------------------- | ----------: | --------: |
| touching exactly one pool |      26 939 |     13.3% |
| touching more than one    | **176 217** | **86.7%** |

A multi-hop path payment through three pools would show all three pools'
legs on each pool's row. This is the same collision 0247 measured from the
other direction, and it re-confirms Path B rather than replacing it.

There is a second reason not to build on that column: 0411 removed the
`net_settled` read from list endpoints because it scanned ~26M rows per page,
and 0417 owns the `(ledger, tx)`-leading companion that would make such a read
affordable. Even if the attribution were right, the read is not ready.

## Path decision (from 0247, re-confirmed 2026-07-30)

**Path B — ingest-side extraction.** Rejected alternatives:

- **Snapshot reserve-delta** (0247 "Path E") is exact only for ledgers with a
  single LP op per pool; measured per-op collision 25% → ~75% coverage ceiling.
- **Read-time XDR fetch** (0247 "Path A") on this hot list endpoint is too
  costly as a 25%-of-rows fallback.
- **Reuse `net_settled`** — see above, 86.7% mis-attribution.

Path B reads each op's own non-collapsed `LedgerEntryChanges` at ingest →
100% per-op, no collision, no hot-path S3. Needs a narrow side table plus an
ADR-0029 clarification: LP-only amounts are single-digit MB, not the multi-TB
corpus ADR 0029 rejected.

## Implementation

1. **Schema** — `lp_operation_amounts (pool_id, ledger_sequence,
transaction_id, application_order, asset_id, amount)`, ReplacingMergeTree,
   `ORDER BY (pool_id, ledger_sequence, transaction_id, application_order,
asset_id)`. Pool-leading so the pool page seeks rather than scans, and
   partitioned like its siblings.
2. **Trades** — emit rows from the atoms `gross_volume_a_by_pool` already
   walks, instead of only summing them. Both legs: the function reads
   `amountA` only, so the asset-B side needs adding.
3. **Deposits and withdrawals** — **not** covered by step 2. The comment on
   `gross_volume_a_by_pool` says it: LP deposits and withdrawals carry no
   claim atoms. The op body holds only the caller's `max`/`min` bounds, so the
   actual amounts come from that op's own `LedgerEntryChanges`. This is the
   genuinely new extraction and the bulk of the remaining work.
4. **Backfill** — historical rows; reuse the 0266 backfill worker, which
   already shares `gross_volume_a_by_pool` with live ingest so both paths stay
   identical.
5. **API** — LP-tx rows gain the amounts. No `?expand=` parameter: that was a
   Path A artefact, and with the data in the DB the amounts are just part of
   the row.
6. **Frontend** — un-hide the "Amount" column in `PoolTransactions.tsx` and
   render the deposit / withdraw / trade shapes.
7. **ADR 0029** — record the LP-only exception with the measured size.

## Acceptance criteria

- [x] 0247 path decision recorded — Path B, re-confirmed against prod 2026-07-30
- [ ] `lp_operation_amounts` populated on live ingest, both legs, trades and
      deposits/withdrawals
- [ ] Amounts verified against Horizon on a **multi-pool** path payment — the
      86.7% case, not a single-pool one
- [ ] Backfill run; live and backfill paths produce identical rows for a
      replayed range
- [ ] Pool-page read seeks on `pool_id`; `read_rows` measured and recorded
- [ ] FE "Amount" column un-hidden, rendering deposit / withdraw / trade
- [ ] The stale comment in `PoolTransactions.tsx` is corrected — it currently
      points at task **0249**, which is about destroying AWS infrastructure
- [ ] **Docs updated** — `docs/architecture/**` schema + endpoint contract per
      ADR 0032, and the ADR-0029 exception
- [ ] **API types regenerated** — touches `crates/api/**`
