---
id: '0505'
title: 'FEATURE: store `total_coins` / `fee_pool` from the ledger header — a free, per-ledger supply oracle'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0331', '0321', '0463', '0503', '0504']
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
  users, say which one it is. Most explorers show circulating.
- The near-miss happened because the check had to be improvised from outside
  sources. With `total_coins` stored, the question would have been a single
  query.

## Scope

- Parse and store `total_coins`, `fee_pool`, `base_reserve` and
  `bucket_list_hash` on `ledgers` (`ALTER … ADD COLUMN … DEFAULT` first, then
  the writer — the ADR 0055 deployment order).
- A continuous check comparing our summed native supply against
  `total_coins`, with the residual attributed to unindexed venues rather than
  silently ignored.
- Decide whether "supply" on the UI means circulating or total, and label it.
- Whether to backfill historical ledgers is a separate, measured decision.

## Acceptance criteria

- [ ] `total_coins` and `fee_pool` stored per ledger and verified against a
      decoded header for a sampled ledger
- [ ] The residual (`total_coins` − our sum − `fee_pool`) is computed and its
      size recorded — the first real measurement of the unindexed-venue gap
- [ ] Any user-facing supply figure states which supply it is
- [ ] **Docs updated** — schema section for `ledgers`
- [ ] **API types regenerated** — only if a DTO gains the field
