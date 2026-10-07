---
id: '0497'
title: 'RESEARCH: retire repair-tier1 — move every MIN-semantics copy off RMT state tables'
type: RESEARCH
status: completed
related_adr: ['0055']
related_tasks: ['0464', '0463', '0420', '0492', '0421']
tags:
  [
    backend,
    clickhouse,
    backfill-runner,
    data-integrity,
    priority-high,
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
  - date: '2026-09-25'
    status: active
    who: karolkow
    note: >
      Activated to retire the two NFT entries, whose columns no reader uses
      since 0528. The LP entry stays until task 0468's storage fix; accounts
      and soroban_contracts stay until their routes land.
  - date: '2026-09-25'
    status: active
    who: karolkow
    note: >
      Priority raised low → high (decision 38 A): the MIN copies hold false
      values in production today — ~570k account first-seen ledgers, ~1.6k
      contract deploy ledgers, ~100k zeroed LP first deposits (0468).
  - date: '2026-10-01'
    status: active
    who: karolkow
    note: >
      Two more entries retired. soroban_contracts (decision 402 A, PR #575):
      the stored deployment is right for all 154,331 contracts and the
      rebuild's formula would have moved 1,652 to an upgrade ledger.
      lp_positions (task 0468, decision 399 B, PR #578): the column was
      dropped. repair-tier1 now rebuilds accounts.first_seen_ledger only.
      Converted to a directory (file past 150 lines).
  - date: '2026-10-06'
    status: completed
    who: karolkow
    note: >
      Archived. Four of five entries retired (both NFT copies, lp_positions,
      soroban_contracts); `repair_tier1.rs` now rebuilds only
      `accounts.first_seen_ledger`. That last entry, its candidate routes and
      the deletion of the subcommand with its mandatory step in
      `docs/backfills.md` are handed to 0421, whose storage change is the
      route that retires it.
---

# RESEARCH: retire repair-tier1

## Summary

`repair-tier1` rebuilds "first seen" columns that `ReplacingMergeTree` state
tables cannot keep: a later write replaces the whole row, so a historic minimum
is lost on live ingest as well as on parallel backfill. The goal is to move
every such value off the replaced rows, entry by entry, until the subcommand
and the mandatory step in `docs/backfills.md` can be deleted.

## Stan teraz (2026-10-01)

| Entry                                                    | State                                                                                                                                                                                                                                                                                             |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `nfts.minted_at_ledger`, `nfts_pending.minted_at_ledger` | retired — read from the ownership history, columns dropped (2026-09-25 / 2026-10-01)                                                                                                                                                                                                              |
| `lp_positions.first_deposit_ledger`                      | retired — column dropped (task 0468, decision 399 B, PR #578, 2026-10-01)                                                                                                                                                                                                                         |
| `soroban_contracts.{deployer_id, deployed_at_ledger}`    | retired — not corrupt: written once at creation, carried by every upgrade; all 154,331 contracts agree across their rows. The rebuild took the first surviving upgrade for 1,652 contracts (decision 402 A, PR #575, 2026-10-01). The "1 597 / 146 397 diverge" below compared with that formula. |
| `accounts.first_seen_ledger`                             | **handed to 0421** — the only entry left (thread 403); 0421 also deletes the subcommand and its `docs/backfills.md` step                                                                                                                                                                          |

**Accounts, the remaining entry.** About 3.5% late (estimate, 400-row sample,
task 0531). Measured 2026-10-01: one account's first appearance read from
`transaction_participants` costs 25 ms even for the busiest account (4.3 M
rows, 65 MiB, `ORDER BY ledger_sequence LIMIT 1`); a 50-account list page
costs 2.3–27 GiB (task 0531). Candidate routes, undecided:

- account page reads from history, list drops the column (patch);
- split per task 0421: `accounts` keeps identity + first/last seen as
  `min`/`max` on `AggregatingMergeTree`; `sequence_number` / `home_domain`
  move to `account_entry_state` (from scratch, no argMax needed);
- `max` over a `(ledger, value)` tuple as an argMax with plain inserts —
  untested on 26.3.

Rejected for accounts by the user on 2026-10-01: a separate first-seen table
and keeping `repair-tier1`.

Findings and progress before 2026-10-01:
[notes/R-findings-2026-09.md](notes/R-findings-2026-09.md).
