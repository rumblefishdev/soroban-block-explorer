---
id: '0497'
title: 'RESEARCH: retire repair-tier1 — move every MIN-semantics copy off RMT state tables'
type: RESEARCH
status: backlog
related_adr: ['0055']
related_tasks: ['0464', '0463', '0420', '0492']
tags:
  [
    backend,
    clickhouse,
    backfill-runner,
    data-integrity,
    priority-low,
    effort-medium,
  ]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from the LP-holdings decision session. The direction is decided
      there: repair-tier1 is a compensating process for MIN-semantics columns
      copied onto ReplacingMergeTree state tables, and it should die as a
      class — one entry at a time, as each copy moves to a fact-derived or
      history-derived read. The LP entry already dies with that session's
      design. This task is the per-column investigation for the rest.
---

# RESEARCH: retire repair-tier1

## The verdict already taken

`repair-tier1` exists because ReplacingMergeTree keeps the **newest** row per
key while MIN-semantics columns need the **earliest** value — so every
parallel or `--reindex` backfill silently corrupts them, and a mandatory
repair pass (indexer stopped) recomputes them from append-only fact tables
(`repair_tier1.rs:18-45`, `docs/backfills.md`).

That is a correctly built compensator for a modelling compromise. The
decision, taken in the LP-holdings map session: **the compromise goes, not
just the symptom.** Each MIN copy moved off an RMT state table kills its
repair entry; when the last entry dies, the subcommand and the runbook step
die with it. New rule recorded in the merge ADR: no new MIN-semantics copies
on RMT state tables, ever.

## Inventory to investigate, per column

| Column                                                 | Fact source (already used by the repair)                                              | Candidate route                                                                                                                                     |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lp_positions.first_deposit_ledger`                    | `operations_appearances`, type 22                                                     | **dies via the LP merge** — derived in the pool-side companion MV at refresh; fallback sparse column. Not this task's work; listed for completeness |
| `accounts.first_seen_ledger`                           | `MIN(ledger_sequence)` over `transaction_participants` (3.6 B rows)                   | the hard one — rendered on the account page, 24 M accounts; measure a companion-MV refresh vs read-time derive vs keep-until-0464                   |
| `nfts.minted_at_ledger`                                | `nft_ownership`, event_type 0                                                         | companion or read-time; measure                                                                                                                     |
| `nfts_pending.minted_at_ledger`                        | `nft_ownership_pending`, event_type 0                                                 | same, or dies if `nfts_pending` goes vestigial (task 0309 direction)                                                                                |
| `soroban_contracts.deployer_id` + `deployed_at_ledger` | no dedicated fact table — repair reads the raw pre-FINAL table, documented as fragile | worst case: may need its own small fact table, or 0464-era treatment; investigate first                                                             |

## Questions to answer

- Per column: derivation cost at read vs at MV refresh vs staying until the
  balance-history table (0464) absorbs it. Measure, do not estimate —
  `accounts.first_seen_ledger` over 3.6 B `transaction_participants` rows is
  the one that can sink a route.
- Which consumers actually render each column (check `web/src/`, not a guess —
  the LP session nearly declared a live column dead by grepping a directory
  that does not exist).
- Whether `soroban_contracts`' deployer info needs a proper fact table first —
  the repair's own docs call the current rebuild fragile.
- Sequencing with 0464: anything 0464 absorbs for free should not get its own
  machinery here.

## Done means

A per-column route recorded (companion / read-time / wait-for-0464 / keep,
with the measured reason), implementation subtasks filed for the routes that
win, and an explicit statement of what remains in `repair-tier1` and until
when. The end state — the subcommand deleted, `docs/backfills.md` losing the
mandatory step — is the success criterion even if it lands incrementally.
