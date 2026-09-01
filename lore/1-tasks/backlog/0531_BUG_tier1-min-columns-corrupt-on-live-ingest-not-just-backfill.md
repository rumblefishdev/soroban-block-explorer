---
id: '0531'
title: 'Tier-1 MIN columns corrupt on LIVE ingest, not only under parallel backfill — derive all six at read time'
type: BUG
status: backlog
related_adr: ['0040', '0044', '0045']
related_tasks: ['0528', '0322', '0228']
tags: ['clickhouse', 'data-integrity', 'api', 'indexer', 'effort-medium']
links: []
history:
  - date: '2026-09-01'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0528. 0528 fixed ONE of six Tier-1 columns; the same
      MIN-under-RMT defect corrupts the other five, and prod sampling confirms
      it. Also corrects a premise carried since 0228: the corruption is NOT
      confined to cross-machine parallel backfill — it happens on ordinary live
      ingest, every time a later event lands in a later batch. This is the gap
      task that 0322 item (G) anticipated.
---

# Tier-1 MIN columns corrupt on live ingest, not only under parallel backfill

## Summary

Six columns hold a MIN-over-history value inside a `ReplacingMergeTree` state
table. RMT keeps the row with the highest version and replaces it **whole**, so
any later event overwrites the historic minimum. `repair-tier1` recomputes them
from append-only fact tables as a one-shot pass.

0528 established that for `nfts.minted_at_ledger` this is not a backfill
artifact but an ongoing live-ingest defect, and fixed it by deriving the value at
read time. The same treatment is owed to the remaining five columns.

## The premise this corrects

`repair_tier1.rs` and task 0228 attribute the corruption to cross-machine
parallel backfill: worker N stamps a first-seen value for its own ledger range
with no visibility into earlier ranges, and the post-merge RMT collapse keeps the
latest writer's value.

That is true but incomplete. The same thing happens with a single indexer on
ordinary live ingest: the indexer sees only the current batch, so a transfer,
burn or later appearance carries no historic minimum, and the RMT replace erases
whatever was there. Under that reading `repair-tier1` is not a post-backfill
step — it is a recurring cleanup for a defect that never stops producing.

## Measured on prod (2026-09-01)

| Table               | Column                 | Finding                                                                                                                                               |
| ------------------- | ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `nfts`              | `minted_at_ledger`     | 623 / 13 915 NULL (4.5%), ~30/day — **fixed by 0528**                                                                                                 |
| `accounts`          | `first_seen_ledger`    | **14 / 400 sampled (3.5%) diverge**, all of them LATER than the true first appearance. Extrapolates to ~570k of 16.17M accounts                       |
| `soroban_contracts` | `deployed_at_ledger`   | **1 597 / 146 397 diverge (1.1%)**, plus 3 NULL where the value is known                                                                              |
| `nfts_pending`      | `minted_at_ledger`     | 4 / 277 NULL (1.4%)                                                                                                                                   |
| `lp_positions`      | `first_deposit_ledger` | **not measured** — `operations_appearances` (6.85 B rows) sorts on `ledger_sequence`, so a per-pool check is a full scan. Same defect by construction |
| `soroban_contracts` | `deployer_id`          | not measured separately; shares the rebuild with `deployed_at_ledger`                                                                                 |

`accounts.first_seen_ledger` is the most exposed: it is served on the account
detail response and the account list, and rendered in both
(`AccountSummary.tsx`, `AccountsTable.tsx`). `lp_positions.first_deposit_ledger`
is on the wire too.

Sampling note: the `accounts` figure comes from a 400-row deterministic slice
(`id % 4001 = 7`) joined to `transaction_participants` on the sort key. A full
check is not affordable — `transaction_participants` holds 10.73 B rows. Treat
3.5% as an estimate with a real sample behind it, not a census.

## Approach

Follow 0528: stop reading the stored column, derive from the append-only fact
table in the query. Correct values appear immediately, retroactively, with no
migration, no `EXCHANGE TABLES` and no indexer stop.

| Column                                                 | Derivation                                                                                   |
| ------------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| `accounts.first_seen_ledger`                           | `min(ledger_sequence)` over `transaction_participants`                                       |
| `lp_positions.first_deposit_ledger`                    | `min(ledger_sequence)` over `operations_appearances WHERE type = 22`                         |
| `soroban_contracts.deployed_at_ledger` + `deployer_id` | `min(wasm_uploaded_at_ledger)` + `argMin(deployer_id, …)` over rows with a non-NULL deployer |
| `nfts_pending.minted_at_ledger`                        | `min(ledger_sequence)` over `nft_ownership_pending WHERE event_type = 0`                     |

Two traps 0528 hit, both certain to recur here:

- `min()` over a non-Nullable column returns a **non-Nullable** type, and without
  `join_use_nulls = 1` (unavailable, `api_reader` is readonly) a LEFT JOIN miss
  fills the type DEFAULT rather than NULL. Wrap in `nullIf(_, 0)` or the endpoint
  500s on decode and a missing value renders as "ledger 0".
- Where the column is a sort key or cursor key, the ORDER BY, the keyset
  predicate and the cursor payload must move together or pagination stops being
  total.

**Cost is the open question, not correctness.** `nfts` was cheap because
`nft_ownership` is 23 k rows. `transaction_participants` is 10.73 B and
`operations_appearances` is 6.85 B, and neither sorts in a way that makes a
per-page aggregate free. The account list is a hot path. Expect to need a
page-scoped derivation (aggregate only the ids on the current page, as the
existing enrichment CTEs already do), a materialised view, or an accepted
`repair-tier1` cadence for those two — decide per column, with a measurement.

## Acceptance Criteria

- [ ] Each of the five remaining columns is either derived at read time, or has a
      written decision recording why derivation is not affordable and what
      replaces it
- [ ] `accounts` list and detail measured before/after — no page-latency
      regression on the hot path
- [ ] Keyset pagination proven total wherever a derived value is a cursor key
- [ ] Prod measurement before and after, per column, including a re-measure of
      the `accounts` sample
- [ ] `repair-tier1`'s doc comment and task 0228 corrected: the defect is
      live-ingest, not backfill-only
- [ ] 0322 item (G) closed or updated to point here
- [ ] **Docs updated** — the endpoint-query files for every touched endpoint
- [ ] **API types regenerated** — expected `N/A` (sources change, wire shape does
      not); confirm with an actual empty diff

## Notes

- Deriving every column removes the reason for the `nfts`/`nfts_pending` half of
  `repair-tier1` entirely, and shrinks the rest. Retiring those subcommands is a
  natural follow-up once the readers are gone.
- Do not treat this as urgent-and-total. `accounts` is the one with real user
  exposure and real cost risk; the small tables are nearly free. Sequence by
  exposure, and measure before committing to an approach on the two big fact
  tables.
