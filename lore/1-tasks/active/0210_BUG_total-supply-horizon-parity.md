---
id: '0210'
title: 'BUG: assets.total_supply Horizon parity — extend MVP sum to 4 sources'
type: BUG
status: active
related_adr: ['0043', '0055', '0056', '0057']
related_tasks:
  [
    '0194',
    '0197',
    '0331',
    '0339',
    '0323',
    '0499',
    '0504',
    '0515',
    '0523',
    '0540',
  ]
tags:
  [priority-high, effort-medium, layer-indexer, layer-xdr-parsing, correctness]
milestone: 2
links:
  - https://developers.stellar.org/docs/data/horizon/api-reference/aggregations/assets/list
history:
  - date: '2026-05-12'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0194 Future Work. 0194 shipped an MVP `total_supply`
      that sums only trustlines via `SUM(account_balances_current.balance)`
      per `(code, issuer_id)`. Horizon `/assets` aggregates 4 sources; the
      MVP misses 3 of them, causing known drift up to ~20-50% on DeFi
      assets (USDC w/ Soroswap + SAC). This task closes the gap and
      validates parity against an external source.
  - date: '2026-06-30'
    status: backlog
    who: stkrolikiewicz
    note: >
      Re-confirmed in a SAC/asset modeling session: the SAC contract-holder gap
      (`holder_count` + `total_supply` miss contract-side `ContractData` balances) is real
      and has a Horizon parity target (`num_accounts` + `num_contracts` / `contracts_amount`).
      Phase 3 (SAC contract holdings) owns the supply half; the `holder_count` half stays
      deferred here (out of scope) but now has a confirmed Horizon target if un-deferred —
      note the "semantics differ from trustline count" caveat applies to the ACCOUNT side;
      the CONTRACT side has a clean `num_contracts` target. 0323 Phase 2 executed →
      `soroban_contracts` is now deployed-only, so deployed-SAC identification for Phase 3 is
      cleaner (`is_sac=true, deployed>0`). Entity-model context: 0339 (SAC = facet of
      classic_credit, not a separate asset_type).
  - date: '2026-07-02'
    status: backlog
    who: karolkow
    note: >
      2 of the 4 sources SUBSUMED by task 0331 (unified `balances`): #1 trustlines
      (classic→balances migration) and #4 SAC/contract holdings (contract-held type-0/1
      re-key, ADR 0051 — incl. Soroban-DEX pool reserves, which are contract-held). The
      old `recompute_asset_aggregates` mechanism is dead (PG retired); supply is now
      `sum(balances)`. Remaining = the 2 NON-contract sources: #2 claimable balances
      (only ops parsed, no state table) + #3 native protocol LP reserves
      (`LiquidityPoolEntry`, not a contract). Rewritten scope: write synthetic `balances`
      rows for those two. Still backlog. See the dated status section in the body.
  - date: '2026-07-02'
    status: backlog
    who: claude
    note: >
      Faza-3 item folded here from the 0331 OPS close-out (2026-07-02): a per-protocol decoder
      for CUSTOM-STORAGE Soroban LP pools. ~264 type-3 LP-share tokens (Comet `CPAL` x136,
      `Pool Share Token`/Soroswap x128) render `—` for supply because their LP-share balances
      live in custom u32-keyed instance storage, NOT the standard SEP-41 `Balance(Address)`
      ContractData key the 0331 seed reads. Needs one decoder per protocol (Comet / Soroswap /
      Phoenix layouts). SCOPE FLAG: this is a SOROBAN (type-3) LP-SHARE SUPPLY gap, distinct
      from 0210's classic Horizon-parity core (#2 claimable + #3 native-LP) — parked here per
      operator; a standalone task or 0199 (LP analytics) may be a cleaner home if it muddies
      0210. The pool's HELD reserves are already captured by 0331 (contract-held); only the
      LP-SHARE token supply is missing. External check (StellarExpert live, 2026-07-02)
      confirmed the type-3 coverage is otherwise complete — no indexing gap, just this decoder.
  - date: '2026-08-18'
    status: backlog
    who: karolkow
    note: >
      Task 0505 MERGED IN and its file removed. Two things changed. (1) This
      task's verification target was Horizon, which Karol ruled legacy and
      banned from verification on 2026-08-17 — so the acceptance criterion
      "< 1% drift vs Horizon" is no longer usable. The replacement is the
      protocol's own `LedgerHeader.total_coins` / `fee_pool`, which we already
      receive in every ledger and currently discard. (2) That reframes the
      goal: supply is not validated by matching another indexer, it is
      validated by a reconciliation identity that must balance. See the
      2026-08-18 section in the body.
  - date: '2026-09-03'
    status: backlog
    who: stkrolikiewicz
    note: >
      First per-asset measurement of the two remaining sources (USDT0), which
      confirms they are the only two: LP reserves 24.5064194 + claimable
      0.3070000 account for the entire gap, exactly. Four findings that change
      the plan — the drift estimate is stale (0.001%, not 20-50%: 0331 ate it),
      LP reserve data is already exact so that half is a query rather than an
      ingestion path, the naive MV edit costs 3.7x per refresh, and the planned
      synthetic-`balances` mechanism would corrupt `holder_count`. See the
      2026-09-03 section in the body.
  - date: '2026-09-14'
    status: backlog
    who: stkrolikiewicz
    note: >
      Data audit on production: do the tables we already hold cover #2 and #3?
      #3 yes. Every classic pool changed since the floor has snapshots and a
      complete `legs`, so it is a query with no backfill; pools unchanged since
      the floor are absent because there was no floor seed. #2 no.
      `asset_transfers` holds exact claimable-balance flows but not the stock:
      15% of first-week payouts were created before the floor, and the XLM flow
      nets to −3,406.68. First run of the XLM identity: 110,054 XLM unexplained
      out of 105.4 bn. Both gaps close with one checkpoint read, not an S3
      backfill. See the 2026-09-14 section in the body.
  - date: '2026-09-15'
    status: active
    who: karolkow
    note: >
      Promoted, scoped to claimable balances first; classic LP reserves are a
      later stage. The 2026-09-14 plan item 2 (stock = seed + net `B` edges
      from `asset_transfers`) is replaced: the stock comes from
      `ClaimableBalanceEntry` changes in a dedicated table shaped like
      `balances`, joined to the snapshot reconciliation per ADR 0057.
      Measurements and the rejected options are in the 2026-09-15 section.
---

# BUG: `assets.total_supply` Horizon parity — extend MVP sum to 4 sources

## Summary

0194 shipped `assets.total_supply` as a per-ledger recompute summing **only trustlines**:

```sql
SUM(account_balances_current.balance) WHERE (code, issuer_id) = (...)
```

Horizon `/assets` aggregates the same field across **four** sources. Three are
missing from the MVP — producing systematic under-count on every classic credit
that also lives in claimable balances, LP reserves, or SAC contract storage.
Drift is up to ~20-50% on DeFi assets (USDC w/ Soroswap + SAC) per 0194 closing
notes.

This is the only ADR 0043 list-endpoint column whose **correctness** is suspect.
0197 audit verifies only non-NULL, not value parity — so this gap will not be
caught by the audit and must be its own task.

## 2026-07-02 (karolkow) — 2 of 4 sources SUBSUMED by task 0331; mechanism changed

Task **0331** (unified `balances` model, Option C) closed **2 of the 4 Horizon
sources** — including the one this task flagged as "heaviest design work":

- **#1 Trustlines — DONE.** The classic `account_balances_current` → `balances`
  migration + live single-write lands every trustline holding in `balances`.
- **#4 SAC / contract holdings — DONE.** 0331's contract-held type-0/1 re-key
  (ADR 0051) indexes every contract that holds a classic/native asset via its SAC
  as a `balances` row keyed on the wrapped asset. **This includes Soroban-DEX pool
  reserves** (Soroswap/Phoenix etc. — they hold their reserves AS a contract), which
  was the bulk of the SAC-holdings concern.

**Mechanism is different now.** This task's plan targets `recompute_asset_aggregates`
in `crates/indexer/src/handler/persist/write.rs` — that whole PG path is **dead**
(PG retired). On the new model, supply = `sum(balances)` via `balance_aggregates`, so
each source just needs its holdings written as `balances` rows (additive, no recompute).

**Remaining = the 2 NON-contract sources only:**

- **#2 Claimable balances** — we parse the _operations_ (create/claim/clawback) but
  keep **no state table** of per-asset claimable amounts (no `claimable_balances`
  table on prod). Needs a new ingestion path.
- **#3 NATIVE protocol LP reserves** — a classic Stellar AMM (`LiquidityPoolEntry`)
  is NOT a contract; its reserves live in the protocol pool entry, not a trustline or
  a `Balance(contract)` entry, so 0331 does not capture them. (`liquidity_pools` holds
  74,728 pool _definitions_ but no reserve columns; reserves are in `pool_snapshots`.)

**Rewritten scope:** write synthetic `balances` rows for claimable amounts (holder =
claimable-balance id) + native-LP reserves (holder = pool id), keyed by `assets.id`.
Then `sum(balances)` reaches full Horizon parity. Much smaller than the original 4-source
recompute. Until then, classic-asset `total_supply` undercounts by (claimable + native-LP
reserves) — the residual ~20-50% drift on heavily-pooled assets.

## 2026-08-18 (karolkow) — the oracle changes: `total_coins`, not Horizon

**Horizon is legacy and banned from verification.** The old acceptance target
("< 1% drift vs Horizon `/assets`") cannot be used. Two independent reasons,
either sufficient: Horizon is another indexer's opinion rather than the
protocol's own accounting, and it has twice misled this project on fields it
derives itself.

**The replacement is already in every ledger and we throw it away.**
`LedgerHeader` carries `total_coins` and `fee_pool` — the protocol's own count
of every stroop in existence. Our `ledgers` table stores six header fields
(`sequence`, `hash`, `closed_at`, `protocol_version`, `transaction_count`,
`base_fee`) and discards the rest, including both of these, plus `base_reserve`
(minimum-balance reasoning) and `bucket_list_hash` (checkpoint state hash —
useful to task 0502).

### The reconciliation identity — this task's real acceptance criterion

The right question is not "does our sum match an external figure" but "does the
ledger balance". For XLM:

```
total_coins  =  Σ account XLM          (indexed today)
              + Σ claimable balances   (source #2 — NOT indexed)
              + Σ native LP reserves   (source #3 — NOT indexed)
              + fee_pool               (header field — not stored)
```

Our sum should therefore **fall short of `total_coins` by exactly the
unindexed terms**. Equality today would signal a double-count, not success.

That inverts how this task proves itself. Instead of chasing a percentage
against someone else's number, the residual becomes a **measurement of the
remaining gap**, and it should shrink to `fee_pool` alone as sources #2 and #3
land. When it does, the identity closes — and that is the completion signal.

For non-native assets there is no header equivalent, so those keep an
external cross-check; use raw XDR / the checkpoint snapshot (task 0502),
never Horizon.

### The continuous reconciliation check

Not a CI test — CI has no production data, and a one-shot verification would
have caught none of this year's regressions. It must be a **monitored
invariant**: both sides already live in ClickHouse (`total_coins` per ledger
once stored, the sum in `balance_aggregates`), so the residual is a query that
can run on a schedule alongside the existing aggregate refresh.

What makes it useful is that the residual should be **stable**, not zero.
Alert on unexplained movement, not on a threshold:

- residual jumps up → we started missing value (a write path dropped rows, a
  venue grew, ingestion fell behind);
- residual jumps down or goes negative → we are counting value that is not
  there (phantom balances, a double-count, ghosts).

Concrete evidence that this is not hypothetical: ~1.3M phantom XLM from
merged-account ghosts (task 0321) sat inside the published `total_supply`
undetected, and would have moved this residual the day it appeared.

### Scope added by the merge

- Store `total_coins`, `fee_pool`, `base_reserve`, `bucket_list_hash` on
  `ledgers` (`ALTER … ADD COLUMN … DEFAULT` first, then the writer — the
  ADR 0055 deployment order).
- Establish the identity above with each term measured, not asserted —
  including where contract-held XLM (SAC, re-keyed to native by ADR 0051)
  sits within it.
- Ship the residual as a monitored invariant with alerting on movement.
- Replace the Horizon acceptance criterion with "the identity closes".

### Notes carried over from 0505

- **Circulating supply is not total supply.** Our published 105,409,692,490
  XLM looked like a 2x error against the quoted ~50B until the ~55.4B in
  `GALAXYVOID…` — SDF's 2019 burn address — was verified by decoding its raw
  `AccountEntry` via `getLedgerEntries`: the chain agrees with us to the
  stroop. **Task 0342 owns the display convention**; this task only supplies
  the number that makes the distinction measurable.
- That episode is itself the argument for storing the oracle: answering
  "why 105B and not 50B" required external sources and hand-decoded XDR, and
  would have been one query if `total_coins` were stored.
- Source #2 (claimable balances) overlaps task **0504**, which found the same
  gap from the other direction — five ledger entry types parsed and never
  stored. Whichever runs first should claim the ingestion path; the other
  consumes it.

## 2026-09-03 (stkrolikiewicz) — the gap measured on a live asset

First per-asset measurement since 0331. Target: `USDT0-GATISXX6BZ6NC7IKQBY37CJD4SOZL3CYZJWXEDG6JVIY4WBS6KXJHN6Q`
(LayerZero OFT, listed 2026-07-21). Every figure below is from production.

| bucket                             | amount                |
| ---------------------------------- | --------------------- |
| trustlines (8,253 authorized)      | 5,370.0258229         |
| contracts (14)                     | 2,589,655.2118767     |
| **our `total_supply`**             | **2,595,025.2376996** |
| claimable balances (1) — source #2 | 0.3070000             |
| native LP reserves (6 pools) — #3  | 24.5064194            |
| **full supply**                    | **2,595,050.0511190** |

The two missing sources account for the gap **exactly** — 24.8134194 USDT0,
nothing unexplained. That is the useful part: sources #1 and #4 are not merely
"done", they are provably exact, and #2 + #3 are provably the whole remainder.

### 1. The drift estimate in this task is stale

The "~20-50% on DeFi assets" figure is inherited from 0194, i.e. from **before**
0331 closed sources #1 and #4. Measured now, the residual on a live,
pool-listed, contract-heavy asset is **0.00096%**. 0331 ate essentially all of
the drift this task was opened for.

That does not close the task — a token whose float sits mostly in AMM will still
show a fraction of the truth, and the field promises "total". But the urgency is
different from what `priority-high` implies, and re-triage should be the first
step, not the last.

### 2. LP reserve data is already exact — that half is a query, not an ingestion path

The body says reserves "are in `pool_snapshots`" without saying whether they can
be trusted. They can. Reconstructed from `liquidity_pool_snapshots` +
`liquidity_pools`, USDT0 shows **24.5064194 across 6 pools** — same value to the
seventh decimal, same pool count, as the external cross-check.

Source #3 therefore needs no new parsing and no new ledger-entry handling. Only
source #2 (claimable balances) is a genuine ingestion path — no state table
exists, and it needs the full create/claim/clawback lifecycle or balances hang
in our DB forever after someone claims them. Overlaps 0504.

### 3. Do not put the pool scan inside `balance_aggregates_mv`

| table                       | rows     | on disk  |
| --------------------------- | -------- | -------- |
| `balances` (MV reads today) | 122.44 M | 1.52 GiB |
| `liquidity_pool_snapshots`  | 324.97 M | 6.86 GiB |

Current reserves require `argMax(reserve, ledger_sequence) GROUP BY pool_id` — a
full scan of a time series, because snapshots are history, not state. The MV
refreshes **every 2 minutes**, so folding this in raises the refresh from 122 M
to 447 M rows, 30× an hour, on the box 0356 already identified as the
bottleneck.

Cheaper: an **incremental** MV (insert-triggered, not refreshable) into a
current-state table, which `balance_aggregates_mv` then joins as something small.

```sql
CREATE TABLE pool_reserves_current (pool_id FixedString(32), reserve_a Decimal(38,7),
       reserve_b Decimal(38,7), ledger_sequence Int64)
ENGINE = ReplacingMergeTree(ledger_sequence) ORDER BY pool_id;

CREATE MATERIALIZED VIEW pool_reserves_mv TO pool_reserves_current AS
SELECT pool_id, reserve_a, reserve_b, ledger_sequence FROM liquidity_pool_snapshots;
```

52,733 rows out, no scan per refresh. One heavy pass over history is needed to
seed it; that one is unavoidable.

**Filter on `pool_kind = 0`** — and note that 52,733 is exactly the classic pool
count, so the figure above already assumes it. Production carries 497 registered
Soroban-AMM pools (`pool_kind = 1`, task 0374) and **zero** reserve snapshots for
them today, so the MV as written is correct by accident. The moment 0374 starts
persisting their reserves, an unfiltered version double-counts: a Soroban pool
holds its reserves AS a contract, so ADR 0051 already sums them into `balances`.
Source #3 is the NATIVE `LiquidityPoolEntry` and nothing else.

### 4. The planned synthetic-`balances` rows would corrupt `holder_count`

The 2026-07-02 plan writes synthetic `balances` rows (holder = pool id /
claimable-balance id). Those rows flow through the **same** aggregate:

```sql
toInt32(countIf(amount > 0)) AS holder_count
```

A pool is not a holder, and neither is a claimable balance. `holder_count` is
currently **correct** (91, matching the external funded-trustline count on the
day of measurement) — this change would break a right field to fix a wrong one.
Whatever synthetic holder-id space is chosen must be excluded from the count,
which is an argument for a distinguishable id range rather than an opaque hash.

### 5. Unit mismatch between the two sides

`balances.amount` is raw `Int128` (scaled by the asset's `decimals` at read);
`liquidity_pool_snapshots.reserve_{a,b}` is `Decimal(38,7)` — already scaled.
A `reserve * 1e7` bridge is right for classic assets and **wrong for
Soroban-AMM pools** carrying non-7-decimal assets (`pool_kind` ≠ 0, see 0374).
Scale by the asset's own `decimals`, not by a constant.

### Also worth fixing while here

Two comments assert that `sum(balances)` equals real supply and list the known
exceptions as "TTL-archived tail + true rebasing" — neither mentions LP reserves
or claimable balances, which are the entire measured gap:
`crates/db-clickhouse/schema/init.sql` (the `soroban_token_supply` tombstone) and
`crates/api/src/assets/queries.rs` (the `total_supply` header comment).

## 2026-09-14 (stkrolikiewicz) — data audit: do we already hold what #2 and #3 need?

> Read-only, production, tip ≈ 64,422,963. Question: can claimable balances
> (#2) and classic LP reserves (#3) be computed exactly from tables we already
> hold, after last week's schema changes (0540 `asset_transfers`, the 0374/0518
> pool tables, `legs`, 0547)?

**Answer.** #3 yes, for every pool changed since the ingest floor, with no
backfill. #2 no: the flows are complete, the stock is not, and no S3 re-parse can
supply it. Both gaps close with one history-archive checkpoint read, not a
backfill.

### Source #3 — classic LP reserves are a query over data we hold

| measure                                                        | value           |
| -------------------------------------------------------------- | --------------- |
| classic pools in `liquidity_pools`                             | 52,927          |
| with at least one snapshot / with `legs` filled                | 52,927 / 52,927 |
| `legs[1]` = pair asset A / `legs[2]` = pair asset B            | 52,927 / 52,925 |
| Soroban pools, excluded (their reserves are contract balances) | 769             |
| pools whose first snapshot is the floor ledger 50,457,424      | 77              |
| alive by newest `total_shares` / dead                          | 40,384 / 12,543 |

- `legs` is complete, so the join can go through it. PR #455 makes it the only
  option once the pair columns are dropped.
- The two `legs[2]` exceptions are not wrong legs. Their asset B has **no row in
  `assets`**: pool `C305DB2D…7252` (PIF) and pool `92415CA8…F1C6` (SUR810).
  Their reserves would attach to an id no reader resolves. That is an `assets`
  completeness defect, separate from this task.
- **There was no pool seed at the floor.** Only 77 pools first appear in the
  floor ledger itself, so a pool unchanged since the floor has no row anywhere.
  A re-parse of the ingested range cannot add it: it replays ledgers the pool
  never appears in. Only a checkpoint bucket list can.

How much it matters per asset, with share = LP reserves / (LP reserves +
`total_supply`):

| measure                                              | value                 |
| ---------------------------------------------------- | --------------------- |
| assets that are a leg of at least one classic pool   | 22,254                |
| share > 1% / > 10% / > 50%                           | 3,686 / 2,290 / 1,468 |
| `total_supply` 0 or absent while pools hold reserves | 403                   |
| XLM in classic pool reserves (11,775 pools hold XLM) | 22,541,446.6982385    |

For 1,468 assets the displayed `total_supply` is less than half of what
balances and pools hold together. The LP half alone fixes most of the visible
error, which is an input for re-triaging `priority-high`.

Cost: newest reserves for all 52,927 pools (`argMax` over 328,251,520 snapshot
rows, joined through `legs`) ran in 1.8–2.8 s at `max_threads = 8`. Inside the
2-minute refreshable MV that is still 30 runs an hour, so the current-state
table from §3 above stands.

### Source #2 — claimable balances: flows complete, stock missing

`asset_transfers` rows with a `B` endpoint since the floor (raw, before
deduplication): 411,756,582 into claimable balances and 413,059,014 out of them,
about 412 M distinct balances on each side. Claimable balances churn
constantly, so any stock has to be maintained live.

XLM, deduplicated per balance:

| lifecycle inside the window                   | balances | XLM                |
| --------------------------------------------- | -------- | ------------------ |
| created and paid out (in = out, 0 mismatches) | 388,907  | 14,158,299.4681639 |
| created, still open                           | 759      | 5,845.4485101      |
| paid out, created before the floor            | 917      | 9,252.1287091      |
| net flow into claimable balances              |          | −3,406.6801990     |

2 × 388,907 + 759 + 917 = 779,490, which is exactly the deduplicated edge
count. The edges are exact and only the starting stock is missing, which is why
a flow-only stock goes negative.

All assets, first 120,960 ledgers after the floor (about a week): 5,127,995
claimable balances paid out, of which **773,252 (15.1%)** have no creation edge.
They were created before the floor.

### The XLM identity — first run of the 2026-08-18 acceptance criterion

`totalCoins` and `feePool` come from the header of checkpoint 64,422,847 in the
SDF history archive (`core_live_001`, which lagged the tip by 116 ledgers).
`feePool` is rolled forward by Σ `fee_charged` over ledgers 64,422,848–64,422,963,
assuming `fee_charged` is net of Soroban refunds as 0540's T11 found. The
holdings were read in one statement with the tip at 64,422,963 before and
after.

| component, XLM                                        | value                                            |
| ----------------------------------------------------- | ------------------------------------------------ |
| `totalCoins`                                          | 105,443,902,087.3472865                          |
| `feePool` at 64,422,963                               | 10,599,177.9387927                               |
| Σ `balances`, native (accounts and contracts)         | 105,410,645,562.8546771                          |
| Σ classic LP reserves, native                         | 22,541,446.6982385                               |
| residual after balances and LP                        | 115,899.8555782                                  |
| minus open claimable balances created after the floor | 5,845.4485101                                    |
| **unexplained**                                       | **110,054.4070681**, 1.04 × 10⁻⁶ of `totalCoins` |

The unexplained part is claimable balances created before the floor and still
open, pools unchanged since the floor, and TTL-archived native contract
balances. For XLM, the tables we hold explain everything but about one
millionth. Credit assets have no such identity, so their claimable-balance gap
cannot be bounded from our data. `ledgers` stores neither `totalCoins` nor
`feePool`; both came from the archive.

### What this changes in the plan

1. **#3 is a query, with no backfill.** Classic pools only, joined through
   `legs`, `Decimal(7)` × 10⁷ to raw units.
2. ~~**#2 needs no new live ingestion path.** Seed the stock once from a
   checkpoint (`ClaimableBalanceEntry` from the bucket list), then add the net
   `B` edges from `asset_transfers` after that checkpoint.~~ **Superseded
   2026-09-15** — see the next section. backfill-runner's `snapshot` module
   already reads the buckets, but its classifier handles only `Account` and
   `Trustline` entries today.
3. **The same checkpoint read closes the dormant-pool gap** through
   `LiquidityPoolEntry`.
4. **Store `totalCoins` and `feePool` per ledger from now on.** The XLM identity
   then runs continuously, and after steps 2 and 3 its residual should fall to
   the TTL-archived tail.
5. **Last week's constraints hold.** Supply additions must never flow through
   `countIf(amount > 0)`, because 0547 sorts the assets list by `holder_count`.
   Soroban pools stay out: their reserves are contract balances, and
   `liquidity_pool_snapshots` is classic-only.

## 2026-09-15 (karolkow) — claimable balances first: decisions

> Read-only production measurements, tip ≈ 64,438,376. Scope of this stage:
> source #2 only. Classic LP reserves (#3) are a later stage.

### D1 — the stock comes from ledger entries, not from flows

A stock computed as seed + Σ edges inherits every edge the decoder drops or
duplicates, forever; nothing corrects it (baseline ~150 decoder rejects per
500,000 ledgers, task 0540). `ClaimableBalanceEntry` changes carry the exact
amount per row and can be corrected from a checkpoint.
`xdr_parser::ledger_value` already treats `B…` as a holder
(`crates/xdr-parser/src/ledger_value.rs`). `asset_transfers` stays as an
independent oracle.

The two sources agree today, window 64,000,000–64,100,000:

| measure                                          | count     |
| ------------------------------------------------ | --------- |
| successful `CreateClaimableBalance`              | 807,439   |
| `asset_transfers` edges into `B…`                | 807,439   |
| successful claim (521,438) + clawback (134,400)  | 655,838   |
| `asset_transfers` edges out of `B…`              | 655,838   |
| failed `CreateClaimableBalance` (create nothing) | 5,233,111 |

`operations_appearances` is folded — `sum(amount)` is the operation count,
`count()` is rows (106,979 in this window). That is the likely source of
0504's ~1.7M-per-two-months figure.

Rejected: checkpoint-only refresh (4.5 GB bucket list per read, always behind);
RPC on demand (`getLedgerEntries` needs keys, cannot list by asset).

### Is it worth doing — per-asset share, estimate

Net flow into `B…` since the floor per asset against `balance_aggregates`
(excludes CBs created before the floor, no RMT dedup — an estimate):

| open CB share of supply | assets | classic LP reserves, 2026-09-14 |
| ----------------------- | ------ | ------------------------------- |
| > 1%                    | 1,725  | 3,686                           |
| > 10%                   | 703    | 2,290                           |
| > 50%                   | 313    | 1,468                           |
| supply 0 or absent      | 332    | 403                             |

### D2 — a dedicated table shaped like `balances` (not `balances` itself)

Lifecycle, 1/64 sample of balance ids since the floor (×64 = estimate):

| measure                         | sample    | ×64    |
| ------------------------------- | --------- | ------ |
| balances since floor            | 6,456,608 | ~413 M |
| removed in the creating ledger  | 0         | 0      |
| removed within ≤ 100 ledgers    | 0.6%      |        |
| removed within ≤ 1 day / 1 week | 58% / 96% |        |
| open, created after the floor   | 14,396    | ~920 k |

Every balance is a live row plus a tombstone: ~46 M rows/year at the current
rate (estimate; the 2024 wave, ledgers 52–54 M, ran ~9× faster). Removed
balances in the window above: 655,838, against 87,271 trustline closures in
`balances` over the ~116,000 ledgers from 64,322,000 — ~8.7× the tombstone rate
per ledger. Disk is
not the constraint (~0.6–0.9 GiB/year estimate at 13.8 B/row measured on
`balances`; 372 GiB free).

Why not `balances`: every existing reader assumes a holder is an account or a
contract, and `holder_id` is a one-way hash, so a `B…` row cannot be told apart
in SQL. Concretely, `snapshot-seed` reads every native/classic row that is not a
contract (`backfill-runner/src/snapshot/seed.rs`), finds no account or
trustline for a `B…` holder, classifies it `Ghost`, and writes `amount = 0` at
the checkpoint (`snapshot/verdict.rs`) — every run would zero the open balances.
`holder_count` (`countIf(amount > 0)`, the assets-list sort key since 0547)
would count them. The `cityHash64` in ClickHouse is not the writer's
CityHash-128 low half (checked on three pools: different values), so no SQL
exclusion is possible without a new column.

The dedicated table touches no existing reader and leaves `holder_count`
unchanged. `total_supply` becomes the sum of both tables.

### D3 — the boundary rule, to be recorded as an amendment to ADR 0056

A holding lives outside `balances` only when its lifecycle requires it: mass
churn, and an id that is never reused, so all versions of a closed key can be
hard-deleted safely. Claimable balances qualify (the id hashes the creating
operation). Classic pool reserves do not (low churn; a removed pool's id comes
back when the same pair is re-created), so the LP stage puts `L…` reserve rows
in `balances`, not in this table.

### D4 — no tombstone cleanup in this stage

Nothing to clean at the start: the seed writes only balances open at the
checkpoint, so the ~413 M historical lifecycles never enter the table.
Tombstones accumulate from the writer deploy on. Recorded in the DDL comment:

- **Never TTL.** Deleting a tombstone while the live row sits in an unmerged
  part resurrects a claimed balance.
- A future cleanup is a mutation deleting every version of keys closed more
  than N ledgers ago. It deletes rows, which ADR 0057's "rows are never
  deleted" consequence does not allow today, so it needs that amendment first.
- Trigger to revisit: the table exceeds half of `balances` in rows, or the
  `balance_aggregates_mv` refresh time degrades.

### D5 — joins the snapshot reconciliation (ADR 0057 decision 5, not optional)

A state table built from ledger changes is only right if every ledger reaches
its writer. A gap — seed checkpoint older than the writer deploy, a
`backfill-runner -t` re-ingest that leaves the table out, an indexer outage —
leaves a claimed balance live forever or misses a created one. The checkpoint
comparison is what detects and corrects that, so the table joins
`snapshot-seed` (one flow, the 0523 decision) with its own row stream and
verdicts. `Ghost` → closure is correct for this table and must never reach
`balances`. The schedule itself stays with 0503.

### Work list for this stage

1. DDL for the table (`holder_id`, `asset_id`, `amount`, `last_updated_ledger`,
   `closed_at_ledger`; RMT on `last_updated_ledger`), created before any writer
   ships — the insert opens on a table's first row (`writer.rs::write_rows`),
   and claimable balances change in almost every ledger, so a missing table
   fails nearly every ledger's persist and stalls ingestion.
2. Writer from `ClaimableBalanceEntry` changes, in the shared stage so live
   ingest and `backfill-runner` write identical rows. Fold per balance id across
   the whole ledger, last in application order wins (ADR 0057 decision 6),
   with a regression test for create + claim in one transaction.
3. `ClaimableBalance` in the snapshot classifier; seed from a checkpoint at or
   after the writer's first ledger — refused in code otherwise.
4. `balance_aggregates_mv`: `total_supply` adds this table; `holder_count`
   unchanged.
5. Verification: seed dry-run comparison clean; oracle against `asset_transfers`
   between two checkpoints; `docs/architecture/**` schema and pipeline docs.

### Progress — items 1 and 2 done (2026-09-15)

- **Table** `claimable_balance_holdings` in `init.sql`, columns identical to
  `balances`. Applied to a throwaway local ClickHouse 26.3 database, and the whole
  `init.sql` through the real splitter (`apply_init_sql`). Not yet on production.
- **Parser** `xdr_parser::claimable_balance::extract_claimable_balances`, per
  transaction. A removal carries only the key, so its tombstone takes the asset
  from the `state` pre-image; a removal with no pre-image is logged and dropped.
- **Staging** `persist::claimable_balances::build_claimable_balance_rows`
  reuses `BalanceRow` (no new struct) and folds per `(holder, asset, ledger)`
  across the ledger. Writer slot drained before the `ledgers` commit marker.
  Live indexer and `backfill-runner` share the path. Not targetable with
  `--only`.
- **Real chain, not only constructed meta.** Mainnet claim tx `23273fda…c7b8`
  (ledger 64,438,024) committed as `tests/fixtures/corpus/claimable_balance_claim.b64`:
  all three removals are preceded by their `state` pre-image, and the extracted
  assets (AVLX, MAKER, MAKER on operations 1, 4, 5) match production
  `asset_transfers`, which decodes the same claims from events. The AVLX
  balance's `holder_id` equals production's `asset_transfers.from_id`
  (1,280,410,223,283,636,341) — the oracle join holds.
- **Write path end to end.** `tests/claimable_balance_holdings_e2e.rs` persists a
  create and a later claim through `persist_ledger_clickhouse` and reads one
  tombstone back with `FINAL`. Proven against its defect: with the writer's
  `end(...)` for this table removed, it fails (`left: []`).
- **Tests:** xdr-parser 438 unit + 2 real-corpus, db-clickhouse 149 unit, all
  green; `cargo clippy --workspace --all-targets -D warnings` and `cargo fmt`
  clean.
- **Correction:** the insert opens on a table's first row, not up front
  (`writer.rs::write_rows`). Same effect, stated mechanism fixed in the work list.
- **Docs:** `database-schema-overview.md` §3 + §4.17.1,
  `indexing-pipeline-overview.md` (claimable balances paragraph). API: no change.
- **Not stored, decided (2026-09-15):** claimants, predicates and sponsor of an
  open balance. Supply loses nothing, and the gap is reversible without an S3
  re-parse — the checkpoint carries whole entries. Add when a reader exists.

### Progress — item 3 written, not yet run (2026-09-15)

- `snapshot-seed` now also compares and corrects `claimable_balance_holdings`
  (`backfill-runner/src/snapshot/claimable.rs`), reusing `verdict` and
  `correction` unchanged.
- **Keyed by balance alone** on the network side: a dead bucket record carries
  only the id. The asset sits beside the live holding; a live network balance
  under a different asset is left unmatched, so ours closes and the network's is
  inserted under its real asset.
- **Writer coverage is checked from the data, not from deploy notes.** Claims
  happen in practically every ledger, so the table's first tombstone marks when
  the writer started. `--execute` refuses a checkpoint older than it, or a table
  with no tombstone; the dry-run prints the check instead.
- **Snapshot floor** `MIN_LIVE_CLAIMABLE = 100_000` — an estimate ~9× under the
  ~920k balances still open from after our floor. Re-set from the first dry-run.
- **Key agreement proven:** the snapshot's surrogate for the mainnet AVLX balance
  equals production's `asset_transfers.from_id` (1,280,410,223,283,636,341).
  Both new SQL statements run on a throwaway ClickHouse 26.3 (`min` over an empty
  set returns 0, hence the `count()`).
- Credit assets of inserted balances go through the existing stub pass. Ghosts go
  to `claimable_ghosts.tsv`. Runbook: `docs/backfills.md`.
- **Precondition:** the table must exist on production before any
  `snapshot-seed` run, balances-only runs included — the command now reads it.
- `seed.rs` is now 814 lines (was 791), just over the size limit; no inline tests
  to extract. Candidate split: the dump writers (~90 lines).

### Review of PR #460 (2026-09-16)

Standards + spec review, pre-mortem, over-engineering pass.

- **Fixed:** dump writers moved to `snapshot/dumps.rs` (`seed.rs` 727 lines);
  inline tests extracted from the touched `stage.rs`, `indexer/handler/mod.rs`
  and `persist.rs` into sibling files; one `seed::slice_sql` serves both
  tables; the asset folded into the claimable holding map (no second map kept
  in step by hand).
- **Kept:** a failed writer-coverage check refuses the whole `--execute`,
  balances included.
- **The coverage check rests on how backfill ranges are chosen.** The first
  tombstone says when the writer started, not that it never stopped, and a
  backfill writes this table for whatever range it is given. Neither matters in
  practice: a `--reindex` covers the whole Soroban era up to the tip, a gap-fill
  ends where the live writer resumed, and a seed runs only after the backfill
  has finished. A re-parse of a bounded OLD range would break it — tombstones
  below the deploy, and balances claimed after the range end left live — so that
  is the one shape to avoid. Recorded at `writer_coverage`.
- **`balances` with a holder-kind column was a real alternative** (4 readers to
  filter: `balance_aggregates_mv`, `snapshot/seed.rs`, `balance_seed.rs`,
  `bootstrap.rs`; API reads are per `holder_id`, so they never see a `B…` row).
  Kept the dedicated table: a forgotten filter there hides rows instead of
  counting a claimable balance as a holder, and deleting closed rows later does
  not touch the table the account API reads.

### Production — the table exists (2026-09-16)

`claimable_balance_holdings` created on production, verbatim from `init.sql`;
`SHOW CREATE TABLE` matches column for column (`closed_at_ledger Int64 DEFAULT
0`, `ReplacingMergeTree(last_updated_ledger)`, `ORDER BY (holder_id,
asset_id)`). Empty until the writer deploys, which is now unblocked.

### Progress — item 4 written, not yet applied to production (2026-09-16)

- `balance_aggregates_mv` sums `balances` and `claimable_balance_holdings`
  through one `UNION ALL`; `holder_count` keeps its `balances`-only meaning
  through an `is_holder` flag, so the assets-list sort key (0547) is unchanged.
  Closed rows carry `amount = 0` in both tables and move neither aggregate.
- **Order on production:** DDL, then the writer deploy, then `snapshot-seed`,
  and only then DROP + CREATE the MV. Between the deploy and the seed the table
  holds only balances created since the deploy, so summing it then publishes a
  number that is neither the old one nor the true one. A refreshable MV cannot
  be ALTERed.
- Not yet run against a real ClickHouse — no local server available at the time
  of writing.
- The two comments that called `sum(balances)` the real supply now say
  claimable balances have left the residue (`init.sql` `soroban_token_supply`
  tombstone, `api/src/assets/queries.rs`); classic LP reserves remain in it.

## 2026-09-16 (karolkow) — classic pool reserves: decisions after a second review

A devil's-advocate and an over-engineering pass over every decision above.
Both rejected the plan to write reserves into `balances` (D3).

- **Reserves come from the newest snapshot, not from rows in `balances`.** The
  snapshot is the pool entry's own state (same ledger change, one row per pool
  per ledger). Measured today: 52,974 classic pools, every one with two `legs`;
  0 dead pools with reserves; 0 conflicting newest rows. Rows in `balances`
  would be a copy of it, and a busier one than D3 assumed: 373,501 snapshot
  rows in one day of ledgers, about 747k `balances` rows a day. The seed would
  also zero them as ghosts, and no SQL filter can exclude a pool's hashed
  `holder_id` — the same three reasons claimable balances got their own table.
  D3's "low churn, into `balances`" is withdrawn; ADR 0056's amendment corrected.
- **`balance_aggregates_mv` gets a third branch:** newest `(reserve_a,
reserve_b)` per pool × 10^7, joined through `liquidity_pools.legs`, classic
  pools only, `length(legs) = 2` so one malformed row cannot fail the refresh.
  Moved below `liquidity_pool_snapshots` in `init.sql` — a refreshable MV needs
  its sources at CREATE. `EXPLAIN PLAN` of the whole query passes on production.
  The branch alone ran in 2.7 s; the current refresh takes 1.9 s (estimate:
  refresh roughly doubles).
- **Pools count as holders, classic and Soroban alike** (a Soroban pool contract
  already does). Rule: a holder holds value on its own account — accounts,
  contracts, pools. A claimable balance is value in transit and does not count.
  This changes a published number on purpose: 80,834 positive pool legs over
  22,299 assets; 9,406 assets rise by more than 10%, 1,211 at least double.
- **Backfill keeps writing `claimable_balance_holdings`** (accepted). A
  whole-era `--reindex` brings ~413 M keys (estimate) into the table the MV
  rescans every refresh. Recorded in the DDL comment.
- **Pools unchanged since the ingest floor** have no snapshot, so the MV misses
  them. `snapshot-seed` now inserts, for every live checkpoint pool newer than
  our newest snapshot of it (or without one), a snapshot and the pool row,
  versioned on the entry's `lastModifiedLedgerSeq` — insert-only, so a newer
  live row always wins and no coverage check is needed. Pools the network
  removed while ours still hold reserves are listed in `pools_gone.tsv`, not
  corrected (0 today by our data). Floors: 20,000 live pools in the snapshot,
  20,000 pools on our side.
  - **One builder.** Classic pool rows moved out of `stage.rs` into
    `persist/classic_pools.rs`; staging and the seed both call it, and the seed
    feeds it the checkpoint entry as a `state` change
    (`ledger_entry_changes::entry_as_state_change`). `stage.rs` 3,371 → 3,275.
  - **Proven on the chain:** two mainnet pool entries from `getLedgerEntries`
    (ledgers 64,454,667 and 64,454,668) build exactly the rows production's live
    writer stored for them — reserves, shares, legs, codes, fee
    (`snapshot/pools_tests.rs`). With the ledger stamp broken the test fails
    (`left: 0, right: 64454667`).
  - How many pools this adds is unknown until the first dry-run.
- **Separate PR:** pool shares into `balances` (ADR 0056, tasks 0499/0493).

## Context

### The four sources Horizon aggregates

1. **Trustlines** — `account_balances_current.balance` per `(code, issuer_id)`.
   ✅ **DONE via 0331** (migrated into unified `balances`).
2. **Claimable balances** — `claimable_balances.amount` per `(code, issuer_id)`.
   Pre-claim hot wallet liquidity. ❌ NOT done (only ops parsed; no state table).
3. **Liquidity pool reserves** — NATIVE protocol AMM (`LiquidityPoolEntry`) reserves
   per asset participant. ❌ NOT done (not a contract; reserves in `pool_snapshots`).
   _(Soroban-DEX pool reserves are a CONTRACT holding → already captured by 0331 #4.)_
4. **SAC contract holdings** — Stellar Asset Contract instance balance held
   inside Soroban contracts (SAC entries in `contract_data`).
   ✅ **DONE via 0331** (contract-held type-0/1 re-key, ADR 0051).

### Why this matters

| Asset                          | Trustlines only           | Horizon total | Drift                    |
| ------------------------------ | ------------------------- | ------------- | ------------------------ |
| Native XLM                     | n/a (excluded by Horizon) | n/a           | n/a                      |
| Plain classic credit (no DeFi) | ~accurate                 | ~accurate     | ~0%                      |
| USDC                           | partial                   | high          | ~20-50% (per 0194 notes) |
| Any AMM-listed pair            | partial                   | high          | bound by pool TVL share  |
| Any SAC-wrapped asset          | partial                   | high          | bound by Soroban TVL     |

Drift makes `/v1/assets` list-endpoint `total_supply` misleading vs every other
Stellar explorer. Block explorer SHOULD match Horizon by default.

## Scope

### In

- Extend `recompute_asset_aggregates` (`crates/indexer/src/handler/persist/write.rs`)
  to sum across **all four** sources for `total_supply`.
- Add the three missing sources one-by-one with separate sub-blocks (LP
  reserves first — schema already in place; claimable balances second; SAC
  contract holdings last — heaviest design work).
- For SAC: design + implement per-asset SAC contract holdings tracking. Likely
  a new aggregation table populated by the indexer when SAC-related
  ContractData entries appear. Open question: do we track at-rest or
  per-flow? At-rest is what Horizon does.
- Final validation: external-source parity check on a representative asset
  set — run the new recompute on a backfilled snapshot, then compare against
  Horizon `/assets?asset_code=...&asset_issuer=...` AND
  stellar.expert API for the same assets. Document drift %. Acceptance target
  < 1% drift on the sample (any larger gap means missing source or wrong
  arithmetic).

### Out

- `assets.holder_count` Horizon parity — separate task if drift surfaces;
  active-holder semantics differs from Horizon's "trustline count" anyway,
  per 0194 §1c.
- Changing the field semantics (e.g. adding a separate `circulating_supply`
  column that excludes issuer-held balance). Out of scope; would need ADR.
- Re-running 0196 enrichment-backfill drain — separate, follows once the new
  fields land.

## Implementation Plan

### Phase 1: LP reserves (smallest delta)

Schema in place (`liquidity_pools` table populated by indexer). 0194 Round 4
already prototyped this path; resurrect prototype, add to
`recompute_asset_aggregates`. UPDATE statement gets a UNION-ALL or additional
LEFT JOIN LATERAL. Cost: similar shape to the trustline sum, ~marginal
overhead.

### Phase 2: Claimable balances

`claimable_balances.amount` per `(code, issuer_id)`. Indexer already writes
this table. Add a second sum to the recompute statement.

### Phase 3: SAC contract holdings (design-heavy)

Requires per-asset SAC contract holdings tracking. Two paths:

- **3a — derive at recompute time** from `contract_data` entries (SAC
  instance balances live in `LedgerEntry::ContractData` per the SAC contract
  pattern). Cost: scans a wide table per recompute.
- **3b — maintain a dedicated aggregation table** (`asset_sac_holdings(code,
issuer_id, balance)`) updated by the indexer when SAC ContractData entries
  change. Cost: extra write path, but recompute reads a small narrow table.

Pick 3b if indexer can identify SAC entries cheaply via xdr-parser; pick 3a
otherwise. Decide via a spike before committing schema.

### Phase 4: External-source parity validation (acceptance gate)

After all three sources land, run a one-shot parity check:

1. Pick a sample of ~20 assets covering: plain classic credit, USDC, AMM-only
   asset, SAC-wrapped asset, mixed (all four sources).
2. For each, query:
   - `GET /v1/assets/{code}-{issuer}` on staging (post-backfill)
   - Horizon `/assets?asset_code=...&asset_issuer=...`
   - stellar.expert `/explorer/public/asset/{code}-{issuer}` API
3. Diff `total_supply` across all three. Expected: < 1% drift between our
   value and Horizon. stellar.expert is a tiebreaker.
4. Document results in a snapshot under `docs/audits/2026-MM-DD-
total-supply-parity.md`. Each row a real (code, issuer, ours, horizon,
   stellar.expert, drift%) entry.
5. Any drift > 1% on a non-edge-case asset = bug, fix before merge.

## Acceptance Criteria

- [~] Supply sums all 4 sources. **Trustlines + SAC/contract holdings DONE via 0331**
  (`sum(balances)` over the unified model — the dead `recompute_asset_aggregates`
  is superseded); **claimable + native-LP reserves remain**.
- [x] SAC contract holdings path — DONE via **0331 + ADR 0051** (contract-held type-0/1
      re-key; state-based, no separate aggregation table needed).
- [ ] Per-ledger overhead measured. Target: < +10% over post-0194 baseline.
      0194 measured +4% baseline; new ceiling +14%.
- [ ] External-source parity snapshot committed to `docs/audits/`. Sample
      ≥ 20 assets, drift < 1% on ≥ 95% of them, every outlier explained
      (issuer-held excluded? SAC entry not yet tracked?).
- [ ] Docs updated: `docs/architecture/database-schema/database-schema-overview.md`
      §4.10 Assets (total_supply now 4-source aggregate);
      `docs/architecture/xdr-parsing/` if new SAC parsing lands;
      `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`
      §5.2 step 14 if recompute shape changes substantially.
- [ ] ADR 0043 cross-checked. Allocation (list-endpoint + on-chain → indexer)
      unchanged — no amendment needed unless SAC path forces a new column.

## Future Work

- **Continuous parity monitor** — periodic CI job that re-runs Phase 4 sample
  against Horizon and alerts on drift > 5%. Defer; one-shot validation is
  enough for v1.
- **`circulating_supply`** column — issuer-held balance excluded. Out of
  scope; would need product decision + ADR.

## Notes

- **Phase 3 mechanism = the 0331 ContractData-balance ingestion (UPDATED
  2026-06-29).** ⚠️ The earlier "event-fold over `soroban_events`" idea is
  REFUTED — measured on prod (`stellar` RPC): the fold under-counts 3/3 tokens
  with a getter (vault / rebasing / non-SEP-41-event tokens change balances with
  no foldable event; 54% of type-3 events are non-SEP-41). 0331 pivoted to reading
  **ledger STATE**: `ContractData` `Balance(Address)` entries → `soroban_token_balances`
  (the parser already decodes these — `xdr-parser::extract_soroban_token_balances`).
  Phase 3 (SAC contract holdings) is the **same mechanism on type-2 SAC contracts**:
  same `Vec[Symbol("Balance"), Address]` key, same table/framework — the only delta
  is the value shape (SAC stores a `BalanceValue` **struct**: amount + authorized +
  clawback flags, vs the bespoke-token bare `i128`), so Phase 3 adds a struct decoder

  - resolves SAC `contract_id → (code, issuer)`. The "non-standard storage-key
    layouts" line that scoped out 0138 is **disproven** (the standard `Balance(Address)`
    key was confirmed readable on a real vault token). **0331 lands the ingestion
    framework first; Phase 3 is a small extension on top, not a separate path.**
    0331 still owns type-3 (bespoke Soroban) supply+holders, out of this task's scope.

  * **Double-count trap:** the same classic asset is held two ways — as G-address
    trustline holdings (source #1) AND as Soroban `Balance` entries held by
    C-contracts via the SAC (source #4). Total supply is the ADDITIVE union:
    `trustline holdings + C-contract Balance holdings`. A trustline holder and a
    contract holder are distinct entries, so each is counted once — it's a sum of
    the two sources, NOT a subtraction of one from the other. Matches Horizon parity.

- **0194 deliberately deferred this.** From 0194 closing history (2026-05-XX):
  "Full Horizon-parity total_supply (LP reserves + claimable_balances + SAC
  contract holdings) explicitly deferred to Future Work." This task is the
  promised follow-up.
- **0197 doesn't catch this.** The audit checks non-NULL on sample queries, not
  value parity. Spawned independently per the 0197 punch list.
- **Sequencing.** Phase 1+2 can ship together (LP reserves + claimable
  balances) as a smaller PR. Phase 3 needs its own PR with the spike + design
  decision. Phase 4 parity check runs on top of Phase 3.

### Deep review of PR #460 (2026-09-16)

Four sequential lenses (correctness, production data, devil's advocate, pattern
generalization) and a separate judge, all read-only against production.
Verdict: merge after one runbook fix. No defect in the write path, the view or
the seed on today's data (0 newest-snapshot ties, 0 malformed `legs`, 0 dropped
pools; seeded pool rows equal live rows).

- **Fixed in this PR:**
  - `docs/backfills.md` states the rule that a re-parse never ends before the
    claimable balance writer deploy. Measured why: re-parsing 64,000,000–64,009,999
    after the fact would leave 8,718 claimed balances live across 28 assets, and
    the seed's coverage check cannot see it. No code guard: our re-parses are
    whole-era or gap-fills.
  - The view's CI test now covers a claimable balance (supply, not a holder), a
    classic pool with two snapshots (newest wins, × 10^7, both legs) and a
    Soroban pool (ignored).
  - A ClickHouse test of the query behind the seed's writer-coverage check.
  - The view reads `liquidity_pools FINAL` instead of `argMax(legs)`.
  - `pools_gone.tsv` lists only classic pools that were ours before the
    checkpoint.
  - Comment: every `ALTER` on `balances` also runs on
    `claimable_balance_holdings` (one row struct feeds both).
  - View swap notes in `init.sql`: create it as the replaced view's user, force
    and check the first refresh, roll the view back with the writer.
  - Docs that still called supply `sum(balances)`, the API field docs
    (regenerated types), table counts.
- **Kept, decided:** a pool counts as a holder at any positive amount, dust
  included, like every other holder. XLM gains 10,290 pool holders, 5,907 of
  them under 1 XLM. A dust rule, if ever, applies to every holder kind at once.
- **Measured cost:** refresh 1.3 s → 3.65 s, 115 M → 444 M rows read, 643 MiB;
  about 5–6 s in a year (estimate). The seed's pool read 1.19 s.
- **Follow-ups, not in this PR:** a seeded pool shows its last-modified ledger
  as its creation ledger in the pool API; a pre-deploy check of every insert
  struct against production columns; the `--execute` refusal branch itself
  has no test (it needs a decoded checkpoint).
- **Rollout checks for the view swap:** `SHOW CREATE TABLE balance_aggregates_mv`
  names the user to create it as (`dev_shared` today). After the swap, XLM
  holders should read about 9.97 M (9,960,150 before). Rollback:
  `CREATE MATERIALIZED VIEW balance_aggregates_mv REFRESH EVERY 2 MINUTE TO
balance_aggregates AS SELECT asset_id, sum(amount) AS total_supply,
toInt32(countIf(amount > 0)) AS holder_count FROM balances FINAL GROUP BY
asset_id` after dropping the new one; seeded rows need no removal.
