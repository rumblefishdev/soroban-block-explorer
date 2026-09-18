---
id: '0565'
title: 'BUG: snapshot-seed zeroes contract-held balances whose holder has no soroban_contracts row'
type: BUG
status: backlog
related_adr: ['0051', '0057']
related_tasks: ['0210', '0463', '0503']
tags:
  [
    bug,
    backfill-runner,
    snapshot-seed,
    balances,
    data-integrity,
    priority-high,
    effort-medium,
  ]
links: []
history:
  - date: '2026-09-18'
    status: backlog
    who: karolkow
    note: >
      Found in the 0210 seed run at checkpoint 64,486,655: all 36 rows the run
      called GHOST and zeroed are contract-held SAC holdings, not stale
      account rows. One decoded end to end against the chain — the entry is
      still live. The run before it (64,131,263) zeroed the same class.
---

# BUG: the seed zeroes contract-held balances it cannot recognise as contract-held

## Summary

`snapshot-seed` decides whether a `balances` row is contract-held with one
clause — `holder_id NOT IN (SELECT id FROM soroban_contracts)`
(`backfill-runner/src/snapshot/balances.rs`, `BALANCES_FILTER`). A holder that
is a contract address we never registered passes the filter, gets compared
against the checkpoint's ACCOUNT and TRUSTLINE population, matches nothing,
and is classified `Ghost` — so `--execute` writes `amount = 0` over a holding
the network still carries.

The checkpoint models trustlines, accounts, claimable balances and pools. A
SAC balance is a `ContractData` entry, which the comparison deliberately does
not model, so the network side can never produce a match for these rows. The
verdict is therefore wrong by construction, not by data.

## What it cost, run of 2026-09-18 (checkpoint 64,486,655)

36 rows over 34 holders, zeroed:

| asset   | raw amount     | rows |
| ------- | -------------- | ---- |
| USDC    | 29,573,588,379 | 11   |
| USDM0   | 1,919,900,000  | 7    |
| USDM1   | 1,476,877,359  | 14   |
| PHANTOM | 10,000,000,000 | 1    |
| EURC    | 60,000,000     | 1    |
| native  | 54,267,314     | 2    |

The rows are listed in the run's `ghosts.tsv` (a complete list, not a sample —
the bucket is far below the dump cap), which is what a repair can be built
from.

## Evidence

1. **Every one of the 34 holders is a contract address.** In `asset_transfers`
   they appear only as `to_kind = 'C'` (118 transfers), never as a sender, and
   none has an `accounts` row.
2. **One decoded end to end.** Holder surrogate `5952633038669555834`, USDC,
   10,000 raw, ledger 64,452,815. That ledger's transaction (fetched from RPC,
   decoded with the official CLI) CREATES a `ContractData` entry under SAC
   `CCW67TSZ…` with key `Balance(CAHFBZW6A5INUEPCASR7LFVBEDD5XSXH4OHP4PXRJDLJUPFBZ44WBR2E)`
   and value `amount = 10000, authorized = true` — our row's amount and ledger
   to the unit.
3. **The holding is still live.** `getLedgerEntries` at ledger 64,489,207
   returns that entry unchanged (`lastModifiedLedgerSeq` 64,452,815). The
   holder's own contract-instance key comes back ABSENT: the address was never
   deployed, which is exactly why `soroban_contracts` has no row for it and
   why the filter cannot see it. Nothing is wrong with the contracts table.
4. **Not a one-off.** The previous seed (checkpoint 64,131,263) closed rows of
   the same holders on the same grounds.
5. **Population at risk:** 1,183 `balances` holders are neither in `accounts`
   nor in `soroban_contracts`. Each run zeroes whichever of them held a
   positive classic/native amount at its checkpoint.

## Scope

1. **Stop mis-classifying.** A holder the comparison cannot place — no
   `accounts` row, no `soroban_contracts` row — is NOT comparable and must be
   quarantined and reported, never zeroed. A genuine merged-account ghost
   (task 0321) is distinguishable: it HAS an `accounts` row.
2. **Decide how the holder kind is carried.** The filter reads a dimension
   table to answer a question about the row itself; the same weakness produced
   the `B…` discussion in 0210 (D2). A `holder_kind` column on `balances`
   would answer it locally and let the MV, the seed and `balance_seed` share
   one rule. Needs an ADR amendment (0056/0057) before any column lands.
3. **Repair the rows already zeroed.** The RMT version of the zero is the
   checkpoint ledger, so re-inserting the true amount at its own (older)
   ledger loses the merge. A mutation over the exact (holder, asset) pairs
   from `ghosts.tsv`, restoring `amount` and clearing `closed_at_ledger`, is
   the same shape as the 0210 pool-reserve repair.
4. **Re-run the comparison** after the fix and confirm the bucket is empty on
   a fresh checkpoint.

## Acceptance criteria

- [ ] An unplaceable holder is quarantined, counted and dumped — never zeroed
- [ ] A merged-account ghost is still zeroed (task 0321 behaviour unchanged),
      proven by a test over both shapes
- [ ] The 36 rows of 2026-09-18 restored, with the artifact kept
- [ ] A dry-run on a later checkpoint reports 0 unplaceable holders zeroed
- [ ] `docs/backfills.md` states what the seed does with a holder it cannot
      place
