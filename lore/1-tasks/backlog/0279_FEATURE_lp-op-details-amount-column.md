---
id: '0279'
title: 'LP per-pool amounts: persist what the indexer already computes, un-hide the Amount column'
type: FEATURE
status: backlog
related_adr: ['0029']
related_tasks: ['0274', '0247', '0199', '0261', '0365', '0393', '0377']
tags:
  [
    phase-future,
    effort-large,
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
  - date: '2026-08-06'
    status: backlog
    who: karolkow
    note: >
      Recorded an adjacent finding from 0377 — see "Adjacent". The strict-send
      delivered amount is a field in the operation RESULT XDR
      (`PathPaymentStrictSendResult::Success.last.amount`) that the parser never
      lifts, even though it already reads the surrounding result. It needs no
      side table and no backfill, because the tx-detail heavy block is parsed at
      request time — so it is NOT Path B and not this task's scope, only its
      nearest owner. Unverified against mainnet; `operation.rs` is shared with
      the indexer, so a `details` key may also change persisted JSON. 0377
      deleted the permanently-empty `Received` row rather than reword it.
  - date: '2026-08-11'
    status: backlog
    who: stkrolikiewicz
    note: >
      Sized against prod (full history to ledger 63,827,054) — see "Measured
      on prod". Three corrections: (1) the "single-digit MB" size claim was
      off by three orders of magnitude — the table is ~860M rows / ~18 GB
      compressed (still fine as the ADR-0029 exception, but the real number
      must carry the argument); (2) the proposed ORDER BY silently loses
      amounts when one op takes the same pool twice (CAP-38 interleaved
      matching) — rows must be pre-summed per (op, pool, asset) before
      insert, and the amount should be SIGNED from the pool's perspective so
      one shape covers trade/deposit/withdraw; (3) offers (types 3/4/12)
      have ZERO pool crossings in all history AND in 82M ops from the last
      ~7 weeks, corroborated by a Horizon sample (19/19 recent LP trades are
      path payments) — keep the extractor generic, but tests/validation
      should target path payments. Backfill re-scoped to a TARGETED re-parse:
      only 13.15M distinct pool-active ledgers (20.6% of history), est.
      ~2-4h wall vs ~8-10h full sweep (0359 measured). Bumped effort-medium
      to effort-large per the 0199 triage (needs-backfill). Ownership overlap
      with 0199 Phase B still unresolved.
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

## Adjacent — the strict-send delivered amount is one field away (from 0377)

**Not this task's amount, and not Path B.** Recorded here because 0279 is the
nearest owner of "per-operation amounts" and the finding would otherwise be lost.

The transaction-detail operation card carried a `Received` row for
`PATH_PAYMENT_STRICT_SEND` that was permanently empty — the comment said the
delivered amount "is not derivable from claimedAtoms". True, and misleading: the
amount is not IN the atoms, it is next to them.
`PathPaymentStrictSendResult::Success` carries
`last: SimplePaymentResult { destination, asset, amount }`, and `last.amount` is
the delivered amount.

State of the code (checked 2026-08-06):

- the parser already reads operation results — `claim_atoms`
  (`crates/xdr-parser/src/operation.rs:186`), `tx_op_results` (`:97`);
- `details` gets `poolIds` / `claimedAtoms` / `sendAsset` / `destAsset`
  (`:260-261`, `:344-355`);
- `SimplePaymentResult` appears **only in that file's test fixtures**
  (`:1332-1353`) — never in extraction.

Why it is cheaper than this task: the tx-detail `heavy` block is parsed from the
archive at REQUEST time (runtime enrichment), so lifting `last.amount` needs no
side table, no backfill and no schema change — unlike the LP amounts above,
which have no result-XDR equivalent because deposits and withdrawals raise no
claim atoms.

Unverified, and the reason it was not done in 0377: `operation.rs` is shared
between the indexer and runtime enrichment, so adding a `details` key may change
the persisted JSON too; and it was never checked against a real mainnet
strict-send. 0377 deleted the empty row rather than reword it — when this is
built, the row is written fresh with a real value.

## Path decision (from 0247, re-confirmed 2026-07-30)

**Path B — ingest-side extraction.** Rejected alternatives:

- **Snapshot reserve-delta** (0247 "Path E") is exact only for ledgers with a
  single LP op per pool; measured per-op collision 25% → ~75% coverage ceiling.
- **Read-time XDR fetch** (0247 "Path A") on this hot list endpoint is too
  costly as a 25%-of-rows fallback.
- **Reuse `net_settled`** — see above, 86.7% mis-attribution.

Path B reads each op's own non-collapsed `LedgerEntryChanges` at ingest →
100% per-op, no collision, no hot-path S3. Needs a narrow side table plus an
ADR-0029 clarification: LP-only amounts are ~18 GB compressed (~2.6% of the
DB — see "Measured on prod"), not the multi-TB corpus ADR 0029 rejected.
(An earlier revision said "single-digit MB"; that was wrong by three orders
of magnitude and is corrected by the 2026-08-11 measurement.)

## Measured on prod — 2026-08-11 (full history to ledger 63,827,054)

Pool crossings per op type (`sum(length(pool_ids))` over
`operations_appearances`, fold-count upper bound in parens):

| op type                          |    ops |        crossings |
| -------------------------------- | -----: | ---------------: |
| path_payment_strict_send (13)    | 113.2M |  265.6M (280.0M) |
| path_payment_strict_receive (2)  |  53.7M |  157.8M (158.0M) |
| manage/passive offers (3, 4, 12) |      — |      **0, ever** |
| LP deposit (22) / withdraw (23)  |  1.21M | 1.21M (no atoms) |

- **Rows**: trades 423.4-438.0M crossings x 2 asset legs + d/w 1.21M x 2
  ≈ **~850-880M rows**.
- **Bytes/row**: the identical `(pool_id, ledger, tx)` prefix costs
  **11.68 B/row** compressed on `operation_pools` (system.parts, 619M rows).
  Adding `application_order` (~0.5 B), `asset_id` (2 distinct per pool run,
  ~1 B) and a high-entropy `amount` Int64 (~8 B) → **~21 B/row**, ~66 B raw.
- **Size**: ~860M x 21 B ≈ **17-19 GB compressed** (~57 GB uncompressed);
  DB is ~690 GB → **~2.6%**.
- **Backfill scope**: `uniq(ledger_sequence)` over `operation_pools` =
  **13,145,401 ledgers** (20.6% of history) — the re-parse can skip 4 of 5
  ledgers.
- Offers cross pools **never**: zero in all history, zero in 82M ops over
  the last ~7 weeks, and 19/19 recent Horizon `trade_type=liquidity_pool`
  trades are path payments. Multi-hop is the norm: strict-send averages
  2.35 pools per op.

## Implementation

1. **Schema** — `lp_operation_amounts (pool_id, ledger_sequence,
transaction_id, application_order, asset_id, amount)`, ReplacingMergeTree,
   `ORDER BY (pool_id, ledger_sequence, transaction_id, application_order,
asset_id)`. Pool-leading so the pool page seeks rather than scans, and
   partitioned like its siblings. Two constraints the first draft missed:
   - **Pre-sum atoms per (op, pool, asset) before insert.** One op can take
     the same pool multiple times (CAP-38 interleaved matching); raw
     per-atom rows share the full ORDER BY key and the RMT would silently
     collapse distinct fills. Summing per op is deterministic on replay
     (live/backfill dedup stays correct) and is all the Amount column needs.
   - **`amount` is SIGNED, from the pool's perspective**: trade = one leg
     `+` (asset entering the pool) one leg `-` (leaving); deposit = both
     `+`; withdraw = both `-`. One shape covers all three event kinds and
     the sign disambiguates direction without an extra column.
2. **Trades** — emit rows from the atoms `gross_volume_a_by_pool` already
   walks, instead of only summing them. Both legs: the function reads
   `amountA` only, so the asset-B side needs adding. Keep the extractor
   covering offers (free via `claim_atoms`), but validation effort goes to
   path payments — offers have zero pool crossings ever (see "Measured on
   prod").
3. **Deposits and withdrawals** — **not** covered by step 2. The comment on
   `gross_volume_a_by_pool` says it: LP deposits and withdrawals carry no
   claim atoms. The op body holds only the caller's `max`/`min` bounds, so the
   actual amounts come from that op's own `LedgerEntryChanges`. This is the
   genuinely new extraction and the bulk of the remaining work.
4. **Backfill** — historical rows; reuse the 0266 backfill worker, which
   already shares `gross_volume_a_by_pool` with live ingest so both paths stay
   identical. Re-scoped 2026-08-11 to a **targeted** re-parse:
   - Feed the runner `SELECT DISTINCT ledger_sequence FROM operation_pools`
     (13.15M ledgers, 20.6% of history) instead of a full sweep — est.
     **~2-4h wall** on the box (0359 full-history baseline: ~8-10h with
     s5cmd fan-out; scales with fetch volume).
   - **Pre-create the table on prod before deploying the parser** (the
     `accounts_recent` 500 lesson) — live ingest starts writing the moment
     the indexer restarts.
   - Purely additive: no existing table touched, no EXCHANGE TABLES, no
     `repair-tier1` obligation, indexer keeps running throughout; rollback
     is `DROP TABLE`.
   - Built-in verification: `sum(amount)` per (pool, ledger) over the
     A-side legs must equal `liquidity_pool_snapshots.gross_volume_a`
     (both derive from the same atoms) — one SQL comparison closes the
     backfill gate. Per-row spot checks can use the E3 heavy-fields
     response as a second in-house oracle besides Horizon.
5. **API** — LP-tx rows gain the amounts. No `?expand=` parameter: that was a
   Path A artefact, and with the data in the DB the amounts are just part of
   the row.
6. **Frontend** — un-hide the "Amount" column in `PoolTransactions.tsx` and
   render the deposit / withdraw / trade shapes.
7. **ADR 0029** — record the LP-only exception with the measured size.

Adjacent cleanup, deliberately NOT in scope: once `lp_operation_amounts`
exists, `operation_pools` becomes a value-less projection of the same key
prefix — a retirement candidate, but only after the read path has migrated
and soaked. Note it, don't do it here.

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
