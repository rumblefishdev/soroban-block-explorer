---
id: '0421'
title: 'BUG: accounts row is rewritten with defaults on every touch — first_seen_ledger, sequence_number and home_domain all clobbered'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0420', '0232', '0425']
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

## Root cause (located)

`crates/db-clickhouse/src/persist/stage.rs:636`

```rust
let first_seen_ledger = ov
    .and_then(|o| o.first_seen_ledger)
    .unwrap_or(last_seen_ledger);   // ← current ledger
```

`ov.first_seen_ledger` is only populated when the account is **created** in that
ledger (it comes from the XDR account-state extraction). For an account that
merely _transacts_, the override is `None`, so the fallback stamps the **current
ledger** as `first_seen_ledger`.

`merge_account_state_overrides` (same file, ~line 2060) does take `min()` — but
only **within one batch**. Nothing carries the already-stored value forward
across ledgers, and the writer never reads the existing row.

The engine then makes it worse rather than better:
`ENGINE = ReplacingMergeTree(last_seen_ledger)` keeps the row with the **largest
`last_seen_ledger`** — which is exactly the row carrying the latest, most
incorrect `first_seen_ledger`.

## Why the data is mostly unrecoverable in-place

|                                                             |                        |
| ----------------------------------------------------------- | ---------------------- |
| accounts with several physical copies (min() could recover) | **329,381 (2.3%)**     |
| accounts already merged to one row (true value destroyed)   | **14,025,268 (97.7%)** |

A read-time `min(first_seen_ledger)` would therefore "fix" 2.3% of accounts and
leave 97.7% wrong — while making the two groups inconsistent with each other.
That is why 0420 deliberately did NOT patch this at read time.

## Related: write amplification on the same path

The same writer rewrites the whole account row on every touch:

```
distinct accounts actually active in a day:   135,919
rows inserted into `accounts` that day:     8,461,622   → ~62× amplification
```

This is inherent to holding a per-activity field (`last_seen_ledger`,
`sequence_number`) in the same row as immutable facts (`account_id`,
`first_seen_ledger`). Merges absorb it (steady state ~4.3% un-merged), so it is
a cost rather than an outage — but it is also the mechanism that destroys
`first_seen_ledger`, so a fix should consider both together.

## Why a sentinel / NULL cannot work

The obvious idea — "write NULL (or 0) for `first_seen_ledger` when we are not
creating the account, so we don't clobber it" — **does not work on a
ReplacingMergeTree**. RMT does not merge columns; it picks one **whole winning
row** per key (highest version) and discards the rest. A NULL in the winning row
therefore _erases_ the value rather than preserving it. There is no partial
update to reach for.

## Recommended fix: per-column merge semantics (AggregatingMergeTree)

Change the engine so the invariant is enforced by the **table**, not by writer
discipline:

```sql
ENGINE = AggregatingMergeTree ORDER BY account_id

first_seen_ledger  SimpleAggregateFunction(min, Int64)   -- can never move forward
last_seen_ledger   SimpleAggregateFunction(max, Int64)   -- can never move back
sequence_number    SimpleAggregateFunction(max, Int64)
```

Why this is the right shape here:

- **Nothing is ever overwritten.** AggregatingMergeTree merges column by column
  with the named function, so every insert contributes and `min` wins. The NULL
  problem disappears because no row has to "carry" the whole truth.
- **The writer stays plain.** `SimpleAggregateFunction` (unlike
  `AggregateFunction`) accepts ordinary values — the indexer keeps inserting a
  bare `Int64`, no state encoding. The insert shape in `stage.rs` is unchanged.
- **The current bug becomes harmless.** `min(50457424, 63000000) = 50457424`, so
  even the existing `unwrap_or(last_seen_ledger)` fallback can no longer damage
  the value. Correctness stops depending on remembering not to break it. (The
  line should still be simplified for clarity — it is just no longer load-bearing.)
- **In-house precedent:** `asset_sac` already uses exactly this
  (`SimpleAggregateFunction(max, Int64)` on `AggregatingMergeTree`).

### Open design points (do not skip these)

1. **`home_domain` does not fit.** It needs "latest wins" = `argMax`, which
   `SimpleAggregateFunction` does **not** support (only `min`/`max`/`sum`/`any`/
   `anyLast`/…). Options: `anyLast` (non-deterministic across merges — a stale
   domain can win), `AggregateFunction(argMax, …)` (forces state-encoded inserts,
   invasive), or move the field to its own small table. Decide explicitly.
2. **`sequence_number` via `max`** is correct only because Stellar account
   sequence numbers are monotonically increasing. Confirm before relying on it.
3. **Reads.** `FINAL` behaves the same, and `accounts_recent`
   (`SELECT … FROM accounts FINAL`) keeps working unchanged. A read without
   `FINAL` must `GROUP BY account_id` with `min()`/`max()` — the same dedup
   discipline 0420 established, so no new class of risk.

### Alternative kept on the table

**Split the table** — immutable facts (`account_id`, `first_seen_ledger`)
written once on creation; volatile fields in their own table. Also fixes it by
construction and additionally narrows the 62× rewrite to a small row, at the
cost of a join on every account read and a fallback for accounts whose creation
event was never captured.

**Rejected: carry-forward on write** — the writer reads the stored
`first_seen_ledger` before emitting. A lookup per account per ledger on the hot
ingest path; too expensive.

Whichever is chosen, a **historical backfill** is required to recompute the true
`first_seen_ledger` for all ~14.35M accounts from source data — the engine change
does not resurrect values already destroyed, and the column cannot be repaired
from itself. Migration is a new table + `INSERT SELECT` + `EXCHANGE TABLES`.

## Acceptance Criteria

- [ ] Write path no longer overwrites `first_seen_ledger` for an existing account
- [ ] Engine/table shape makes the "first seen never moves forward" invariant
      structural, not convention
- [ ] Historical backfill recomputes `first_seen_ledger` for all accounts
- [ ] Regression test: an account written across several ledgers keeps its
      original `first_seen_ledger`
- [ ] Accounts list + account summary show a correct account age
- [ ] **`backfill-runner bootstrap` is deleted too.** It exists to top up accounts
      left at `sequence_number = 0`, i.e. to mop up exactly what this bug creates —
      61.7% of recent transaction senders. Once the writer stops emitting defaults,
      nothing produces skeletons and the subcommand has no reason to run. Delete it
      with its `docs/backfills.md` row and its `crates/backfill-runner/README.md`
      entry, in the same PR. Note it is also invoked as a step of `run`; that call
      site goes with it. Per lore 0425 clause 4.
- [ ] **`repair-tier1`'s `accounts` rebuild is deleted, not left as a safety net.**
      That subcommand exists only because the engine cannot express "minimum"; once
      the engine does, keeping it around re-creates the mop this task removes. If
      the other four Tier-1 tables still need it, delete only `rebuild_accounts` and
      say so in `docs/backfills.md`; if 0232 lands first and covers all six columns,
      the whole subcommand goes. Per lore 0425 clause 4 — a pass whose live hole is
      closed must be removed in the same PR that closes it.
- [ ] **Docs updated** — schema change ⇒ update `docs/architecture/**`
- [ ] **API types regenerated** — only if the wire shape changes; `N/A` if the
      column merely becomes correct

## Notes

- Do NOT "fix" this with a read-time `min()`: it repairs 2.3% of accounts and
  silently leaves the rest wrong (see above).
- Discovered via a contradiction while measuring something else: 7,875 accounts
  claimed a `first_seen_ledger` inside the last 21 ledgers while the deduped
  account total was growing by only tens per minute.

## Analysis 2026-08-21 — the shape of the fix, and half of it already exists

Re-measured on production while reviewing the 0463 release (fresh numbers,
same phenomenon this task was opened for):

| population                             | with `sequence_number = 0` |
| -------------------------------------- | -------------------------- |
| all accounts (14,577,283)              | 1,390,359 — 9.5 %          |
| **accounts active recently (451,930)** | **251,186 — 55.6 %**       |

An account that sends a transaction always bumps its sequence on chain, so
every one of those 251k zeroes is ours. The 61.71 % this task recorded in July
and the 55.6 % measured now are the same tap, still running.

### Four options weighed

| #     | option                                                                      | keeps coverage                                                                                                 | stops the clobber | cost                                                                                                                       |
| ----- | --------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ----------------- | -------------------------------------------------------------------------------------------------------------------------- |
| A     | stop writing skeleton rows — only write when an `AccountEntry` was observed | **no** — an account that merely RECEIVED a payment disappears from the list and from search until it transacts | yes               | small                                                                                                                      |
| B     | write the skeleton with the LOWEST version                                  | yes                                                                                                            | yes               | `last_seen_ledger` is both the RMT version and a meaningful column; it cannot be stamped 0 without lying about "last seen" |
| **C** | **split the table by write condition**                                      | **yes**                                                                                                        | **yes**           | migration + historical backfill                                                                                            |
| D     | `AggregatingMergeTree` with per-column `max()` / `argMax()`                 | yes                                                                                                            | yes               | rebuild of the table and every reader; heaviest                                                                            |

**C is the recommended shape**, and 0463 has already built half of it:

- `accounts` — identity plus `first_seen_ledger` / `last_seen_ledger`. Written
  on every touch. Nothing here can be falsified by a touch.
- a side table written ONLY when an `AccountEntry` was in the change set —
  today `account_entry_state` (signers, thresholds, master weight, flags), which
  would gain `sequence_number` and `home_domain`.

`account_entry_state` already carries the exact write condition this fix needs
(`ExtractedAccountState.signers.is_some()` — trustline-only appearances never
touch it) and the exact source (`AccountEntry`). It is not a coincidence: both
tables exist because a whole-row RMT write cannot carry fields the writer did
not observe.

D remains worth a look if per-column merge semantics would also retire
`repair-tier1`'s MIN problem (the 12 Tier-1 columns) in one move — that is the
one thing C does not solve. Weigh them together, not separately.

**Owner's read (2026-08-21): C is the likely answer.** D solves the same
problem through a heavier mechanism — a table rebuild plus every reader
learning aggregate-merge semantics — where C is a write-condition split that
the codebase already demonstrates working. Treat D as the fallback to reach
for only if the Tier-1 MIN columns turn out to need it anyway; do not open
with it.

**The counter-argument C must answer before it is chosen:** under D this side
table can disappear ENTIRELY — `sequence_number`, `home_domain`, signers and
flags all fold back into `accounts` with per-column merge semantics, and the
split stops being necessary. Anything invested in the side table's shape
(including its name) is therefore provisional until this task picks C or D.
The rename done in 0463 was priced with that in mind: minutes, on an empty
table, against a coordinated code-and-data change if deferred past the
indexer deploy.

### The naming smell, and why it was resolved before this task starts

`account_entry_state` holds `flags`, which describe the account as an ASSET ISSUER
(`AUTH_REQUIRED` / `AUTH_REVOCABLE` / `AUTH_IMMUTABLE` /
`AUTH_CLAWBACK_ENABLED`) and have nothing to do with signing. The column is in
the right place — same source, same write condition, same version — but the
name undersells the table, and under option C it would be plainly wrong once
`sequence_number` and `home_domain` move in.

Renaming was FREE at review time (table present on production but EMPTY, zero
consumers in `crates/api` or `web`, writer not yet deployed: 0 of 76,334,267
`balances` rows carried a closure), and it stops being free the moment the
indexer ships. Decision and outcome are recorded in task 0463.

### Other smells found in the same pass (all pre-existing, none blocking 0463)

- **`accounts` has no `flags` column at all** — the parser has always extracted
  `AccountEntry.flags` (`ledger_entry_changes.rs`, long before 0463) and thrown
  them away. Whatever this task does with `sequence_number` should decide
  deliberately whether `flags` belongs to the same row.
- **The invariant is worth stating in the schema, not just here**: a whole-row
  write that defaults missing fields is safe only if it also carries the LOWEST
  version. `soroban_contracts`' stub writer is safe by accident of which column
  is the version; `accounts` is unsafe by the same accident inverted.
- **No reverse index on signers** — "which accounts is `G…` a signer of?" is
  unanswerable today. Not in 0463's scope, worth its own task if the UI wants
  it.
- **Version-column audit came back clean**: of 27 ReplacingMergeTree tables, 15
  carry no version column, and every one of those is either keyed by ledger (a
  re-parse only ever competes with its own earlier parse) or is a pure function
  of an immutable input. No table is missing a version where it needs one.
