---
id: '0421'
title: 'BUG: accounts row is rewritten with defaults on every touch — first_seen_ledger, sequence_number and home_domain all clobbered'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0420', '0232', '0425', '0497']
tags:
  [
    'area-indexer',
    'area-clickhouse',
    'data-integrity',
    'effort-large',
    'priority-high',
  ]
links:
  - crates/db-clickhouse/src/persist/stage.rs
history:
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Widened from one column to three — the same defect, found while auditing the
      backfill subcommands (0425). `stage.rs:699`, three lines below the
      `first_seen_ledger` line this task was opened for, does the same thing to
      `sequence_number`: no account-state override → write `0`. `home_domain` has the
      identical shape (`ov.and_then(...)` → NULL). All three ride the same whole-row
      write, and the RMT version (`last_seen_ledger`) is bumped by exactly the writes
      that lack the data, so the emptied row wins.
      Measured on prod: of 137,655 accounts that were a transaction **source** in
      ledgers 63,400,000–63,500,000, **84,944 (61.71%) carry `sequence_number = 0`**.
      An account that sends a transaction always bumps its sequence on chain, so the
      zero is entirely ours. Skeletons are also twice as common among recently-active
      accounts (14.7%) as among dormant ones (6.75%) — the gap is being produced now,
      not inherited.
      Consequence for tooling: `backfill-runner bootstrap` is not the one-off its
      docstring claims, it is a mop under this tap, and the live indexer has no
      bootstrap of any kind (zero references to RPC snapshotting in
      `crates/indexer/`). 0425's README table has been corrected accordingly.
      The invariant this exposes, worth stating in the fix: **a whole-row write that
      defaults missing fields is safe only if it also carries the lowest version.**
      `soroban_contracts`' stub writer (`stage.rs:1761`) emits an all-NULL row but
      stamps `wasm_uploaded_at_ledger = 0`, so it always loses — safe by accident of
      which column is the version. `accounts` bumps its version on the same write
      that empties the row, so it always wins.
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Found while auditing RMT read paths for 0420. The "first seen" ledger the
      UI shows is not the first time an account was seen - the indexer resets it
      to the current ledger on every account write, and the ReplacingMergeTree
      version column then preserves the most-wrong value. Root cause located to
      an exact line; fix needs a write-path change plus a historical backfill,
      so it is deliberately NOT bundled into 0420 (a read-path task).
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Independent confirmation, with a named witness — reached from a
      different task and without reading this one first, which makes it a real
      second observation rather than a re-reading.** Surfaced while measuring
      task 0214's acceptance criteria.
      Witness: account
      `GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55`, measured on
      prod 2026-07-22.
      - **903,373,913** rows in `transaction_participants` — one of the most
        active accounts on the network, not an edge case
      - **7 unmerged rows** in `accounts`; six carry
        `first_seen_ledger == last_seen_ledger` (a single-ledger observation),
        one carries the true span `50,457,424 → 63,503,839`
      - the ReplacingMergeTree winner (version = `last_seen_ledger`) reports
        **`first_seen_ledger = 63,600,904`** against a true minimum of
        **`50,457,424`** — off by **13.1 million ledgers**, and the reported
        value tracks the chain tip, so it is wrong by more every ledger
      **`first_seen_ledger` is one of the 12 Tier-1 MIN-semantics columns** and
      `repair_tier1.rs:130` already repairs it from `MIN(tp.ledger_sequence)`.
      So the mop exists — but this witness shows the corruption present *now*,
      on a live account, which means the mop is either overdue or the live path
      re-corrupts faster than it can be run. Deciding which is the first question
      for whoever takes this: if it is continuous, `repair-tier1` is a treadmill
      and the write-path fix is the only real remedy.
      **`sequence_number` is clobbered by the identical mechanism, and this is
      now proven against raw XDR — it also answers this task's open design
      point #2.** The skeleton write on a participant appearance carries
      `sequence_number = 0` stamped with `last_seen_ledger = current`, so any
      participant appearance _later_ than the account's last source-tx
      outversions the real sequence with 0.
      Traced end to end 2026-07-23, decoded with the official `stellar` CLI:
      - Witness `GBGGLXUIL75PFOPOAW2MB6MXQNERO3Z7G7X36SGG5JUQCW4FV6T6MRZG`
        **sourced** a successful tx at ledger 63,606,453; its `resultMetaXdr`
        (fetched from Soroban RPC) carries the sequence bump right where our
        parser reads it — `tx_changes_before`: state `273187545255247872` →
        updated `273187545255247873`.
      - The **same account appears as a participant at 63,606,455**, two ledgers
        later. Its only surviving `accounts` row is that skeleton — `seq = 0`,
        `last_seen = 63,606,455` — which wins the RMT version. Current state
        reads 0.
      - Control: a different account whose _last_ touch was its own source-tx
        shows the correct sequence as current state. **The parser is fine; the
        write path clobbers.** This is not an extraction gap.
      **Design point #2 (above) is answered: Stellar sequences are monotonic**
      (the witness bumps by exactly 1;
      [CAP-0001](https://github.com/stellar/stellar-protocol/blob/master/core/cap-0001.md)
      seeds the top 32 bits at creation and only increments after), so
      `SimpleAggregateFunction(max, Int64)` is the correct merge and yields the
      true current sequence. `home_domain` is clobbered the same way and is
      design point #1.
      **Corrected magnitude — read this before quoting a number.** A first pass
      measured "934k / 8.6%" of sourcing accounts at `seq = 0`. That was wrong:
      it counted `WHERE sequence_number = 0` over **un-deduped RMT rows**, so it
      matched the skeleton row of accounts whose _current_ (RMT-winner) state is
      actually correct — the exact `rmt-unmerged-dedup-on-read` trap this repo
      already documents. Deduped with `argMax(sequence_number, last_seen_ledger)`:
      **447,427 distinct successful-tx sources (10% of 4,482,355) currently read
      `seq = 0`** — every one provably wrong, since sourcing a tx requires a real
      sequence. Still large, but half the inflated figure.
      Also same defect class, fix together: `soroban_contracts.is_sac`, a
      non-nullable `Bool` asserting `false` on stub rows that `asset_sac`
      contradicts (see 0435).
  - date: '2026-10-01'
    status: backlog
    who: karolkow
    note: >
      Deferred here (thread 403): accounts.first_seen_ledger is now the last
      repair-tier1 entry. Rejected: a separate first-seen table, and
      repair-tier1 as the lasting answer. Measured: one account's first
      appearance from transaction_participants costs 25 ms; a list page
      2.3-27 GiB. Tested on CH 26.3: SimpleAggregateFunction(max, Tuple(ledger,
      value...)) behaves as argMax with plain inserts. Converted to a
      directory (file past 150 lines).
---

# BUG: `first_seen_ledger` overwritten on every account write

## Summary

`accounts.first_seen_ledger` does not mean "the ledger where this account was
first seen". The indexer rewrites it to the **current** ledger every time an
account is touched, and the `ReplacingMergeTree(last_seen_ledger)` version
column then keeps the newest row — i.e. the **most wrong** value. Measured
errors on real accounts: **2,833,232 / 2,650,380 / 740,815 ledgers** too late.

The value is on the wire (`AccountListItem`, `AccountDetail`) and rendered as a
"First seen" column on the accounts list and in the account summary, so users
see a wrong account age today.

## Stan teraz (2026-10-01)

- `accounts.first_seen_ledger` is the **last entry `repair-tier1` rebuilds**
  (the NFT, LP and contract entries are retired — task 0497). About 3.5% of
  accounts show a first appearance that is too late (estimate, 400-row sample,
  task 0531), on the account page and the account list.
- **Owns the end of `repair-tier1`** (handed over when 0497 was archived,
  2026-10-06): once `first_seen_ledger` survives later writes, this task
  deletes the `repair-tier1` subcommand (`crates/backfill-runner/src/repair_tier1.rs`)
  and the mandatory step in `docs/backfills.md`.
- **Rejected by the user (thread 403):** a separate first-seen table, and
  keeping `repair-tier1` as the lasting answer.
- **Measured:** one account's first appearance from `transaction_participants`
  costs 25 ms even for the busiest account (4.3 M rows, 65 MiB,
  `ORDER BY ledger_sequence LIMIT 1`); a 50-account list page costs 2.3–27 GiB
  (task 0531) — read-time derivation fits the account page only.
- **Tested on a local ClickHouse 26.3:** the tuple `max` acts as argMax —
  [notes/R-tuple-max-test-2026-10.md](notes/R-tuple-max-test-2026-10.md).
- **Routes still open:** the split recommended below (identity + first/last
  seen as `min`/`max` on `AggregatingMergeTree`, volatile fields to
  `account_entry_state` — no argMax needed), or one `AggregatingMergeTree`
  table with the tuple `max` for the latest-wins fields.

Root cause, options and analysis up to 2026-09:
[notes/R-analysis-to-2026-09.md](notes/R-analysis-to-2026-09.md).
