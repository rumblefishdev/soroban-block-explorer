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
- Done: #638 merged; seed `--execute` at checkpoint 64,819,327 (2026-10-07):
  11,046,717 rows, 2,674,015 repaired (same ledger, other counters),
  8,358,413 identical, 14,289 ours newer. RPC sample 21/21 equal.
- Done: #643 (1c, API + page) merged 2026-10-07; ships with the next weekly
  release.
- Done: stage 2 — #646, #647, #652 merged 2026-10-08 (card folds by
  sponsor, signer keys unlinked with a copy button). Checked on 81 accounts
  against RPC + `stellar xdr decode` (9,082 entries, 0 differences) and on
  83 against Horizon for trustline completeness (0 missing).
- Next: after the weekly release — verify both stages on sorobanscan, reply
  on #454 via `/issues`, close this task.
- In force: the count shown is the ledger's own counter, copied 1:1, never a
  row count; the list (stage 2) is committed, not optional.

## Context

Where the counters live and why stage 1 indexes them: [notes/R-where-the-counters-live.md](notes/R-where-the-counters-live.md).

## Implementation Plan

Each PR is one production step. Stage 2 starts only after stage 1 ships.

### Stage 1 — counts (certain: the data and both writers exist)

| PR  | Scope                                                                                                                                         | Production acts after merge                        |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| 1a  | Parser reads `num_sponsoring` / `num_sponsored`; `account_entry_state` gains two columns (`DEFAULT 0`); live writer and seed fill them — #626 | `ALTER TABLE … ADD COLUMN` ×2, then indexer deploy |
| 1b  | Seed writes `account_entry_state` for every live account (drops task 0521's narrowing) — #638                                                 | `snapshot-seed --execute`; check counts vs RPC     |
| 1c  | API `account` detail exposes both counts; the page shows them (two rows in Summary, prototype variant A)                                      | API + SPA deploy                                   |

1c waits for the seed run after #638 (its `same ledger with other counters`
count is the repair) and for a stratified RPC sample (top
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

### Stage 2 — who pays each sponsored entry (decided 2026-10-07)

Shaped like stellar.expert, which lists sponsors on the **sponsored**
account ("Account base reserve sponsored by X", "USDC trustline sponsored by
X") and shows only a count on the sponsor's side. Read **live from RPC**, not
indexed: `getLedgerEntries` returns each entry's `sponsoring_id` in `extXdr`,
the account's `signer_sponsoring_ids` cover signers, and our `balances` name
the trustlines to ask for. PR 2a (#646) shares the RPC pool of the WASM
fetcher; PR 2b (#647) adds `GET /v1/accounts/{id}/sponsorship` and a
"Sponsored reserves" card grouped by sponsor. **Not delivered:** a list on
the sponsor's side ("whom GAUA7… pays for") — it needs an index over the
whole network (4M entries for one wallet), stellar.expert does not offer
it, and nobody asked again; dropped, not deferred.

Re-checked 2026-10-08: Horizon answers it (`accounts?sponsor=X`, 200
accounts a page in ~1.9 s, sponsor per trustline and signer, none per data
entry, no total). Declined: it would make the page depend on a public
service SDF is winding down (no new features, RPC preferred), behind a rate
limit our Lambdas share through AWS egress. If asked for, build our own
index instead.

## Acceptance Criteria

- [ ] Stage 1: for a stratified sample (top sponsors, a sponsored account, an
      account with neither, an account last changed before our floor) both
      counters equal RPC `getLedgerEntries`.
- [ ] Stage 1: the seed's `same ledger with other counters` count on the
      first pass ≈ the sponsored accounts written before 1a, and ~0 on the
      next pass.
- [ ] Stage 1: account page shows both counts; no account shows a 0 that the
      chain contradicts.
- [ ] Stage 2: on a sponsored account the listed reserves sum to the
      chain's `num_sponsored` when every sponsored entry is an account,
      trustline or signer (GBEEFP 6/6, GBIIXI 7/7, GA3WEM 3/3 on 2026-10-07).
- [ ] **Docs updated** — `docs/architecture/**` schema and API pages for the
      new columns, endpoint and table; `docs/backfills.md` for the counted entry-state pass.
- [ ] **API types regenerated** — 1c and stage 2 touch `crates/api/**`.

## Notes

- Throwaway UI prototype (variants A/B/C) was shown outside the repo;
  stage 1 follows A, stage 2 follows B.
