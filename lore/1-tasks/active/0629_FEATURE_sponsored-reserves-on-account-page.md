---
id: '0629'
title: 'Sponsored reserves on the account page: counts first, then what each sponsor pays for'
type: FEATURE
status: active
related_adr: ['0055']
related_tasks: ['0463', '0521']
tags:
  [backend, clickhouse, api, frontend, accounts, priority-medium, effort-large]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/454'
history:
  - date: '2026-10-06'
    status: backlog
    who: karolkow
    note: >
      Spawned from issue #454. Split into small PRs, the certain part first:
      the two reserve counters the ledger already stores on every AccountEntry.
  - date: '2026-10-06'
    status: active
    who: karolkow
    note: 'Started stage 1, PR 1a.'
---

# Sponsored reserves on the account page

## Summary

Issue #454 asks the account page to show how many reserves an account sponsors
for others, and which entries those are (an expandable list, as on
stellar.expert). A reserve is the 0.5 XLM the network locks per ledger entry
(2 for the account itself, 1 per trustline, offer, signer or data entry, 1 per
claimant of a claimable balance); CAP-33 lets another account pay it. We index
nothing about sponsorship today. This task adds it in two stages: the counts,
which the ledger keeps on the account itself, then the per-entry list, which
needs a new table.

## Stan teraz

- Done: scope measured; decided 2026-10-06 — the literal ask (counts, then
  the list), counts stored by the indexer, not read live from RPC.
- Done: #625, #626, #627 live in `production-2026.10.07-1` (testnet since
  2026-10-07); ALTERs on `default` and `testnet`. Mainnet wallet
  `GAUA7…PNJU` 4,051,315 / 0 equals RPC.
- Next: #638 (1b, the refill) in review; then build backfill-runner on the
  box and run the seed with `--refill-entry-state-older-than 64816029`.
- In force: the count shown is the ledger's own counter, copied 1:1, never a
  row count; the list (stage 2) is committed, not optional.

## Context

**Where the numbers live.** `AccountEntry.ext.v1.ext.v2` carries
`num_sponsoring` (reserves this account pays for others) and `num_sponsored`
(reserves of this account paid by others). They change only together with the
`AccountEntry`, which the live writer already turns into an
`account_entry_state` row (`persist/stage.rs`, whole-row per observed entry),
and which the checkpoint seed already covers for accounts older than our
ledger floor (`backfill-runner/src/snapshot/entry_state.rs`, task 0521). Today
both counters are read past and dropped.

**Why the counter and not a count of rows.** One sponsored account costs 2
reserves, a claimable balance one per claimant; the counter is what the
network enforces and what stellar.expert shows. A list of sponsored entries
(stage 2) would not sum to it row by row.

**Who is sponsoring, measured 2026-10-06.** Last 7 days on production:
69,266 `BEGIN_SPONSORING_FUTURE_RESERVES` operations from 100 source accounts,
56 `REVOKE_SPONSORSHIP` from 4. Read live via RPC `getLedgerEntries`:
`GAUA7XL5…PNJU` sponsors 4,043,490 reserves, `GDB3RSSW…6CU` 111,104 — so a
list must be paginated and keyed by sponsor.

**Trap for the refill.** The seed writes an `account_entry_state` row only when
the network's entry is newer than our newest row (task 0521). After the
columns are added, almost every account already has a row at the same
version, so the seed would skip them and the counters would stay 0. The
refill rewrites only accounts whose newest row predates 1a going live — not
every account, which would re-insert ~11M identical rows and blind the 0521
"cannot move" signal. Rows carry the entry's own `lastModifiedLedgerSeq`, so a
live write that lands meanwhile still wins.

## Implementation Plan

Each PR is one production step. Stage 2 starts only after stage 1 ships.

### Stage 1 — counts (certain: the data and both writers exist)

| PR  | Scope                                                                                                                                         | Production acts after merge                                 |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| 1a  | Parser reads `num_sponsoring` / `num_sponsored`; `account_entry_state` gains two columns (`DEFAULT 0`); live writer and seed fill them — #626 | `ALTER TABLE … ADD COLUMN` ×2, then indexer deploy          |
| 1b  | Seed: a one-off mode that rewrites accounts whose newest row predates 1a                                                                      | `snapshot-seed --execute` in that mode; check counts vs RPC |
| 1c  | API `account` detail exposes both counts; the page shows them (two rows in Summary, prototype variant A)                                      | API + SPA deploy                                            |

1c waits for the refill run (#638) to report `refilled` ≈ the accounts whose
newest row predates 64,816,029, and for a stratified RPC sample (top
sponsors, sponsored accounts, an account with neither, one last changed
before our floor) to match. A refilled row keeps its old version by design,
so "no row older than 1a" can never be the gate. Shipped earlier, the page
would show 0 where the chain says otherwise — a wrong number, not a missing
one. An
account with no `account_entry_state` row shows "unknown", as Signers does.

**Rejected: reading the counters live via RPC on each page load** — cheaper
(no ALTER, no refill), but every account page would then depend on an external
RPC at request time, a second source of truth beside the index, and stage 2
needs the stored data anyway.

### Stage 2 — what a sponsor pays for (needs its own design)

New table keyed by sponsor: one row per sponsored ledger entry (account,
trustline, offer, data, claimable balance, signer) with its owner and reserve
count, filled from `LedgerEntry.ext.v1.sponsoring_id` and
`AccountEntry` `signer_sponsoring_ids`, versioned on the entry's ledger,
closed when the entry or its sponsorship goes. Seeded from the checkpoint
snapshot, then live. Paginated endpoint and the expandable list (prototype
variant B). The reverse direction ("sponsored by", variant C) reads the same
table by owner — not asked for in #454; decide when stage 2 is designed.

## Acceptance Criteria

- [ ] Stage 1: for a stratified sample (top sponsors, a sponsored account, an
      account with neither, an account last changed before our floor) both
      counters equal RPC `getLedgerEntries`.
- [ ] Stage 1: the seed refill reports how many rows it rewrote; a second
      normal seed pass writes ~0 entry-state rows (the 0521 signal still works).
- [ ] Stage 1: account page shows both counts; no account shows a 0 that the
      chain contradicts.
- [ ] Stage 2: list for `GAUA7XL5…PNJU` paginates, and its reserves per kind
      sum to the counter.
- [ ] **Docs updated** — `docs/architecture/**` schema and API pages for the
      new columns, endpoint and table; `docs/backfills.md` for the refill mode.
- [ ] **API types regenerated** — 1c and stage 2 touch `crates/api/**`.

## Notes

- Throwaway UI prototype (variants A/B/C) was shown outside the repo;
  stage 1 follows A, stage 2 follows B.
