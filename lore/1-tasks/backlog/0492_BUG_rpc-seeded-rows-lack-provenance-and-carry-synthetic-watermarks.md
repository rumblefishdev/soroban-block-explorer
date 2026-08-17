---
id: '0492'
title: 'BUG: RPC-seeded account/balance rows carry no provenance and a synthetic watermark'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0421', '0425', '0463', '0420']
tags:
  [
    backend,
    clickhouse,
    backfill-runner,
    data-integrity,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0463 planning map. Found while auditing runtime
      dependencies, then confirmed by a targeted read-only probe and
      corroborated a third time, unsought, by a separate investigation into
      account watermarks. Production data is affected today; the values look
      correct, the metadata does not.
---

# BUG: RPC-seeded rows carry no provenance and a synthetic watermark

## Summary

`backfill-runner`'s bootstrap asks a Soroban RPC for `LedgerKey::Account`
state, decodes it, and stages it into the ClickHouse `accounts` and `balances`
tables next to parser-emitted rows — stamped `last_seen_ledger = window_end + 1`
so it wins the ReplacingMergeTree race. Two problems follow, neither of them
about the values themselves:

1. **No provenance.** Nothing on the row records that it came from an RPC
   rather than from the archive, so no consumer can tell chain-derived state
   from imported state.
2. **A synthetic watermark.** `getLedgerEntries` returns the node's current
   head; the response's `latestLedger` is deserialized
   (`rpc_snapshot.rs:584-585`) and never read. A snapshot taken while replaying
   an old window therefore records present-day state under a historical
   watermark, and `last_seen_ledger` / `last_updated_ledger` stop being usable
   as change watermarks for **everyone**, not just for these rows.

## Evidence

Established read-only; the full write-up lives in the 0463 planning map
(`.wayfinder/0463/answers/T2.md` and `T7.md`, local, not committed).

- Prod fingerprint: `accounts.last_seen_ledger` spikes of **395,994 @
  62528000** and **316,433 @ 60096000** — exactly the two documented `--end`+1
  boundaries. Next-highest real ledger count is 746; actual participant counts
  at those two ledgers are 522 and 417. Corroborated in `balances`.
  `repair_tier1` was checked and ruled out as the author.
- Operator artefacts: a captured `pre-export-metrics.json` records two real
  passes (`staged=602282`) against a public RPC, and a production validation
  report names "bootstrap-only RPC-seeded accounts".
- **Blast radius: 628,076 accounts (4.33 %) and 897,448 native balances
  (6.31 %)** currently serve RPC-sourced state.
- Independent corroboration: a separate investigation into dormant-account
  watermarks measured **~4 %** of `accounts.last_seen_ledger` /
  `balances.last_updated_ledger` values as synthetic 64k-boundary stamps,
  without looking for this defect.

Values themselves decode cleanly, so this is a metadata defect, not
corruption. Two confidence gaps stay open honestly: two machines' bootstrap
counters were never captured, and the affected-row counts are point-in-time.

## Why it is not spent

Task 0425 deliberately kept `bootstrap` as a recurring mop — 61.7 % of
transacting accounts read `sequence_number = 0` — and 0421 has not landed. The
mechanism runs again on the next backfill.

## The fix already exists in-repo

`balance_seed.rs:165` versions on the entry's own `lastModifiedLedgerSeq`
rather than on the window boundary. That is the correct pattern; `bootstrap.rs`
(around `:365-386`) does not follow it.

The synthetic watermark doubles as an accidental marker, so affected rows are
targetable **without** a re-parse.

## Acceptance criteria

- [ ] Rows written from an RPC snapshot carry explicit provenance
- [ ] The watermark comes from the entry's own `lastModifiedLedgerSeq`, never
      from the replay window boundary — `latestLedger` is read or the reason it
      is not is recorded
- [ ] Existing affected rows are repaired or re-derived, and the count is
      verified afterwards rather than assumed
- [ ] `last_seen_ledger` / `last_updated_ledger` are documented as trustworthy
      change watermarks once the repair lands, since other work wants to depend
      on them
- [ ] Coordinated with 0421 and 0425 rather than racing them
- [ ] **Docs updated** — `docs/backfills.md` describes the bootstrap knob and
      must state what it stamps
- [ ] **API types regenerated** — `N/A` unless a provenance field reaches a DTO

## Relation to 0463

0463 (account detail: complete balances and signers) found this while choosing
its own source of truth, and must not lean on either watermark as a change
signal until this lands. It is otherwise independent — 0463 does not wait for
it.
