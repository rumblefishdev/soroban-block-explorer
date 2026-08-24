---
id: '0503'
title: 'OPS: exhaustive completeness audit — every indexed entity against real network state'
type: OPS
status: backlog
related_adr: ['0055']
related_tasks: ['0502', '0463', '0321', '0500', '0501', '0492']
tags: [ops, clickhouse, data-integrity, audit, priority-medium, effort-medium]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0463. Every completeness measurement this project has ever
      made was taken over our OWN data, which cannot see what we never
      ingested. The checkpoint snapshot removes that blindness for the first
      time. Blocked on task 0502 (the decoder).
---

# OPS: exhaustive completeness audit

## The blindness this removes

Every completeness check we have ever run sampled **our own tables** and
compared entries we already hold against the chain. That method cannot
detect an entity we never ingested at all — it has no row to sample. With
78.85 % of chain history predating our ledger floor, the size of that blind
spot has never been known.

The checkpoint snapshot (task 0502) is a full state snapshot of pubnet, so
for the first time the question is answerable in the correct direction:
**not "is what we have correct?" but "what does the network have that we do
not?"**

Two live examples of what the old method missed, both found in one day:
~52k merged accounts still holding phantom XLM (task 0321), and dead accounts
rendering as alive (task 0500). Both were found by accident, not by a check
designed to find them.

## Scope — per entity, both directions

For each entity we index, compare our deduplicated state against the
snapshot and report **four** numbers, never a single "match" percentage:

| Entity                    | Our table             | Snapshot entry                            |
| ------------------------- | --------------------- | ----------------------------------------- |
| accounts                  | `accounts`            | `AccountEntry`                            |
| classic + native holdings | `balances`            | `TrustLineEntry` + `AccountEntry.balance` |
| Soroban token holdings    | `balances` (type 3)   | `ContractData` balance entries            |
| LP positions              | `lp_positions`        | pool-share `TrustLineEntry`               |
| pools                     | `liquidity_pools`     | `LiquidityPoolEntry`                      |
| contracts                 | `soroban_contracts`   | `ContractData` / `ContractCode`           |
| signers + thresholds      | `account_entry_state` | `AccountEntry`                            |

Per entity, report:

1. **missing** — in the snapshot, absent from us (the blind spot),
2. **ghosts** — in us, absent from the snapshot (the 0321 class),
3. **divergent** — present in both, values disagree,
4. **stale** — present in both, our `lastModifiedLedgerSeq` is behind.

Report absolute counts **and** the value at stake where the entity carries
one (phantom XLM, mislabelled holders), because a count alone does not convey
whether a gap matters.

## Measured baseline 2026-08-18 (checkpoint 64,010,495) — the ledger of what is and is not compared

Raw per-type counts from `snapshot-tally` (a research probe since removed in
the 2026-08-20 review — re-measure via `snapshot-seed`'s dry-run distinct-entry
report or the 0502 decoder) (full 21-bucket pass, distinct
entries after first-wins where stated). NOTHING below is forgotten: every row
has an explicit status and an owner task.

| snapshot entry type      | network live (records unless noted) | our side                                                                                              | compared?                                                                       | owner           |
| ------------------------ | ----------------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | --------------- |
| `account`                | **10,863,731 distinct**             | `accounts` 14.5M ids, `balances` native                                                               | **YES** — four-way + RPC 260/260                                                | 0463 (done)     |
| `trustline` (classic)    | **32,344,912 distinct**             | `balances`                                                                                            | **YES** — four-way + RPC                                                        | 0463 (done)     |
| `trustline` (pool share) | **77,048 distinct live**            | `lp_positions`: 108,579 pairs, only 40,738 positive, 67,841 at zero (same live-zero/closed ambiguity) | counted, NOT diffed                                                             | 0499 / ADR 0056 |
| `liquidity_pool`         | 179,523                             | `liquidity_pools`                                                                                     | NOT diffed                                                                      | **this task**   |
| `contract_data`          | 17,124,415                          | `balances` type-3, `soroban_contracts`, `nfts`                                                        | NOT diffed (needs ScVal Balance-key decode; archived-state caveat, 0463 map T8) | **this task**   |
| `contract_code`          | 2,774                               | `soroban_contracts.wasm_hash`                                                                         | NOT diffed                                                                      | **this task**   |
| `offer`                  | 1,208,197                           | **no table** — and open offers lock funds we report as spendable                                      | tally only                                                                      | 0504            |
| `claimable_balance`      | 5,443,206                           | **no table** — value addressed to accounts, invisible                                                 | tally only                                                                      | 0504            |
| `data`                   | 100,449                             | **no table**                                                                                          | tally only                                                                      | 0504            |
| `ttl`                    | 16,987,781                          | **no table** — Soroban archival state; type-3 may over-report                                         | tally only                                                                      | 0504 / 0463 T8  |
| `config_setting`         | 54                                  | **no table** — network config, no product surface                                                     | tally only; deliberate skip candidate                                           | 0504            |

## The window discriminator — the audit's core verdict rule

For every discrepancy, read the entry's own `lastModifiedLedgerSeq` against
our ledger floor (50,457,424) and the seed checkpoint:

- **before the floor** → we never saw it; a coverage gap, not a defect;
- **inside our window** → the change passed through our parser and the result
  is still wrong: **we index incorrectly** — a bug with a reproduction ledger
  attached;
- **after the 0463 seed lands**, the first category collapses for seeded
  entities: any NEW discrepancy in accounts/trustlines/native IS an indexing
  defect (modulo export-vs-checkpoint skew churn, measured growing 1.5k→25k
  divergents with the gap — take the export minutes before the snapshot).

Proven live already: the 0463 comparison put 99.997% of 19.29M missing
trustlines below the floor and the 648 in-window ones were all post-export
churn — the parser's first full-population correctness pass.

## Standing check: same-version content ties (added 2026-08-19)

For every ledger-versioned RMT table, count keys carrying more than one
distinct content at the same version — ReplacingMergeTree resolves such a tie
arbitrarily, and `argMax` reads flip a coin:

```sql
SELECT count() FROM (
  SELECT <key cols>, <version col> FROM <table>
  GROUP BY <key cols>, <version col>
  HAVING uniqExact(<content cols>) > 1
);  -- slice big tables on the leading ORDER BY column
```

Baseline 2026-08-19: zero everywhere except `balances` (1,238,583 — root
cause proven in task 0463: the 2026-06-23 merge-tombstone fix vs a re-parse
of 54M–63.04M; fully repaired by the 0463 seed). Identical duplicate rows are
harmless (RMT collapses them); ONLY differing content at one version counts.
The mechanism recurs whenever a state writer's semantics change and old
windows are re-parsed — which is exactly what this audit exists to catch.

## In-ledger ordering audit (2026-08-19) — the SECOND tie source, checked table by table

Two distinct mechanisms can put two different contents under one key+version:
**(a) between runs** — a semantic writer change plus a re-parse of old windows
(the `balances` case above), and **(b) within one ledger** — two transactions
touching the same entity, where the writer must keep chain-application order.

(b) audited across the full schema:

| class                                    | tables                                                                                                                                                                                                 | verdict                                                                                                                                                                                                                                                                                                                             |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| state (final-state-per-ledger semantics) | `accounts`, `balances`, `account_entry_state`, `soroban_contracts`, `liquidity_pools`, `lp_positions`, `nfts`, `nfts_pending`                                                                          | every writer folds per key with LAST-wins in tx/op application order before insert (each verified at its emit site); chain order comes from processing txs in ledger order and changes in meta order. Regression tests exist for balances, signers, merge-then-recreate; MISSING for accounts/lp/pools/nfts folds — listed as a gap |
| fact with order column                   | `transactions` (application_order), `operations_appearances` (application_order), `soroban_events` (event_index), `nft_ownership(+_pending)` (event_order), `lp_operation_amounts` (application_order) | key distinguishes intra-ledger order — two real events cannot collapse                                                                                                                                                                                                                                                              |
| fact with per-tx aggregation             | `operation_asset_appearances` (`net_settled` computed per (tx, asset) BEFORE insert — `amount_by_tx_asset`)                                                                                            | collapse impossible by construction; the value is a per-tx net, not per-op                                                                                                                                                                                                                                                          |
| presence (collapse intended)             | `transaction_participants`, `operation_pools`, `soroban_invocations_appearances`, `operation_asset_appearances` (presence half)                                                                        | one row per (entity, tx) is the SEMANTIC — no order needed                                                                                                                                                                                                                                                                          |
| snapshot-per-ledger                      | `liquidity_pool_snapshots` (pool, ledger; no version)                                                                                                                                                  | several pool ops in one ledger emit rows under one key; RMT keeps the last inserted = last in apply order = end-of-ledger state, which IS the table's meaning. Deterministic under one code version; cross-run divergence falls under mechanism (a)                                                                                 |

**Fact tables measured for between-run divergence too (2026-08-19):** the
version-less fact tables cannot tie (no version column) but CAN hold
duplicates with different content if a re-parse changed what the parser
emits. Probed with `GROUP BY <full sort key> HAVING uniqExact(<content>) > 1`
over three 2k-ledger windows — 58.0M and 60.5M (inside the re-parsed band
that produced the `balances` ties) and 63.5M (fresh) — across `transactions`,
`operations_appearances`, `soroban_events`, `lp_operation_amounts`,
`nft_ownership`, `liquidity_pool_snapshots`: **zero divergent keys in all 18
probes**. The June re-parse changed only the account-state path, and the fact
writers emitted byte-identical rows. Sampled, not exhaustive — the full-census
version of this probe belongs to this audit's recurring run.
(`transaction_participants` is divergence-proof by construction: its key is
its entire content.)

Verdict for (b): **no table can lose or misorder an intra-ledger sequence
today.** The residual risk is untested folds (state-table column above) and any
FUTURE writer added without an order column — both are review-time checks.

Mechanism (a) has no in-schema defence and never will without lying about
versions: the arbiter is the NETWORK, via the snapshot reconciliation — see
`docs/backfills.md`, which now makes it a mandatory post-re-parse step.

## Deferred from 0463 (2026-08-24) — the three populations the seed counts but does not diff

The 0463 seed's `summary.txt` prints a **NOT COMPARED** block so the report can
never read as exhaustive. Those three lines are this task's inbox. Measured on
production the same day:

| population              | our rows    | network side                 | owner     |
| ----------------------- | ----------- | ---------------------------- | --------- |
| contract-held classic   | **70,347**  | `ContractData` (SAC balance) | this task |
| Soroban type-3 holdings | **72,369**  | `ContractData` (token)       | this task |
| pool shares             | 40,652 live | 77,048 live `TrustLineEntry` | 0499      |

### The first two are ONE piece of work, not two

A contract holding USDC and a contract holding a Soroban token are the **same
ledger entry**: `ContractData`. A contract has no trustline, so the snapshot's
`TrustLineEntry` set would call every one of those holdings a phantom — which is why
they are excluded rather than diffed. `snapshot.rs` today counts every
`ContractData` record into `unmodelled` and drops it.

One decoder unlocks both: recognise the SAC/token balance key shape
(`ScVec[ScSymbol("Balance"), ScAddress]`) and read the amount out of the entry
value. It belongs beside the existing `classify()` arm, and is the natural
first extension after 0502 extracts the module.

Counts are DISTINCT `(holder_id, asset_id)` keys. A first pass recorded
1,189,717 / 150,499 here — raw row counts, inflated 2-3x by unmerged
ReplacingMergeTree parts (and, for the first figure, by a JOIN against an
`assets` table carrying its own duplicates). Corrected 2026-08-24; the same
defect in the seed's own NOT COMPARED query was fixed the same day.

Until it exists, **~143k of our holdings have never been checked against
the network in either direction** — no ghost check, no missing check. The
classic gap measured 60% and the pool-share gap 47%; treating this population
as probably-fine is exactly the assumption this audit exists to kill.

### Pool shares are 0499's, with a number attached

Already decoded and deduplicated in `SnapshotState::pool_shares`, just never
diffed against `lp_positions`. Measured 47% short; recorded in 0499 with the
~100-line comparator sketch. It leaves this task's list when the ADR 0056 merge
folds pool shares into `balances`.

## Orphan holders in `balances` (added 2026-08-24, from 0463)

Found while chain-checking the seed's classic-ghost bucket: rows in `balances`
whose `holder_id` has NO row in `accounts` and is not a contract either.
Measured on one 1/64 `holder_id` slice: 9 orphan holders among 111,879, so
roughly **576 network-wide (0.008%)**.

Two consequences the audit must own:

- **They are unverifiable from any side.** No source carries their StrKey —
  not `accounts`, and not the checkpoint snapshot, which holds no
  `AccountEntry` for them live or dead. The 1,941 classic-ghost rows the 0463
  seed zeroes are exactly this population; the zeroing is safe (with no StrKey
  the account page cannot render them), but nothing can prove them right or
  wrong individually.
- **They are a parity defect in their own right**: some write path emitted a
  `balances` row without emitting the `accounts` row beside it. The audit
  should enumerate the full set (the slice query generalises), date them by
  `last_updated_ledger`, and attribute them to a writer — the same
  producer-attribution method the tie audit uses.

## Rules for the audit itself

- **Read-only.** This measures; remediation is a separate task per finding.
- **No Horizon.** Raw XDR only — Horizon is legacy and synthesizes fields the
  ledger does not carry.
- **Report the method with the number.** State what was measured exhaustively
  versus sampled, and the sampling rule where used.
- **Do not fold findings into this task.** Each real gap becomes its own
  task with its own measured scale, the way 0321/0500/0501 did.
- **Make it repeatable.** The value is in re-running it after every backfill
  or write-path change; a one-shot script that nobody can re-run is a failure
  of this task even if the numbers were right.

## Acceptance criteria

- [ ] **TOTALITY: every one of the snapshot's 10 entry types appears in the
      report** — either as a four-way diff against our table, or as an explicit
      exemption naming the owner task (offers/claimable/data/ttl/config → 0504,
      pool shares → 0499 until the merge). A type silently missing from the
      report fails this audit even if every reported number is right — "we
      never got to it" produced the 60% gap
- [ ] `ContractData` balance entries decoded and diffed — the ~143k
      contract-held + type-3 holdings the 0463 seed could only count
- [ ] `account_entry_state` diffed against `AccountEntry` signers on every run
      AFTER the 0463 seed (first fill has nothing to compare; from then on a
      divergence is a writer defect, not a gap)
- [ ] Four-way counts per entity, with the method stated per number
- [ ] Value-at-stake reported wherever the entity carries value
- [ ] Every non-trivial gap filed as its own task with its measured scale
- [ ] Re-runnable by someone who was not here — documented invocation
- [ ] **Docs updated** — `docs/backfills.md` gains the audit as a procedure
- [ ] **API types** — N/A
