---
id: '0279'
title: 'LP per-pool amounts: persist what the indexer already computes, un-hide the Amount column'
type: FEATURE
status: completed
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
  - date: '2026-08-16'
    status: active
    who: stkrolikiewicz
    note: >
      Backfill complete — 211/211 partitions, zero gaps against
      operation_pools. Measured: 929,971,594 rows, 11.36 GiB, 13.12 B/row,
      no duplicates left (RMT collapsed the overlapping re-runs), no
      OPTIMIZE FINAL needed. The 2026-08-11 estimate of ~860M rows was
      right; the mid-run revision to 1.24B was not. Run incident and its
      fixes tracked in 0488.
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
  - date: '2026-08-11'
    status: active
    who: stkrolikiewicz
    note: >
      Activated — go decision recorded. Ownership resolved: #371 belongs wholly
      to this task; 0199 §"Also owns" now just points here. Design pinned
      without further team round-trips: row per (op, pool, asset), `amount`
      Int64 raw stroops SIGNED from the pool's perspective (see step 1 for
      the type rationale), pre-summed per op. Run plan recorded in step 4:
      reproduce the 0359 setup (s5cmd pre-fetch + ~6 external runner
      processes on disjoint ranges — the runner has no --workers flag and
      >6 was measured no-faster on the 24-core box), write ONLY this table
      (targeted-write, 0266 pattern) to keep the no-repair-tier1 claim
      valid, and calibrate the 2-4h estimate with a one-partition pilot
      before quoting a completion time.
  - date: '2026-08-11'
    status: active
    who: stkrolikiewicz
    note: >
      Schema + trade emission implemented (branch
      feat/0279_lp-op-details-amount-column): `lp_operation_amounts` in
      init.sql (DDL parse-checked on prod CH 26.3.10 via EXPLAIN AST),
      `LpOperationAmountRow`, `pool_fill_amounts` in stage.rs beside the
      `gross_volume_a_by_pool` it mirrors, writer wiring, 2 unit tests,
      schema docs per ADR 0032. SIGN CONVENTION VERIFIED ON PROD rather
      than read off the spec: the XDR atom is written from the offer
      owner's side (`assetSold` is taken FROM the pool, `assetBought` sent
      TO it), confirmed on ledger 63,904,097 of pool `41270552…` (XLM/TF)
      — Horizon reports the pool selling XLM and that ledger's snapshot
      moves `reserve_a` down by exactly the summed sold amount while
      `reserve_b` rises. That same check CORRECTED the backfill gate
      recorded here on 08-11 morning: `gross_volume_a` is gross, so the
      cross-check is `sum(abs(amount))` over the A legs, not the positive
      ones (see step 4). Deposits/withdrawals still pending — they carry
      no claim atoms and come from `LedgerEntryChanges`.
  - date: '2026-08-11'
    status: active
    who: stkrolikiewicz
    note: >
      Deposits/withdrawals done — the write side is complete, both event
      kinds, one row shape. `pool_delta_details` (operation.rs) subtracts the
      pool entry's before/after images from the op's OWN meta, which yields
      the pool-side sign for free (deposit `+/+`, withdrawal `-/-`) and
      handles both boundaries by construction (created = no pre-image,
      Removed = no post-image). It rides `extract_operations`, which already
      hands every op its own changes — the `cb_details` precedent — so NO new
      StageInputs field, NO indexer/backfill wiring, and the amounts surface
      on the tx-detail page for free (untyped `details` JSON, so no OpenAPI
      codegen either). Verified on prod: deposit 274467346725453825 in ledger
      63,904,409 of pool `52d16f5b…` — Horizon reports 0.0529699 XLM +
      37.6376180 SSLX deposited and the snapshots move `reserve_a`/`reserve_b`
      by exactly those figures (that ledger's `gross_volume_a` is NULL,
      confirming deposits stay out of trade volume). 348 xdr-parser + 90
      db-clickhouse tests green, clippy clean. Next: API read + FE column,
      then the targeted backfill.
  - date: '2026-08-11'
    status: active
    who: stkrolikiewicz
    note: >
      API + FE done, and the BACKFILL PILOT RAN — locally, no prod writes
      (`backfill-runner/examples/pilot_lp_amounts.rs`, 120 real mainnet
      ledgers through the real `parse_ledger` + `stage::prepare`). It
      overturned two things this task asserted. (1) SCOPE: "20.6% of
      history, skip 4 of 5 ledgers" divided by the CHAIN tip, but our
      ingest starts at 50,457,424 — the pool-active set is 13.16M of the
      14.55M ledgers we hold, i.e. 90.5%, and 100% at the tip. Targeting
      saves ~10%, not ~80%. (2) TIME: measured 17.3 / 45.1 / 39.5
      ledgers/sec/core at ledgers 63.8M / 57.0M / 51.0M → ~128 core-hours →
      **~20h at 6 workers (band 13-35h)**, not 2-4h; the old figure scaled
      0359's ~8-10h by ledger COUNT, which assumes uniform per-ledger cost
      and treated an estimate as a measurement. Cost is not monotonic in
      time — the mid-2025 sample is the cheapest and half the bytes. Row
      estimate UNCHANGED (~850-880M): that one comes from complete-table
      aggregates and the pilot's ~65 rows/ledger agrees. Also found a
      BLOCKER: the runner has no targeted-write mode (0266 used a bespoke
      harness), so one must be built or the run writes every table and
      re-arms the repair-tier1 obligation. API/FE detail: rows carry
      `amount_a`/`amount_b` as SIGNED raw-stroop STRINGS (a JSON number is
      a browser double — a leg above 2^53 stroops would lose digits, and
      `reserve_a`/`total_supply` already set that precedent), the read
      degrades to blank instead of 500 when the table is absent (deploy
      order), and the FE renders `in → out` for swaps, `X + Y` for
      deposits/withdrawals, blank for unknown. Remaining: targeted-write
      mode, then deploy (table pre-create FIRST) and the run.
  - date: '2026-08-17'
    status: completed
    who: stkrolikiewicz
    note: >
      Closed by reconciling the criteria against what production actually
      does — the task still claimed the Amount column was hidden while it had
      been rendering for days. Verified live: pool LCCC…MXS7 shows
      `Deposit 0.1260385 XLM + 0.0000045 YxT`, `Trade 0.0000001 YxT →
      0.0027931 XLM`, and a three-operation transaction as three lines; the
      multi-pool Horizon check landed on tx 24d04961…c5b5, which crosses
      three pools and agrees to the stroop. Shipped across
      production-2026.08.17-1 and -2 (the latter carrying 0489, without which
      every credit12 leg was missing). Two criteria are NOT ticked: the
      `read_rows` measurement is handed to 0491, which re-keys this read and
      already carries it; withdraw rendering is inferred from the shared
      code branch, not observed. Issue #371 stays OPEN — this task was one
      third of it, and 0491 holds the rest.
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
ADR-0029 clarification: LP-only amounts are **11.36 GiB compressed, ~1.6% of
the DB** (measured after the backfill completed — see "Backfill executed"),
not the multi-TB corpus ADR 0029 rejected. (An earlier revision said
"single-digit MB", wrong by three orders of magnitude; the 2026-08-11
estimate of ~18 GB was the right order and 60% high.)

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
  **13.16M ledgers**. An earlier revision called that "20.6% of history —
  the re-parse can skip 4 of 5 ledgers"; **that was wrong** (corrected
  2026-08-11 by the pilot). It divided by the CHAIN tip (63.8M), but our
  ingest starts at the Soroban go-live: `ledgers` holds
  **50,457,424 → 63,915,942 = 14.55M rows**, so the pool-active set is
  **90.5% of what we actually have** — and 100% of recent history (40
  consecutive tip ledgers sampled, every one pool-active). Targeting saves
  ~10%, not ~80%.
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
   - **Type: `Int64`, raw stroops, not Nullable** (decided 2026-08-11).
     Source fields are XDR `int64` (claim atoms, trustline balances) and a
     per-op sum is bounded by the pool's reserve (itself `int64`), so no
     overflow is reachable; AMM pools are classic-only → always 7 decimals,
     scaled at read like `net_settled`/balances. Rejected: `Int128` (only
     needed for Soroban i128 token amounts, which classic pools cannot
     carry — doubles the heaviest column for an impossible case) and
     `Decimal128(7)` (matches `gross_volume_a` in snapshots, but that is a
     read-model choice for Lambda USD math; fact tables store raw ints,
     and the verification query is one `toDecimal128(...)/1e7` cast away).
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
     (13.16M ledgers) instead of a full sweep. **The saving is ~10%, not
     ~80%** — see the scope correction above — so targeting is a nicety,
     not the plan's load-bearing idea.
   - **Wall estimate: ~20h at 6 workers, band 13-35h** (pilot, 2026-08-11;
     supersedes the "~2-4h" this line used to carry). That number came
     from scaling 0359's ~8-10h by ledger COUNT, which assumes every
     ledger costs the same AND treated 0359's figure as measured when it
     was itself an estimate. The pilot measures the real inner loop:
     **17.3 / 45.1 / 39.5 ledgers per second per core** at ledgers 63.8M /
     57.0M / 51.0M — cost tracks ledger density and is NOT monotonic in
     time (the mid-2025 sample is the cheapest and half the bytes). Mean
     ~0.035 s/ledger → ~128 core-hours → ~21h across 6 workers. Excludes
     the S3 fetch and the CH insert; measured on a laptop, so the box may
     differ. Harness: `backfill-runner/examples/pilot_lp_amounts.rs`.
   - **Pre-create the table on prod before deploying the parser** (the
     `accounts_recent` 500 lesson) — live ingest starts writing the moment
     the indexer restarts.
   - Purely additive: no existing table touched, no EXCHANGE TABLES, no
     `repair-tier1` obligation, indexer keeps running throughout; rollback
     is `DROP TABLE`.
   - Built-in verification: **`sum(abs(amount))`** per (pool, ledger) over
     the A-side legs must equal `liquidity_pool_snapshots.gross_volume_a`
     (both derive from the same atoms) — one SQL comparison closes the
     backfill gate. Per-row spot checks can use the E3 heavy-fields
     response as a second in-house oracle besides Horizon.
     **ABS, corrected 2026-08-11** (an earlier revision of this line said
     "the positive A legs"): `gross_volume_a` is a GROSS figure —
     `append_pool_claims` takes each atom's A-side amount whichever way the
     swap went, both non-negative — so a pool that only sold A that ledger
     has every A leg negative here and a positives-only sum reads 0 against
     a non-zero volume. Known legitimate mismatch: an op crossing the SAME
     pool in BOTH directions nets out at this table's per-op grain while
     `gross_volume_a` counts both crossings gross.

   **Run plan (recorded 2026-08-11, after the go decision):**

   - **Parallelism = the 0359 setup**: s5cmd pre-fetch of the S3 files +
     K external runner processes on disjoint `--start/--end` ranges — the
     runner itself has NO `--workers` flag (docs/backfills.md §2). Use
     K=6: measured 2026-07 on the 24-core box, more than 6 was no faster
     (disk-bound). The 2-4h estimate ASSUMES this setup; single-process
     would be >10h.
   - **Targeting**: the runner is range-based, it cannot take a ledger
     list today. Two options, in order of preference: (a) list-driven
     s5cmd pre-fetch of only the 13.15M pool-active files + verify how
     the range loop behaves on a missing file (or add a small
     --ledger-list mode); (b) fallback with zero runner changes — ranges
     from the first pool-active ledger (AMMs exist since protocol 18,
     ~Nov 2021) to tip. With the scope correction above this fallback
     costs ~10% more than the targeted form, not 2x — our range starts at
     50.46M anyway, so both shapes parse almost the same ledgers.
   - **BLOCKER, found 2026-08-11: the runner has no targeted-write mode.**
     `run --reindex` puts every ledger through the full staging pipeline
     and writes every table. 0266 did NOT use a runner flag for this — it
     ran a bespoke harness ("Targeted write only — do NOT run the full
     persist pipeline") and INSERTed the rows it wanted. So a
     write-only-this-table mode has to be built before the run starts;
     without it the "no repair-tier1" claim below is void, because a full
     reindex re-arms the Tier-1 MIN-corruption trap.
   - **Write ONLY `lp_operation_amounts`** (targeted-write, the proven
     0266 pattern) — NOT a full `--reindex`. This is the condition that
     keeps the "no repair-tier1" claim valid: the parallel-run Tier-1
     MIN-corruption trap applies to tables the run writes, and this run
     writes one table with no MIN-semantics columns. A full reindex on
     the side would silently re-arm that obligation.
   - **Pilot before quoting a time**: run one recent (densest) 500k
     partition first, extrapolate by the partition's share of the target
     set — same calibration as 0304 tiling. Check whether the 0359 raw
     ledger files still sit on the box disk (fast-path skips the fetch
     entirely if they do).

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

## Wire shape: per operation, not per transaction — decided 2026-08-12

The row on the pool page is a TRANSACTION, so the first cut summed the
amounts across every operation that transaction ran against the pool. Review
caught that the sum sits under an Event chip naming ONE category
(`classifyLpTx` resolves a bundled deposit + path payment to "Deposit"), so
the figure read smaller than the deposit it was captioned with, and where the
trade leg dominated an asset the signs came out `+/-` and rendered as a swap
arrow beneath a Deposit chip.

First fix withheld the amount for such transactions, on this file's own
assumption that bundling is rare. **Measured instead — it is 8.2%**:

```sql
SELECT count() AS pairs, countIf(ops > 1) AS multi
FROM (SELECT transaction_id, arrayJoin(pool_ids) AS pool, sum(amount) AS ops
      FROM operations_appearances
      WHERE notEmpty(pool_ids) AND ledger_sequence > 63700000
      GROUP BY transaction_id, pool)
-- 8,491,737 pairs / 697,529 multi-op = 8.214%
```

One row in twelve. Hiding that many leaves a permanent hole indistinguishable
from "backfill hasn't reached here", and recreates the click-through #371 was
raised about. So the wire carries `amounts: [{application_order, amount_a,
amount_b}]` — one entry per operation — and the FE renders a line each. 92% of
rows are one-element lists and look unchanged.

Two consequences worth keeping:

- The read query got SIMPLER, not harder: no outer aggregation, no
  `uniqExact` ops counter, no suppression branch. One `GROUP BY` that exists
  only to dedup the RMT.
- The "rare bundling" comment in `classifyLpTx` was the source of the wrong
  assumption; it now carries the measured number instead.

Storage is unaffected — the table was always keyed per operation, since the
RMT key demands it. Only the read path changed.

## Backfill executed — measured, not estimated (2026-08-16)

The historical re-parse is **complete**: every one of the 211 partitions from
the dataset floor (50,457,424) to the live floor carries ≥99% of the ledgers
`operation_pools` has for the same range. Zero gaps.

|                        |                           estimated |    **measured** |
| ---------------------- | ----------------------------------: | --------------: |
| Rows                   |   850–880M (later revised to 1.24B) | **929,971,594** |
| On disk, compressed    |                              ~18 GB |   **11.36 GiB** |
| Bytes / row            | 21 (extrapolated) → 14.74 (partial) |       **13.12** |
| Compression ratio      |                                   — |          **5×** |
| Active parts           |                                   — |             135 |
| Share of the 690 GB DB |                               ~2.6% |       **~1.6%** |

Two corrections worth keeping, because both were mine:

- The **original 850–880M estimate was good**; the mid-run "correction" to
  1.24B was the error. It multiplied a raw `count()` of `operation_pools` by
  two, and that count carries un-merged RMT duplicates while the 2× ratio does
  not hold uniformly across eras. Measured on one partition, the tables sit at
  1.0004 rows each; globally the ratio is 1.50. `operation_pools` is a poor
  multiplier — the reference is only trustworthy for _coverage_, which is what
  the gap query uses it for.
- Bytes/row came in **below** every estimate (13.12 vs 21), so the table is
  cheaper than the ADR-0029 exception assumed.

**No `OPTIMIZE FINAL` needed.** A sampled partition has zero duplicates
(3,397,459 rows = 3,397,459 distinct keys) — background merges collapsed the
overlapping bands the successive worker layouts wrote. It would not matter if
they had not: the read path's `GROUP BY … any(amount)` is exact over
byte-identical duplicates, which is why it was written that way.

`lp_operation_amounts` also confirms **2.0006 rows per (pool, transaction)** in
the sampled partition — the two canonical legs, as designed.

The run itself is written up in [0488](../backlog/0488_OPS_backfill-must-not-starve-production.md):
it filled the box's filesystem, took ClickHouse's write space with it and put
live ingestion 10 hours behind before the real cause (a 351 GB `tmux -v` log,
not the scratch) was found.

## Acceptance criteria

- [x] 0247 path decision recorded — Path B, re-confirmed against prod 2026-07-30
- [x] `lp_operation_amounts` populated on live ingest, both legs, trades and
      deposits/withdrawals — deployed 2026-08-12, verified through the API on a
      busy pool (three of five recent rows carried two operations each, with
      rates agreeing to four decimals across independent transactions)
- [x] Amounts verified against Horizon on a **multi-pool** path payment — the
      86.7% case, not a single-pool one. Tx `24d04961…c5b5` (2026-08-17)
      crosses three pools; Horizon's `liquidity_pool_trade` effects give
      `8ca53441… sold 0.0056817 CETES / bought 0.0025000 yXLM`,
      `7fec7836… sold 1.2470222 AQUA / bought 0.0056817 CETES`,
      `59fa1dc5… sold 0.0025202 XLM / bought 1.2470222 AQUA` — and
      `lp_operation_amounts` holds `+25000 / −56817` for our pool's two legs,
      matching to the stroop with the sign convention (positive = entered
      the pool)
- [x] Backfill run — complete 2026-08-16, 211/211 partitions, zero gaps
      against `operation_pools`; overlapping re-runs collapsed by the RMT with
      no duplicates left in the sampled partition
- [ ] ~~Pool-page read seeks on `pool_id`; `read_rows` measured and
      recorded~~ — **handed to [0491](../backlog/0491_FEATURE_pool-activity-per-operation-rows-and-trades-filter.md)**,
      which already carries it as an acceptance criterion. 0491 re-keys this
      exact read to a per-operation cursor, so a number measured now describes
      a query that is about to be replaced. Measuring it there compares the
      shapes that matter — before and after the change — instead of pinning a
      baseline nobody will read
- [x] FE "Amount" column un-hidden, rendering deposit / withdraw / trade,
      one line per operation (see the 8.2% measurement above) — verified on
      production 2026-08-17 on pool `LCCC…MXS7`: `Deposit 0.1260385 XLM +
  0.0000045 YxT`, `Trade 0.0000001 YxT → 0.0027931 XLM`, and a
      three-operation transaction rendering three lines. Withdraw was **not**
      observed live — it takes the same same-sign `+` branch as deposit, only
      with negative amounts, and that branch is covered by
      `PoolTransactions.test.tsx`, but this is an inference rather than an
      observation and is recorded as such
- [x] The stale comment in `PoolTransactions.tsx` is corrected — the doc
      comment now cites task 0279 and issue #371; no `0249` reference remains
      in the file
- [x] **Docs updated** — `lp_operation_amounts` documented in
      `database-schema-overview.md` and in the canonical
      `20_get_liquidity_pools_transactions.sql`, whose leg-resolution note was
      further corrected by 0489
- [x] **API types regenerated** — `amount_a` present in both
      `openapi.json` and `generated/types.gen.ts`
