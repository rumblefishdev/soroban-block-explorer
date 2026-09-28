---
id: '0424'
title: 'BUG: NFT ownership order is ambiguous within a ledger — current owner can be nondeterministic'
type: BUG
status: active
related_adr: ['0059']
related_tasks: ['0415', '0538', '0586']
tags:
  [
    'xdr-parser',
    'clickhouse',
    'indexer',
    'nft',
    'phase-future',
    'effort-medium',
    'priority-medium',
  ]
links:
  - crates/xdr-parser/src/state.rs
  - crates/api/src/nfts/queries.rs
history:
  - date: '2026-07-21'
    status: backlog
    who: karolkow
    note: >
      Found while auditing log-sourced facts (0415). Independent of the
      events-vs-ledger debate — this is our own ordering bug and must be fixed
      whichever source NFT ownership ends up reading.
  - date: '2026-09-28'
    status: active
    who: karolkow
    note: >
      Activated as epic 0538 step 4 (thread 292 A): the ownership rows move to
      the canonical event location, which removes the last two tables carrying
      `transaction_id`. Decided: history filled from `soroban_events` through
      the same Rust extraction (293 B2); `event_order` leaves the API for the
      position (294 A).
---

# BUG: NFT ownership order is ambiguous within a ledger

## Summary

`nft_ownership.event_order` is **our own counter**, not a chain ordering. It is
keyed `(contract_id, token_id, ledger_sequence)` and restarts at `0` for every
token in every ledger, so it does **not** encode which transaction within the
ledger came first. `nfts.current_owner_ledger` — the RMT version that decides the
current owner — is **ledger-only**. When one token has two or more ownership
events in the same ledger, the winner is decided by the merge, i.e.
**nondeterministically**, and the displayed current owner can be wrong.

## Context

`crates/xdr-parser/src/state.rs` (`extract_nft_ownership_events`):

```rust
let mut order_counter: HashMap<(String, String, u32), u16> = HashMap::new();
…
let key = (event.contract_id.clone(), token_id.clone(), event.ledger_sequence);
let counter = order_counter.entry(key).or_insert(0);
```

Consequences:

- Ordering by `(ledger_sequence, event_order)` is only meaningful **across**
  ledgers. Within a ledger it compares two counters that both start at `0`.
- `mint` + `transfer` in a single transaction (mint to treasury, then transfer to
  buyer) is a **mass pattern**, not an edge case — so the ambiguous case is common.
- Any analysis that folds this stream to a "current owner" inherits the ambiguity.
  This already produced a false positive during the 0415 audit: a consistency
  check read the ordering as authoritative and flagged 88 tokens as having a
  "transfer before mint", which the ordering data cannot actually establish.

**Measured on prod (2026-07-21):** **88 tokens** have more than one ownership
event inside a single ledger — that is the population at risk. This is NOT a
claim that all 88 currently display the wrong owner; it is the set where the
answer is not determined by the data.

```sql
SELECT count() FROM (
  SELECT contract_id, token_id, ledger_sequence, count() AS ev
  FROM nft_ownership GROUP BY contract_id, token_id, ledger_sequence
  HAVING ev > 1);
```

## Epic 0538 step 4 — the canonical location (2026-09-28)

**Measured (read-only):** `nft_ownership` 23,504 rows (398 KiB),
`nft_ownership_pending` 521; 24,025 rows over 11,454 ledgers and 139
contracts, from ledger 51,827,994. No storage gain — the step is what lets
`transactions.id` (31.6 GiB) go, and it fixes this task's order.

**Found in the code** (read-only map, 2026-09-28):

- The position exists upstream and is dropped: `ExtractedEvent.event_id`
  (`EventId {ledger, transaction_index, operation_index, event_index}`) is
  not copied by the `NftEvent` constructors (`xdr-parser/src/nft.rs`).
- NFT events are per-operation contract events, so every one has an rpc id
  in every protocol (only transaction-level events without a stage lack one).
- `consecutive_mint` expands one event into many rows with one id — the key
  keeps `token_id`.
- Readers: the transfers tab keys `(ledger_sequence, event_order)` —
  **corrected 2026-09-28:** it reads ONE token, whose counter is distinct
  within a ledger, so its `LIMIT 1 BY` does not collapse distinct rows (the
  first map said it did); `accounts/balance_changes.rs` joins on `transaction_id` and infers the
  moved pieces from the owner timeline; `event_order` is on the wire
  (`NftTransferItem`) and in the frontend row key.
- Promotion `_pending` → live is `INSERT … SELECT *`
  (`backfill-runner/src/nft_reclassify.rs`): both tables change together.

**Decided (karolkow):**

- Work tracked here, not in a new task (292 A).
- History filled from `soroban_events`: the rows' contract events are read
  back (topics / data are the parser's own JSON, stored verbatim) and run
  through the same `detect_nft_events` → `extract_nft_ownership_events` as
  the indexer — no S3 (293 B2). Gate: whole rows against the old tables,
  position aside.
- `event_order` leaves the API; a transfer carries `application_order`,
  `operation_index`, `event_index` like the other lists (294 A).

**Plan — parallel change, as tasks 0372 / 0586:** new tables beside the old
ones (key `(contract_id, token_id, ledger_sequence, application_order,
operation_index, event_index)`, no `transaction_id`, no `event_order`), dual
write, fill, readers, stop the old writes, drop.

- **Names** (295 A): `nft_ownership_changes`, `nft_ownership_changes_pending`
  — a row is any change of owner (mint, transfer, burn).
- **`nfts` current owner** (296 A): its same-ledger tie is fixed in its own
  PR after the readers — another table, another step.
- **PRs** (297 A):

| PR  | What                                                                                         | Deploy                 | Operator           |
| --- | -------------------------------------------------------------------------------------------- | ---------------------- | ------------------ |
| 0   | move: `nft.rs` tests → `nft/tests.rs`, NFT state → `state/nfts.rs`                           | no                     | —                  |
| 1   | parser carries the event position; new tables beside the old; promotion moves both           | yes                    | `CREATE` ×2 before |
| 2   | fill tool from `soroban_events` + gate against the old tables                                | no (run from a laptop) | —                  |
| 3   | readers: transfers tab by position, new wire fields + frontend; `balance_changes` exact join | yes                    | —                  |
| 4   | old tables no longer written; allowlist empty                                                | yes                    | `DROP` ×2 after    |

- **PR 0 opened** (2026-09-28):
  [#521](https://github.com/rumblefishdev/soroban-block-explorer/pull/521),
  `refactor/0424-move-nft-parsing` — `nft.rs` 1,203 → 425, `state.rs`
  1,344 → 1,191; 926 lines moved, glue only; `xdr-parser` 501 tests pass.
- **PR 1 (write both)** — branch `feat/0424-nft-ownership-changes-dual-write`,
  local, stacked on #521: `50ea9de1` — `NftEvent` / `ExtractedNftEvent` keep
  the source `event_id`; `nft_ownership_changes{,_pending}` (DDL, one row
  struct for both, staging beside the old pair, writer); staging refuses a
  change without an event id; `nft-reclassify` moves both pairs; merge
  scripts list the new tables; `stage.rs` 2,949 → 2,948. `eca010a1` — schema
  overview §4.13.2, pipeline, deployment step 1. Checks: workspace clippy
  clean; parser test that a `consecutive_mint`'s tokens share one id and a
  transfer keeps its own; staging tests (hot / pending / dropped routing of
  the new rows, refusal without an id); `db-clickhouse` all tests pass on a
  local ClickHouse 26.3 (the G9 e2e writes and reads the location);
  `xdr-parser`, `backfill-runner`, `indexer`, `api` 1,294 tests pass.
- **#521 merged; PR 1 opened** (2026-09-28) as
  [#523](https://github.com/rumblefishdev/soroban-block-explorer/pull/523),
  base `develop`, no `/code-review` (thread 300 B).

**NFT task sweep** (2026-09-28, read-only; 18 tasks) — what rides with this
work:

- **0497's mint ledger moves in PR 3** (not optional): `/nfts` derives
  `minted_at_ledger` from `nft_ownership`; the PR 4 drop would break it.
- **0542's admin-as-owner fix stays in 0542** (thread 301 B): a
  `[mint, admin, to]` mint stores the admin as owner (`nft.rs` `extract_args`,
  `>=` then the first address) — 46 events over 5 of our collections. The
  PR 2 fill copies today's behaviour; 0542's fix then re-derives it.
- **0376's contract owners** — its own small PR right after PR 3 (thread
  303 A): 31% of NFT owners are contracts and the list resolves owners
  through `accounts` only (0376's figure, 2026-09-08).
- Unblocked by this task, later: 0558 (token id on `asset_transfers`),
  0415's re-check of the 88 same-ledger tokens.
- Housekeeping (304 A): 0529 and 0531 (already `completed`) and 0259
  (closed, its check passed) moved to archive.

- **PR 1 review fix** (2026-09-28, `38040973`, pushed to #523): the change's
  `application_order` now comes from the ledger's transaction order
  (`app_order_by_hash`), as `soroban_events` stages its rows — not from the
  rpc id's `transaction_index` through an `i16` conversion (ADR 0059 keeps the
  two apart). The routing test gives the id a `transaction_index` that differs
  from the position, pinning the source.
- **PR 2 (fill)** — branch `feat/0424-nft-ownership-changes-fill`, local,
  stacked on #523: `a31bfc80` — `backfill-runner nft-ownership-fill`: the
  collections' contract events read back from `soroban_events`, run through
  `detect_nft_events` → `extract_nft_ownership_events`, located as the live
  writer does, routed by today's verdict; `--dry-run` is the gate (multiset
  against the old tables on contract, token, ledger, owner, type). `b572c4ed`
  — `backfills.md`. **Dry run on production (read-only, 2026-09-28): 31,093
  events → 23,540 hot + 521 pending = the old tables' 23,540 + 521,
  `only_old=0 only_new=0`.** Every located key is distinct (a shared key would
  have collapsed and shown in `only_old`). Tests: routing by verdict, position
  from the row, one `consecutive_mint` under one id, bad stored JSON is an
  error, the multiset difference.
- **PR 2 opened** (2026-09-28):
  [#525](https://github.com/rumblefishdev/soroban-block-explorer/pull/525),
  draft, stacked on #523.
- **PR 3 (readers)** — branch `feat/0424-nft-ownership-changes-readers`,
  local, stacked on #523: `87d8f1ce` — transfers tab on
  `nft_ownership_changes` (keyset and `LIMIT 1 BY` on the location,
  `transactions` joined on the position); `NftTransferItem` carries
  `application_order` / `operation_index` / `event_index` in place of
  `event_order` (294 A), the cursor the same (an old cursor fails to decode →
  400); mint ledger on `/nfts` and the detail from the new table (0497's
  reader); `balance_changes` names pieces by `(ledger_sequence,
application_order)` — `TxKey.transaction_id` and the account page's
  `t.id` read are gone; the SPA keys transfer rows by the location; API types
  regenerated. New smoke `same_ledger_changes_come_in_chain_order` (newest
  first by location, each `from_account` the previous change's owner).
  `041b2d2f` — canonical SQL 15–17, FINAL table, deployment step 2 (after the
  fill's gate, with the SPA). Checks: clippy clean; `api` 675 tests pass;
  NFT smokes on a local ClickHouse seeded with one token changed twice in one
  ledger (they skip on an empty one): 5 pass, the new one ran; web 397 tests,
  typecheck, lint clean. Not exercised: `resolve_moved_pieces` against data
  (runs only on a page with an NFT movement) — the new tables do not exist
  on production yet.
- **#523 merged** (2026-09-28, 12:49 UTC) — PR 1 is on `develop`. **#525
  merged 13:19 UTC into #523's branch after #523 had merged**, so the fill
  tool is NOT on `develop`: it is recovered on
  `feat/0424-nft-ownership-fill-to-develop` (the stranded commits merged onto
  `develop`, plus `0b8f5c93` marking it temporary — thread 314 A: PR 4
  deletes it with the old tables). Needs its own PR.
- **PR 3 opened** as [#527](https://github.com/rumblefishdev/soroban-block-explorer/pull/527),
  draft, base `develop` (`develop` merged in: the canonical SQL set was
  retired by ADR 0060 / task 0588, so this branch's edits to SQL 15–17 went
  with it; `NftTransfers.tsx` took `develop`'s new `DataList` with the
  location row key). Checks after the merge: `api` 679 tests, web 396, clippy,
  typecheck, lint clean.
- **Fill tool on `develop`** as
  [#528](https://github.com/rumblefishdev/soroban-block-explorer/pull/528)
  (merged 2026-09-28). Merge order: #528 first, #527 only after the fill —
  every Compute deploy ships all of `develop`, so readers merged earlier would
  go out with an empty history.
- **Step 1 deployed** (2026-09-28): both tables created by the operator and
  checked against `init.sql` in `system.columns` (columns, types, sort key);
  Compute from `develop` `f1e53c45`, indexer 14:26:33 UTC. No NFT change
  reached the indexer in the first minutes (last one at 64,663,865, before
  the deploy), so the live write is still to be seen.
- **History filled** (2026-09-28). Dry run (read-only) 14:37 UTC: 31,118
  events → 23,550 hot + 521 pending, 0 dropped, `only_old=0 only_new=0`.
  Real run by the operator 14:44 UTC with a write identity, same numbers.
  The command writes only the two new tables (`INSERT`, append-only; no
  `ALTER` / `DELETE` / `OPTIMIZE` in its path — the `ALTER` in `sink.rs` is
  test-only). After: `nft_ownership_changes` 23,552, `_pending` 521 (`FINAL`)
  = the old tables' 24,073; the two extra hot rows are the live indexer's,
  head at ledger 64,664,182 — the step-1 writer works on production.
  The WARN lines (`no known arg shape parsed`) are contracts
  `CBMKSLJL…`, `CD7ZVM24…`, `CAK7EUVA…`, `CBBVYBTC…`: the same shapes the
  live indexer drops, so the old tables lack them too.
- **Step 2 (readers, #527) deployed** (2026-09-28): API 14:53:48 UTC, SPA
  14:54:14 UTC (from `develop` `2c762bc0`). Deployed API through the dev
  proxy: token 18 of `CCIP47L5…` (two changes in one operation) lists
  event 1 `mint` then event 0 `transfer` — the chain's own order, checked
  against the stored events (the contract emits `transfer` from itself, then
  `mint`); cursor paging walks both rows; an old `event_order` cursor
  answers 400. `api_reader` reads only `nft_ownership_changes` after the
  deploy, 0 query exceptions. The SPA chunk `NftDetailPage` carries the new
  fields.
- **Step 3 (stop the old writes, #533) deployed** (2026-09-28): indexer and
  API 18:21:37 UTC, SPA 18:22:05 UTC (from `develop` `e7736c58`). Last write
  to `nft_ownership` 18:18:25 UTC (head ledger 64,666,744); from 18:22:24 the
  indexer writes `nft_ownership_changes` alone; 0 write or read exceptions.
- **Proof before the drops** (read-only, 18:50 UTC, `FINAL`, multisets with
  `EXCEPT ALL` so a NULL owner compares equal):
  - hot 23,583 old vs 23,587 new, pending 521 vs 521 — on (contract, token,
    ledger, owner, type): 0 only old; 4 only new, all past the old head
    (ledgers 64,666,792–64,666,821, written after the switch);
  - with the transaction: every old `transaction_id` resolves in
    `transactions` (0 of 24,104 missing), and (contract, token, ledger,
    `application_order`, owner, type) through it equals the new rows: 0 / 0
    — the change of index is the only change;
  - every one of the 24,108 new locations is a contract event in
    `soroban_events` for that contract (0 without).
  - prices-api: no `prices_*` read of either table in 14 days;
    `api_reader` last 13:13 UTC, before the readers deploy.
- **Both old tables dropped** by the operator (2026-09-28, ~18:55 UTC):
  `system.tables` 0; ingest at the network head, 0 write or read exceptions;
  `nft_ownership_changes` 23,588 + `_pending` 521. No column named
  `transaction_id` remains on production (`system.columns`: 0) — only
  `transactions.id` itself, which epic 0538 retires next. Parallel change
  done; still open here: the `nfts` current-owner tie (296 A / W306), the
  CAP-67 stage subtask and the state-table audit.

## Implementation

- Thread the transaction's **application order** (and the event's index within the
  transaction) into `NftEvent` / `ExtractedNftEvent`, so a total order exists:
  `(ledger_sequence, application_order, event_index)`.
- Use that tuple for `event_order` (or add columns) and for the `nfts` RMT
  **version**, so the same-ledger tie is resolved by data, not by the merge.
- Backfill/re-ingest the affected range; verify the 88 at-risk tokens resolve
  deterministically afterwards.
- Add a regression test: two ownership events for one token in one ledger, in both
  emission orders, must yield the later one as current owner.

## Subtask: event display order ignores the CAP-67 stage

Same class (ordering thrown away at ingest), different table — found in the
2026-07-21 audit.

`TransactionEvent.stage` is decoded but never persisted or used
(`crates/xdr-parser/src/event.rs`), and tx-level events are numbered `0..k`
**before** the per-operation events. Both read paths then sort by `event_index`
ascending (`crates/api/src/contracts/queries.rs`, and the tx-detail split).

CAP-0067 places the initial fee charge at `BEFORE_ALL_TXS` and the **fee refund at
`AFTER_ALL_TXS`** — i.e. after every transaction in the ledger. Numbering the
refund into the low indices renders it **before** the contract events that caused
it, which is simply the wrong story on the transaction page.

Reported measurement (from the audit agent, **not independently re-verified** —
confirm before acting): in ledgers 63,578,000–63,578,074, `event_index = 0` is a
`fee` event in 36,635 transactions; `event_index = 1` is `fee` in 12,188 and
`transfer` in 2,780.

Fix: carry `stage` through to storage and order by `(stage, application_order,
index-within-stage)` rather than by a flat ingest counter. Verify the CAP-67 stage
semantics against the spec first — the protocol, not our current numbering, is the
authority on what order these belong in.

## Subtask: is the same tie possible in the OTHER state tables?

This is a **class** of bug, not one table. Any `ReplacingMergeTree` whose version
column is **ledger-only** cannot break a tie between two writes for the same key
inside one ledger — the merge picks arbitrarily. `init.sql` has ~9 such tables:

| Version column            | init.sql                                                      |
| ------------------------- | ------------------------------------------------------------- |
| `last_seen_ledger`        | 157                                                           |
| `wasm_uploaded_at_ledger` | 210                                                           |
| `last_updated_ledger`     | 388, 412, 506, 516                                            |
| `current_owner_ledger`    | 442, 468                                                      |
| `version`                 | 242, 341, 489 — check whether this one is composite/monotonic |

**A working mitigation already exists in the codebase** — copy it rather than
inventing one. `persist::stage::build_balance_rows` dedups by key keeping the LAST
occurrence _before_ insert, precisely because "two txs in one ledger can touch the
same holder+asset, producing rows that share the RMT version … a tie the merge
would resolve nondeterministically". So `balances` is mitigated **in Rust**, not by
the version column.

Audit each table above and record one of:

- **mitigated in-process** (like `build_balance_rows`) — note where, and check the
  guarantee still holds if the same key is written by a _different_ path (live
  ingest vs S3 re-ingest overlap, or two writers in one run), which in-batch dedup
  does not cover;
- **not mitigated** (like `nfts`) — same defect as this task, fix the same way;
- **not applicable** — same-ledger double-write for one key is structurally
  impossible; state why.

Note the `version`-based tables (242/341/489) may already be the correct pattern —
if so, promote it as the convention instead of spreading in-process dedup.

## Acceptance Criteria

- [ ] Ownership events carry a total, chain-derived order within a ledger
- [ ] `nfts` RMT version breaks same-ledger ties deterministically
- [ ] Re-ingested range: the 88 at-risk tokens resolve to a stable current owner
      across repeated merges
- [ ] Regression test covers both emission orders in a single ledger
- [ ] 0415's consistency checks re-run against the corrected ordering (the earlier
      "transfer before mint" signal must be re-evaluated, not carried over)
- [ ] Every ledger-only-versioned RMT table audited and classified
      (mitigated in-process / not mitigated / not applicable), with the in-batch
      dedup's cross-path limitation assessed
- [ ] A single convention chosen and written down (composite version column vs
      in-process last-wins), so new state tables do not reintroduce the tie
