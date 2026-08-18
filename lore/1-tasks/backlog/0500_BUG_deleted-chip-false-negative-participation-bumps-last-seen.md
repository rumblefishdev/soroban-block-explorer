---
id: '0500'
title: 'BUG: merged accounts read as alive — participant skeletons bump last_seen past death'
type: BUG
status: backlog
related_adr: ['0055']
related_tasks: ['0324', '0295', '0321', '0463', '0492']
tags:
  [
    backend,
    api,
    accounts,
    data-correctness,
    clickhouse,
    priority-medium,
    effort-small,
  ]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Found during the pre-archive verification of 0321, on Karol's explicit
      instruction to verify the deleted chip live before archiving. Five of
      five sampled merged accounts failed the check. The tombstones themselves
      (0321's scope) verified correct; this is a separate defect in the
      deleted DERIVATION, not in the data 0321 backfilled.
---

# BUG: dead accounts render as alive

## Symptom — measured, 5 of 5

Five accounts merged around ledger 51.0M: Horizon returns 404 (dead), our
native tombstone is correct (`amount = 0` at the merge ledger), and the
`fetch_deleted_status` derivation returns **false** — the page shows them as
ordinary live accounts. Example:
`GCCNV3G4RULCQMQN3NVT2IW3MRIMBY5QCRKUYLBUH6W7GIQBNZEZACTJ`.

## Root cause — a third writer nobody accounted for

`fetch_deleted_status` (task 0324, `accounts/queries.rs:311-317`) assumes:
_"`last_seen_ledger` is the GREATEST of every appearance, so a deleting merge
necessarily sits in that ledger."_ That conflates **entry changes** with
**address appearances**.

The indexer writes **skeleton `accounts` rows for every transaction
participant** (so name joins resolve; `sequence_number = 0` is their
signature). A failed payment _to_ a dead address is a participation without
any entry change — and it bumps the dead account's `last_seen_ledger` past its
death. Verified on the example: exactly one row version,
`sequence_number = 0`, `last_seen = 52037224`, one `transaction_participants`
hit at that very ledger, and the account's last real operation is its merge
(type 8) at 51049419 — a year earlier.

The derivation then looks for a merge at the synthetic ledger, finds nothing,
and reports alive. Any dead address that anyone ever references afterwards is
permanently mis-rendered.

Scale unmeasured; the sampling that found it was not designed to measure it.
Measure before fixing: dead accounts (type-8 sources, successful, never
recreated) whose `last_seen_ledger` exceeds their merge ledger.

## The fundamental fix is already decided — do not patch the old derivation

ADR 0055 gives `balances` a lifecycle column, and the 0463 writer already
stamps the `account_merge` native tombstone with `closed_at_ledger` going
forward; the checkpoint seed stamps the historical ones. After the seed:

**`deleted` = the native balance row's `closed_at_ledger != 0`.**

One column read, no joins, no per-ledger partition prune, immune to
participation noise, and it removes the `ponytail:` same-ledger
merge-recreate caveat 0324 documented. The old two-table EXISTS derivation is
deleted, not repaired.

## Acceptance criteria

- [ ] The five sampled accounts render as deleted; Horizon agrees on each
- [ ] A recycled account (merged, then recreated — ~47 % of one measured
      window's merge sources) still renders as alive
- [ ] Scale measured and recorded before the fix, re-measured zero after
- [ ] `fetch_deleted_status`'s old derivation is removed, not left dormant
- [ ] Sequenced AFTER the 0463 seed has run and verified — the column is only
      trustworthy then
- [ ] **Docs updated** — read-path section describing the deleted derivation
- [ ] **API types** — N/A unless the DTO shape changes
