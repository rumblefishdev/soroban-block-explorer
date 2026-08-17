---
id: '0505'
title: 'FEATURE: store `total_coins` / `fee_pool` from the ledger header — a free, per-ledger supply oracle'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0210', '0342', '0331', '0321', '0463', '0503', '0504']
tags:
  [
    backend,
    xdr-parser,
    clickhouse,
    data-integrity,
    priority-medium,
    effort-small,
  ]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Found while hunting for capability gaps. We publish an XLM total_supply
      computed by summing our own balances table and have never had anything
      to check it against — while the protocol hands us the authoritative
      number in every single ledger header, for free, and we drop it.
---

# FEATURE: the ledger header already carries the answer

## What we drop

`LedgerHeader` carries `total_coins` and `fee_pool` — the protocol's own
authoritative accounting of every stroop in existence. Our `ledgers` table
stores six fields (`sequence`, `hash`, `closed_at`, `protocol_version`,
`transaction_count`, `base_fee`) and discards the rest, including both of
these. It also drops `base_reserve` (needed to reason about minimum balances)
and `bucket_list_hash` (the checkpoint state hash — relevant to task 0502).

## Why this is worth a task

**1. Our most public number has never been verifiable.**
`balance_aggregates.total_supply` for XLM is a sum over our `balances` table.
It is published, and until now nothing could contradict it. Today's
~1.3M phantom XLM from merged-account ghosts (task 0321) sat in that figure
undetected precisely because there was no oracle.

**2. The difference quantifies our blind spot, exactly.**
XLM legitimately lives in venues we do not index — claimable balances,
liquidity-pool reserves, the fee pool. Task 0331 flagged "which venues do we
not count" as an open question and it has stayed open. With `total_coins`
stored, the answer is one subtraction per ledger:
`total_coins − (our sum) − fee_pool ≈ value in venues we do not index`.
That turns an open question into a monitored number.

**3. It is nearly free.** The header is already parsed for the six fields we
keep; this adds columns and a few assignments. No new source, no backfill
machinery — though a backfill over existing ledgers is a separate decision
(the value is mostly forward-looking, since the check is continuous).

## A cautionary note worth keeping

While investigating, our published figure (105,409,692,490 XLM) looked like a
2× error against the widely quoted ~50 billion supply. It is **not** an
error: the ~55.4 billion difference sits in `GALAXYVOID…`, the address holding
SDF's 2019 burn — coins removed from circulation but never destroyed, so they
remain in `total_coins`. Verified by decoding the account's raw `AccountEntry`
via `getLedgerEntries`: the chain agrees with our stored balance to the
stroop.

Two lessons, both worth encoding in whatever this task builds:

- **circulating supply ≠ total supply.** If a "supply" figure is ever shown to
  users, say which one it is. Most explorers show circulating. (Task 0342
  owns this decision and had already identified the same burn-void account on
  2026-07-02 — the "discovery" here was a rediscovery, which is itself an
  argument for the oracle: a stored number would have made the question
  trivial instead of sending someone hunting through external sources.)
- The near-miss happened because the check had to be improvised from outside
  sources. With `total_coins` stored, the question would have been a single
  query.

## Relationship to the two tasks that already exist

Checked before filing — this does NOT duplicate either, but neither can be
read without it:

- **Task 0210** (`total_supply` parity) wants our supply validated against an
  external source, and that source is **Horizon** — now legacy and banned from
  verification. 0210 therefore has a target it may no longer use.
  **`total_coins` is the replacement oracle**, and a better one: it is the
  protocol's own accounting rather than another indexer's opinion, it needs no
  network call, and it arrives with every ledger.
- **Task 0342** (supply display convention) already owns the burn-void
  question — it was filed 2026-07-02 after the 0331 run surfaced exactly the
  `GALAXYVOID…` balance, verified as real on-chain data. **The
  circulating-versus-total labelling decision belongs to 0342, not here.**
  This task only supplies the number that makes the distinction measurable.

## The reconciliation identity — the real deliverable

The question "should our sum converge to `total_coins`?" has a sharper answer
than yes or no. XLM exists in venues we do and do not index, so the identity
is:

```
total_coins  =  Σ account XLM (indexed)
              + Σ claimable balances (NOT indexed — task 0504)
              + Σ liquidity-pool reserves (NOT indexed — LiquidityPoolEntry)
              + fee_pool (header field, not indexed)
```

So our sum should **not** equal `total_coins` — it should fall short by
exactly the unindexed terms. That is what makes the oracle useful: the residual
is not noise to explain away, it is the **measured size of our blind spot**,
and it should shrink to `fee_pool` alone as 0504's entry types get indexed.

Establishing this identity — confirming each term, and where contract-held XLM
(SAC, re-keyed to the native id by ADR 0051) sits within it — is the substance
of this task. Do not assume the terms; verify each against a decoded header
and the chain.

## Scope

- Parse and store `total_coins`, `fee_pool`, `base_reserve` and
  `bucket_list_hash` on `ledgers` (`ALTER … ADD COLUMN … DEFAULT` first, then
  the writer — the ADR 0055 deployment order).
- Establish and document the reconciliation identity above, with each term
  measured rather than asserted.
- A continuous check of the residual, so a regression (phantom XLM, a
  double-count, a dropped venue) surfaces as a moving number instead of
  sitting undetected.
- Hand the circulating-versus-total labelling to **0342**; hand the
  external-parity goal of **0210** its new oracle.
- Whether to backfill historical ledgers is a separate, measured decision.

## Acceptance criteria

- [ ] `total_coins` and `fee_pool` stored per ledger and verified against a
      decoded header for a sampled ledger
- [ ] The residual (`total_coins` − our sum − `fee_pool`) is computed and its
      size recorded — the first real measurement of the unindexed-venue gap
- [ ] Any user-facing supply figure states which supply it is
- [ ] **Docs updated** — schema section for `ledgers`
- [ ] **API types regenerated** — only if a DTO gains the field
