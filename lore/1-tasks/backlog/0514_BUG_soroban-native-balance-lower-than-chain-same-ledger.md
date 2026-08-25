---
id: '0514'
title: "BUG: native balance written lower than the chain's for the same ledger on Soroban transactions"
type: BUG
status: backlog
related_adr: ['0057']
related_tasks: ['0463', '0503']
tags:
  [
    bug,
    indexer,
    soroban,
    balances,
    data-integrity,
    priority-high,
    effort-medium,
  ]
links: []
history:
  - date: '2026-08-24'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0463. Found by the snapshot-seed comparison: the
      divergent-same-ledger quarantine bucket turned out to be one-directional
      on its FULL population (17,798 of 17,798 rows — our amount lower), live
      (ledgers up to the checkpoint itself), and Soroban-only (106 of 107
      recent account/ledger pairs carry a Soroban tx, zero classic).
---

# BUG: native balance lower than the chain's at the same ledger (Soroban path)

## Summary

For accounts touched by a Soroban transaction, the indexer records a native
XLM balance that is LOWER than what the chain holds for that exact ledger.
The write is otherwise correct (right account, right ledger); only the amount
is behind. The live writer keeps producing these.

## Measured facts (2026-08-24, checkpoints 64,106,495 / 64,107,135)

All figures from the `snapshot-seed` dry-run comparison plus independent RPC
decoding (task 0463 audit; method in that task's
`notes/V-chain-audit-method.md`).

- **17,798 rows** in the divergent-same-ledger bucket: our row and the
  chain's entry carry the SAME `lastModifiedLedgerSeq`, different amounts.
- **One-directional on the full population, not a sample**: 17,798 ours-lower,
  0 ours-higher, 0 equal.
- **Live, not historical**: ledgers run to the checkpoint itself; the newest
  band (>= 64.0M, roughly a week) holds 107 rows spread over 96 distinct
  ledgers — a continuous process. Extrapolated rate ~1,900 rows/week.
- **Soroban-localised**: of the 107 recent (account, ledger) pairs, our
  `transactions` table shows a Soroban transaction from that account in that
  ledger for 106 — and zero classic-only pairs.
- Differences are small (typically 0.001–0.007 XLM) and cluster hard: 249
  distinct values across 1,000 sampled rows (9,614 stroops on 415 accounts,
  9,624 on 102, 77,268 on 73). Magnitudes are consistent with fee-refund-sized
  amounts.

## Hypothesis refuted (do not re-try)

"The parser misses a fourth meta container carrying the unused-resource-fee
refund." False: `TransactionMetaV4` (stellar-xdr 26.0.1) has exactly
`tx_changes_before` / `operations` / `tx_changes_after`, and
`extract_ledger_entry_changes` reads all three.

Open lead, not a cause: the refund also surfaces as a `TransactionEvent` with
`stage = AfterAllTxs` (pinned against mainnet in
`crates/xdr-parser/tests/tx_event_stage_real_meta.rs`), and the balance write
path never reasons about stages — but it consumes entry CHANGES, not events,
so this only matters if the final balance state lands somewhere the change
iteration mis-orders or drops. The investigation should diff our applied
change sequence against the chain's final entry for one affected
(account, ledger) pair, byte by byte.

## Scope

1. **Root cause** in the Soroban write path (`xdr-parser` change extraction →
   `db-clickhouse` persist), proven on a reproduced pair, not inferred.
2. **Fix the writer**, with a regression test on real meta from an affected
   ledger.
3. **Repair the accumulated rows.** The mechanism is already designed and
   dry-run-verified in task 0463, then deliberately removed from the seed: a
   `--heal-same-ledger` mode that supersedes both sides with the NETWORK's
   value at the CHECKPOINT version (the checkpoint strictly outversions the
   tie; writing at the tied ledger would only add a third same-version
   candidate for the ReplacingMergeTree coin flip). Its dry run healed 17,798
   rows; 200 sampled healed values were verified equal to chain via RPC.
   Resurrect that flag HERE, after the writer fix — running it while the
   writer still produces new ties would decay silently.

## Why the seed (0463) does not cover this

The seed writes the network's value only for rows it classifies as missing,
closures, ghosts or snapshot-newer heals. Same-ledger ties are quarantined and
left untouched — correcting the symptom from the seed while the writer keeps
producing new ties would need re-running forever. Order matters: fix writer,
then heal once.

## Acceptance criteria

- [ ] Root cause identified and written down, with one affected
      (account, ledger) pair decoded end to end as evidence
- [ ] Writer fixed; regression test on real meta from an affected ledger
- [ ] Accumulated rows healed to the chain's value (the 0463 heal design),
      full list kept as an artifact
- [ ] The standing tie-audit query (0503) returns zero NEW rows for a week of
      post-fix ledgers
