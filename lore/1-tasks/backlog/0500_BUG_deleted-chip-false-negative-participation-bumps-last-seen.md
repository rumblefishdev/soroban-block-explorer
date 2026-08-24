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

## Scale MEASURED 2026-08-24 (the pre-fix measurement this task required)

Window 55,000,000–55,200,000 (200k ledgers), successful `account_merge`
sources joined against deduplicated `accounts`:

| population                                         | count       | share |
| -------------------------------------------------- | ----------- | ----- |
| merged sources in window                           | **112,080** |       |
| `last_seen_ledger` bumped past the merge ledger    | 20,382      | 18.2% |
| …of which the NEWEST row is a participant skeleton | **16,187**  | 14.4% |
| (`sequence_number = 0` — not a recreation)         |             |       |

The 16,187 are the false-negative floor for this window: dead accounts the
chip renders as alive. The remaining ~4,195 bumped rows have a real
`sequence_number` at their newest version — recreation candidates, correctly
alive. **5 of 5 sampled skeleton-bumped accounts verified ABSENT on chain**
via `getLedgerEntries` (raw XDR, per the no-Horizon rule), including
`GA274C3GJHBJIG7BC7SF6F5HFMHIPDEFVAY7IO7ZTTNGXHSQ4653DQ4C` — merged at
55,126,142, bumped to 60,541,934 (~10 months past its death) and still
rendering as an ordinary live account.

Extrapolation is not linear across the chain (merge density varies), but at
~14% of merged sources per window the affected population is in the hundreds
of thousands. The decided fix (native `closed_at_ledger != 0` after the 0463
seed) covers every one of these by construction: the seed zeroes the ghost
and stamps the closure regardless of what later participation did to
`last_seen_ledger`. Re-measure zero AFTER the seed, per the AC.

Also confirmed while measuring: a NEW-column-on-accounts alternative stays
rejected — `accounts` takes whole-row skeleton writes (task 0421, 55.6% of
active rows have `sequence_number = 0`), so a `deleted` column there would be
clobbered by exactly the participation writes that cause this bug.

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
