---
id: '0464'
title: 'REFACTOR: model the trustline as an entity, not as a number (and index signers)'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0463', '0321', '0331']
tags:
  [
    backend,
    clickhouse,
    xdr-parser,
    backfill,
    data-model,
    priority-low,
    effort-large,
  ]
links: []
history:
  - date: '2026-08-04'
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0463 source comparison. The root defect behind issue
      #377: our balances row IS a number, so it cannot say "this trustline
      no longer exists" — it can only say zero, which is also what an empty
      live trustline says. Deliberately NOT scheduled: the fix is gated on
      an archive re-parse, and 0463 answers the same question lazily at a
      fraction of the cost. Recorded so the decision is not rediscovered.
---

# REFACTOR: the trustline is an entity, not a number

## The defect

On Stellar there is no object called "balance". There is a **trustline ledger
entry**, and the balance is one of its fields. We inverted that: our row _is_
the amount, and the entry's existence was never recorded.

So a trustline that holds nothing and a trustline that was closed are written
identically — `amount = 0` — and no query can tell them apart. Everything
downstream inherits that: the account page hides both to avoid showing ghosts.

The parser is not the problem. It already emits `removed_trustlines`
separately from balances; the information exists and is thrown away at the
ClickHouse row shape.

## What "done" looks like

A row exists if and only if the ledger entry exists; the balance is a column
on it. Closure removes the row (or tombstones it explicitly). No consumer has
to ask the network what our own database should know.

Same move for the account: **signers and thresholds are in `AccountEntry`**,
which the meta rewrites on every account change — a sequence bump, an incoming
payment, a settings change. The parser already walks those entries and
discards the fields. That is an extraction gap, not a data gap, so this task
can close the signers half of #377 as well.

## Why this is not scheduled

**The history fill has no cheap route.** Measured in 0463:

- A one-off RPC sweep would need **~168,000 `getLedgerEntries` calls**
  (33.6 M ambiguous `(holder, asset)` pairs ÷ 200 keys) — hours of sustained
  load against public infrastructure that is not an export endpoint.
- That leaves an **S3 re-parse of 13.3 M ledgers**, which `docs/backfills.md`
  describes as a multi-machine procedure with a manual `rsync` step and a
  merge its own runbook calls easy to fumble. Nothing user-visible ships until
  it finishes.

**And the data is read sparsely.** 33.6 M ambiguous pairs exist; an account
page asks about one to three. This task precomputes millions of answers so
that thousands are read. Task 0463 computes only the ones someone looks at.

Forward-only (write the new shape from deploy, leave history ambiguous) is
**explicitly rejected** — it would leave every dormant account permanently
wrong, and half-measures on historical data are not how this project fixes
things.

## The three triggers

Do this when **any** of these becomes true — not before:

1. **Balance history for charts.** "How did this account's balance move over
   time" needs an append-only time series keyed `(holder, asset, ledger)`. That
   table _contains_ this task: every row is the state of the relationship at a
   moment, including the moment it ended. Building it makes this task free,
   and building this task first means building half of it twice.
2. **The ~7 % gap.** Measured in 0463: across 84 live sampled accounts the
   chain carries 144 live zero trustlines and we hold 134. `getLedgerEntries`
   cannot list an account's trustlines, so read-time verification can never
   surface the missing ones. Only a re-parse can.
3. **A second consumer.** Anything besides the account detail page that must
   distinguish a closed trustline from an empty one without asking the network.

## Shape when it happens

- **Parser** — emit trustline existence as its own fact; extract `signers`,
  `thresholds` and `flags` off `AccountEntry`.
- **Schema** — a `trustlines` table keyed `(holder_id, asset_id)` carrying
  balance, limit, flags and the version ledger; signers/thresholds on
  `accounts` or a side table. Note the alternatives already rejected in
  `0463/notes/S-source-options.md`: numeric sentinel (poisons
  `total_supply`), `NULL` sentinel (column rewrite, worse compression),
  ReplacingMergeTree `is_deleted` (still a column, and its cleanup depends on
  merges that task 0420 measured as not happening here).
- **Read path** — `fetch_balances` filters on existence, not on `amount != 0`;
  the account DTO serves signers from the database.
- **Backfill** — a `backfill-runner` subcommand, S3 re-parse over the full
  range, following `docs/backfills.md`.
- **Cleanup** — remove 0463's read-time enrichment once this lands; it becomes
  dead weight, including its signers half.

## Acceptance criteria

- [ ] A closed trustline is distinguishable from an empty one in SQL alone,
      with no network call
- [ ] The ~7 % of live zero trustlines we hold no row for are present after
      the backfill — verified by re-running the 0463 probe and getting zero
      "chain has more than we do" accounts
- [ ] Signers and thresholds are served from the database
- [ ] `balance_aggregates_mv` still counts holders as `countIf(amount > 0)`;
      an empty live trustline is not a holder
- [ ] 0463's runtime enrichment is deleted, not left dormant
- [ ] **Docs updated** — schema, read path, and `docs/backfills.md` gains the
      new pass
- [ ] **API types regenerated** — yes, the account DTO changes
