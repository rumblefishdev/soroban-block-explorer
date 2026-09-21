---
title: 'Review of the phase-2 code, and the contract_transactions index'
type: synthesis
status: developing
spawned_from: notes/S-implementation-and-rollout-plan.md
spawns: []
tags: [review, clickhouse, api, rollout]
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/db-clickhouse/src/persist/stage.rs
  - crates/api/src/transactions/queries.rs
  - docs/deployment.md
  - docs/backfills.md
history:
  - date: 2026-09-21
    status: developing
    who: claude
    note: 'Review outcome, the index decision and what it changed'
---

# Review of the phase-2 code, and the contract_transactions index

## Two reviews of one commit

`abd5a801` against `origin/develop` `c4b3c839`, reviewed twice:

- **Deep protocol** — five lenses (correctness, production reality, security,
  devil's advocate, simplification), then a pattern-generalization pass and a
  judge with empty context who re-measured every contested number.
- **`/code-review`** — two axes, standards and spec.

Verdict: request changes; the design stands. The rpc event id was matched to
stellar-rpc's `internal/db/event.go` line for line by two lenses independently,
failed transactions included; a future `TransactionEventStage` variant breaks
the exhaustive `match` at compile time (CAP-67 says new fee events follow the
existing stages).

The deep protocol found everything that needs production, chain or query-log
access. The two-axis review, far cheaper, found six things it missed, among
them a hard violation of `CLAUDE.md` (`stage.rs`, 3,291 lines, touched, its test
files beside the code) and a positional `INSERT` in `docs/backfills.md` that
the rekey breaks. Conformance to the written rules and to the task's own list
is a different question from failure; both axes belong in the floor of any
review of this size.

## Fixed before merge

| finding                                                                                                                                    | fix                                                                                                                    |
| ------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| The contract-filtered transaction list reported the end on a capped page: a contract with 804 transactions showed 13 at the UI's page size | replaced by the index below                                                                                            |
| Nothing marked `develop`/`master` undeployable between merge and window                                                                    | `docs/deployment.md` section; `PROD:` note in `init.sql`                                                               |
| `config_pool_stage_real_e2e` / `pair_factory_stage_real_e2e` staged events without ids and panicked (env-gated, so CI was green)           | ids assigned as the indexer does; run on 23 archive registration ledgers, 20/20 and 3/3; shown to fail without the fix |
| The event-name recipe in `docs/backfills.md` was a positional `INSERT` naming `transaction_id`                                             | `SELECT * REPLACE`, verified read-only on both table shapes                                                            |
| Test files beside the code (`stage.rs`, `asset_transfers.rs`)                                                                              | moved; test counts unchanged (151, 451)                                                                                |

## Decisions (karolkow, 2026-09-21)

- **Build the per-(contract, transaction) presence index in this task, before
  merge.** The window mechanism compensated for its absence. Accounts, assets
  and pools each have one (`transaction_participants`,
  `operation_asset_appearances`, `operation_pools`); contracts had three partial
  sources, one of them per _event_, so no read could ask for "the next N
  transactions" and the code guessed how many rows made a page.
- **Fee events do not count as touching a contract.** Every transaction pays
  one to the native SAC, so counting it made that contract's list every
  transaction on the network (~171 M in partition 127 alone). User-visible:
  the native XLM contract's list now shows the transactions that use the SAC.

## `contract_transactions`

- `(contract_id, ledger_sequence, application_order)`, partitioned like its
  siblings, keyed by the transaction's position (ADR 0059) — the shape 0538
  moves the family to.
- Written at staging from the ledger's own rows: operation events, invocations,
  operations naming a contract. Invocations and operations carry the hash
  surrogate; it maps to the position through the ledger's `transactions` rows,
  and a miss is a staging error.
- **Fee rule** (`SorobanEventRow::is_operation_event`, and the same test in the
  fill SQL): `transaction_index = application_order AND operation_index != 4095`.
  On partition 127 its complement selects 249,035,471 rows = 170,740,563
  charges + 78,294,908 refunds, none outside the native SAC.
- **Read:** one seek, the account list's shape. `contract_positions.rs` is gone.
- **Size:** the three columns cost ~0.96 B/row in this sort order (measured on
  the rekeyed table); ~155 M pairs per partition (_estimate_: one 5k slice ×
  100); a few GiB in all (_estimate_).
- **Fill dry-run** (read-only, 63,700,000–63,705,000): 1,554,897 pairs, 8,369
  contracts, 740 ms, 586 MiB.

## Also found

- **Task 0517 timing.** The event-name backfill has not run. The rekey copy
  began with partition 127, which held 132,256 resolvable `NULL` names when
  copied (0.029% of the partition). Run 0517 after the swap; recorded in
  `docs/backfills.md`.
- **A client outside this repository** reads `soroban_events.transaction_id` and
  `event_index` in two query shapes, last on 2026-09-18; both break at the swap.
  A sweep of `system.query_log` from 2026-05-19 found no other foreign reader of
  `soroban_events`, `soroban_event_ops` or `asset_transfers` (a consumer rarer
  than the retention window would not show).
- **The phase-1 transaction-page figure** ("5 ms, 49 k rows") is a warm re-run of
  a statement without the shipped `JOIN ledgers`; ClickHouse 26.3 runs with the
  query condition cache on. Cold, the shipped statement is ~1.6–2× faster than
  the old one, not 16×. Adding `AND l.sequence = ?` to the join changes nothing
  — `EXPLAIN indexes=1` shows the key already pushed down.
- **The contract-events page** reads 1.32× the old rows on partition 127 (6
  unmerged parts against 2). The plan's `read_rows ≤ old` criterion was never
  evaluated; re-measure once the partition is merged.
  Re-measured 2026-09-21 after the operator merged the partition to one part
  (5.31 → 5.26 GiB, rows unchanged; the old partition has 2 parts). Native
  SAC, first page of 21, both bounded to partition 127; three runs each, each
  with a different upper ledger bound so the query condition cache never hits
  (the read-only profile cannot switch it off). Median: old 98 ms, 1,515,520
  rows, 447 MiB; new 92 ms, 1,490,944 rows, 393 MiB. The new page read fewer
  rows in all three runs (0.96–0.99×): the gate passes.
- **Canonical endpoint SQL 02** still described the pre-0541 UNION, reading
  `soroban_events.transaction_id`; updated with the index seek.
- **graphify** (structural, no LLM) drops calls written as `crate::fn(...)`, so
  cross-crate callers are missing from the graph: it listed 24 callers of
  `extract_events`, all in the parser, and none of the four that mattered.

## Owed before the window

1. Operator: `CREATE TABLE contract_transactions` verbatim from `init.sql`; trial
   on partition 127 (fill, gate, read path); then fill in the phase-3 loop.
2. Operator: `asset_transfers.event_index` gets its `DEFAULT` — gated by a read.
3. Hand the two external query shapes and their replacements to that client's
   owner, or accept its outage explicitly.
4. After the window: the fill SQL over the first ledgers the new indexer wrote
   must equal the writer's rows, `EXCEPT` both ways empty.

## Left as follow-ups

- ~~The operation position is held twice on `ExtractedEvent` (`op_index` /
  `event_pos_in_op` and `event_id`).~~ Not a duplicate: the pair is what the id
  is built from, and the only answer to "which operation emitted it" — a fee
  event's id names operation 0 or 4095. Reading the operation off the id, as
  the review proposed, would put fee events on the first operation's card.
- ~~`assign_event_ids` is a separate step held by convention.~~ One entry
  point now, `xdr_parser::LedgerEvents`; the two steps are private to the
  parser. `extract_events` still returns events without ids, so an un-id'd
  return type would go one step further.
- The transaction lists page within one partition (canonical SQL 02, pre-existing):
  a list ends at the partition boundary. The index would make crossing cheap.
  → task 0381, which already scopes partition-pinned lists and short pages,
  with the liquidity-pool, asset and ledger instances below.
- The NFT collection filter matches by name: 330 names on more than one contract,
  1,681 contracts, the worst name on 490 (measured 2026-09-20). The UI hides the
  filter (`COLLECTION_COLUMN_ENABLED = false`); only direct API callers reach
  it. → task 0486, whose collection view is keyed by contract.
- The liquidity-pool participants list shares the short-page defect; its own
  comment says so. Assets (`SEEK_OVERFETCH`) and ledgers (`LEDGER_OVERFETCH`)
  have the same shape, latent on today's data. → task 0381.
- 26 ClickHouse-gated tests in `crates/api` never run in CI.
- `HumanizedSentence` keys its links by a truncated address: two accounts with
  the same short form collapse, and the sentence links one to the other.
- `PoolActivity` keys a leg by its bare asset code: one code from two issuers
  gives two legs the same React key.
- A truncated WASM import section reads as "does not self-upgrade"
  (`xdr_parser::contract`, `Some(false)`), the same answer as a real negative;
  it feeds the upgradeable badge.
- Past `i16::MAX` ownership events for one NFT in one ledger,
  `xdr_parser::state` skips the rest with a warning. A documented guard against
  a hostile contract, not an oversight; no real contract has reached it.
