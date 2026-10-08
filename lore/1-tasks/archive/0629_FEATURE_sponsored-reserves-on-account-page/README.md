---
id: '0629'
title: 'Sponsored reserves on the account page: counts first, then what each sponsor pays for'
type: FEATURE
status: done
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
  - date: '2026-10-08'
    status: done
    who: karolkow
    note: >
      Both stages live in production-2026.10.08-2 (testnet first, 2026-10-08).
      11 PRs (#625-#627, #638, #643, #646, #647, #652 and seed/ops). Card seen
      on production; 81 accounts equal to RPC, 0 trustlines missing vs Horizon.
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
- Done: #638 merged; seed `--execute` at checkpoint 64,819,327 (2026-10-07):
  11,046,717 rows, 2,674,015 repaired (same ledger, other counters),
  8,358,413 identical, 14,289 ours newer. RPC sample 21/21 equal.
- Done: #643 (1c, API + page) merged 2026-10-07; ships with the next weekly
  release.
- Done: stage 2 — #646, #647, #652 merged 2026-10-08 (card folds by
  sponsor, signer keys unlinked with a copy button). Checked on 81 accounts
  against RPC + `stellar xdr decode` (9,082 entries, 0 differences) and on
  83 against Horizon for trustline completeness (0 missing).
- Done: both stages live in `production-2026.10.08-2` (testnet deployed
  and checked first); card seen on production for `GBEEFP…CT56`.
- In force: the count shown is the ledger's own counter, copied 1:1, never a
  row count; the list (stage 2) is committed, not optional.

## Context

Where the counters live and why stage 1 indexes them: [notes/R-where-the-counters-live.md](notes/R-where-the-counters-live.md).

## Implementation Plan

Stages 1 and 2 as planned and built: [notes/R-implementation-plan.md](notes/R-implementation-plan.md).

## Acceptance Criteria

- [x] Stage 1: for a stratified sample (top sponsors, a sponsored account, an
      account with neither, an account last changed before our floor) both
      counters equal RPC `getLedgerEntries`.
- [ ] Stage 1: the seed's `same ledger with other counters` count on the
      first pass ≈ the sponsored accounts written before 1a, and ~0 on the
      next pass. (First pass 2,674,015 of 2,657,724 sponsored; the second
      pass was not run — the live writer has carried both counters since 1a.)
- [x] Stage 1: account page shows both counts; no account shows a 0 that the
      chain contradicts.
- [x] Stage 2: on a sponsored account the listed reserves sum to the
      chain's `num_sponsored` when every sponsored entry is an account,
      trustline or signer (GBEEFP 6/6, GBIIXI 7/7, GA3WEM 3/3 on 2026-10-07).
- [x] **Docs updated** — `docs/architecture/**` schema and API pages for the
      new columns, endpoint and table; `docs/backfills.md` for the counted entry-state pass.
- [x] **API types regenerated** — 1c and stage 2 touch `crates/api/**`.

## Design Decisions

### Emerged

1. **Stage 2 read live from RPC, not indexed** — every answer sits on the
   account's own entries; ≤6 calls of 200 keys (protocol cap: 1,000
   sub-entries). A failed batch fails the whole answer, never a shorter list.
2. **Signer keys never link** — a key is not an account; three signers of
   GBEEFP had no account entry. They carry a copy button instead.
3. **Horizon is one of several checks, never a dependency** — it settled
   trustline completeness, which RPC cannot list; the sponsor-side list it
   could serve was declined (public service being wound down).

## Issues Encountered

- First release tag landed on the previous release's commit (fetched before
  the master merge was visible); a second tag on the merge hash shipped it.

## Future Work

- 0635 (backlog): `account_entry_state` keeps a closed account's last row.

## Notes

- Throwaway UI prototype (variants A/B/C) was shown outside the repo;
  stage 1 follows A, stage 2 follows B.
