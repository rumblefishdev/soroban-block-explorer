---
id: '0464'
title: 'FEATURE: balance history over time — append-only holdings time series'
type: FEATURE
status: backlog
related_adr: ['0055']
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
      Spawned from the 0463 source comparison as "model the trustline as an
      entity, not as a number". Deliberately not scheduled: the fix was
      believed to be gated on an archive re-parse.
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Rewritten. Two things changed. The entity-table shape was rejected in
      ADR 0055 — the entity is the holding relationship, whose table already
      exists, so the lifecycle became a column on `balances` instead. And the
      cost that justified deferring this task was refuted: the backward fill
      is neither 168k RPC calls nor a 13.3M-ledger re-parse, and the re-parse
      could never have worked (78.85% of history predates our ledger floor).
      What survives is this task's own first trigger — the balance-history
      time series — which is what it now describes.
---

# FEATURE: balance history over time

## Summary

`balances` is a ReplacingMergeTree keyed `(holder_id, asset_id)`. By design it
keeps only the newest version of each holding, so **it cannot answer "how did
this balance move over time"**. Charting a holding's history, or reconstructing
state at a past ledger, needs an append-only time series keyed
`(holder_id, asset_id, ledger)`.

## Why this is not the 0463 work

Task 0463 and [ADR 0055](../../2-adrs/0055_holding-lifecycle-column-on-balances.md)
answer a different question — _does this holding exist right now_ — with a
`closed_at_ledger` column on `balances`. That is deliberately a bridge: cheap,
metadata-only, and enough for the account detail page.

This task is the long-term shape that **subsumes** it. Every row in a history
series is the state of a relationship at a moment, including the moment it
ended, so current state becomes `argMax` over ledger and closure becomes the
last row of a series. When this lands, `closed_at_ledger` becomes derivable
and the bridge can be retired.

Building this first would have meant building half of it twice — which is
exactly why 0463 took the column.

## The three triggers

Do this when **any** of these becomes true:

1. **Charts.** "How did this account's balance move over time" is asked for by
   a user or a spec.
2. **Point-in-time queries.** Anything needing state as of a past ledger —
   analytics, reconciliation, dispute resolution.
3. **The bridge starts hurting.** If `closed_at_ledger` accumulates special
   cases, or a second consumer needs history to answer correctly.

## Shape when it happens

- **Table** — append-only `MergeTree` (not Replacing) keyed
  `(holder_id, asset_id, ledger)`, one row per observed change, carrying
  amount and lifecycle state. Partitioning by ledger range is likely; measure
  before choosing.
- **Parser** — already emits every change needed; no new extraction.
- **Backfill** — the checkpoint bucket snapshot gives the starting state
  (4.54 GB gzipped / 21 files, measured in 0463); history before our ledger
  floor is **not reachable** — 78.85 % of chain history predates it — so the
  series starts at the floor with a seeded opening balance. Record that limit
  explicitly rather than implying completeness.
- **Retirement** — once current state derives from the series,
  `closed_at_ledger` and possibly `balances` itself become redundant. Plan the
  read repointing as part of this task, not after it.

## Constraints inherited from 0463's research

- Rows must always be written complete — RMT replaces whole rows, and a
  partial write silently drops fields.
- Version on the entry's own `lastModifiedLedgerSeq`, never on a window
  boundary (see task 0492 for what the other pattern cost).
- `balance_aggregates_mv` is refreshable and recomputes from `balances`; if
  this task repoints it, the public `total_supply` / `holder_count` numbers are
  in the blast radius.

## Acceptance criteria

- [ ] A holding's amount at any ledger since our floor is answerable in SQL
- [ ] Closure appears as the terminal row of a series, not as a missing row
- [ ] The pre-floor limit is documented, not implied away
- [ ] Current-state reads are repointed, and 0463's bridge column is retired
      rather than left dormant
- [ ] `balance_aggregates_mv` still counts holders as `countIf(amount > 0)`
- [ ] **Docs updated** — schema, read path, and `docs/backfills.md`
- [ ] **API types regenerated** if any DTO gains history fields
