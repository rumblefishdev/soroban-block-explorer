---
id: '0421'
title: 'BUG: first_seen_ledger overwritten on every account write — account age is wrong for ~100% of accounts'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0420']
tags: ['area-indexer', 'area-clickhouse', 'data-integrity', 'effort-large', 'priority-high']
links:
  - crates/db-clickhouse/src/persist/stage.rs
history:
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
merely *transacts*, the override is `None`, so the fallback stamps the **current
ledger** as `first_seen_ledger`.

`merge_account_state_overrides` (same file, ~line 2060) does take `min()` — but
only **within one batch**. Nothing carries the already-stored value forward
across ledgers, and the writer never reads the existing row.

The engine then makes it worse rather than better:
`ENGINE = ReplacingMergeTree(last_seen_ledger)` keeps the row with the **largest
`last_seen_ledger`** — which is exactly the row carrying the latest, most
incorrect `first_seen_ledger`.

## Why the data is mostly unrecoverable in-place

| | |
| --- | --- |
| accounts with several physical copies (min() could recover) | **329,381 (2.3%)** |
| accounts already merged to one row (true value destroyed) | **14,025,268 (97.7%)** |

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

## Implementation options (pick during design)

1. **Split the table** — immutable facts (`account_id`, `first_seen_ledger`)
   written once on creation; volatile fields (`last_seen_ledger`,
   `sequence_number`, `home_domain`) in their own table. Fixes the corruption by
   construction AND narrows the 62× rewrite to a small row.
2. **AggregatingMergeTree** — `min` for `first_seen_ledger`, `max` for
   `last_seen_ledger`, so the engine itself enforces the semantics. Single
   table, but a schema + read-path change everywhere `accounts` is read.
3. **Carry-forward on write** — writer reads the stored `first_seen_ledger`
   before emitting. Rejected on cost: a lookup per account per ledger on the hot
   ingest path.

Whichever is chosen, a **historical backfill** is required to recompute the true
`first_seen_ledger` for all ~14.35M accounts from source data — the current
column cannot be repaired from itself.

## Acceptance Criteria

- [ ] Write path no longer overwrites `first_seen_ledger` for an existing account
- [ ] Engine/table shape makes the "first seen never moves forward" invariant
      structural, not convention
- [ ] Historical backfill recomputes `first_seen_ledger` for all accounts
- [ ] Regression test: an account written across several ledgers keeps its
      original `first_seen_ledger`
- [ ] Accounts list + account summary show a correct account age
- [ ] **Docs updated** — schema change ⇒ update `docs/architecture/**`
- [ ] **API types regenerated** — only if the wire shape changes; `N/A` if the
      column merely becomes correct

## Notes

- Do NOT "fix" this with a read-time `min()`: it repairs 2.3% of accounts and
  silently leaves the rest wrong (see above).
- Discovered via a contradiction while measuring something else: 7,875 accounts
  claimed a `first_seen_ledger` inside the last 21 ledgers while the deduped
  account total was growing by only tens per minute.
