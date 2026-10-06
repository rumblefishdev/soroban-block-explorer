# Task 0421 — root cause and analysis up to 2026-09

Moved from the task file when it became a directory (2026-10-01).

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
