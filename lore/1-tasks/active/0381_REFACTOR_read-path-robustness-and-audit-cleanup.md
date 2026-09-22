---
id: '0381'
title: 'Read-path robustness + architecture-audit cleanup (poison-pill, overscan, dead index/dictionary)'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0359', '0541']
tags: [priority-medium, effort-large, layer-api, robustness]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker §11 (G-architecture-audit MAJOR/MINOR items not otherwise covered).'
  - date: 2026-07-13
    status: active
    who: karolkow
    note: 'Promoted to active to begin implementation.'
---

# Read-path robustness + architecture-audit cleanup

## Summary

Address the read-path robustness + cleanup items from the 0359 architecture audit
(G-architecture-audit) not covered by the other spawned tasks.

## Context

Spawned from 0359 §11. These are query-engine robustness + dead-weight cleanup
items surfaced by the audit; independent of the write-side re-model.

## Implementation

**MAJOR (read robustness):**

- Poison-pill quarantine — a single bad row/partition shouldn't fail a whole read.
- Partition-pinned filtered global lists — global lists that pin to one partition
  miss cross-partition results.
- Overscan ×4 without refill — over-fetch factor never refills to fill a page.

**MINOR (cleanup / small perf):**

- `ledgers` `LIMIT 1 BY` read-in-order check.
- Cursor-to-filter binding.
- Dead dictionary + `idx_tx_hash_bloom` removal.
- Muxed-id dropped in details JSON (preserve the muxed memo-id).
- Sibling-wildcard canary tests for `emit_asset_appearances` /
  `extract_counterparties` / `claim_atoms` (guard against a silent `_` regression).

## Measured instances (task 0541 review, 2026-09-21)

The review of 0541 swept the lists for the short-page defect: a page that comes
back under `limit` for any reason but the end of the data reads as "no next
page", because the envelope infers the end from the row count. Measured on
production 2026-09-20.

| list                                                                                                          | cause                                                                                                      | reach                                                                                                                                                              |
| ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| transactions, every filtered variant                                                                          | pages inside one partition (canonical SQL 02)                                                              | a list ends at a partition boundary, every 500,000 ledgers — and starts empty for a filter quiet in the head's partition; contract filter fixed 2026-09-22 (below) |
| liquidity-pool participants (`liquidity_pools/queries.rs`)                                                    | a row dropped for a dangling account surrogate takes the `limit + 1` sentinel with it                      | 82 of 26,489 pools hold more than one page, the largest 684; only when a surrogate dangles                                                                         |
| assets (`assets/queries.rs`, `SEEK_OVERFETCH`)                                                                | over-fetch, then version dedup; warns and still returns short                                              | latent: at most 6 versions per key                                                                                                                                 |
| ledgers (`ledgers/queries.rs`, `LEDGER_OVERFETCH`)                                                            | over-fetch ×3, no guard                                                                                    | latent: at most 1 row per sequence                                                                                                                                 |
| transactions of one contract, with a source or operation-type filter (`transactions/queries.rs`, Statement B) | takes `limit × 4` positions from the index, then filters; fewer than `limit + 1` survivors read as the end | any contract whose matching transactions are sparse among its own; same on `develop` before 0541                                                                   |

0541 fixed the same defect in the contract-filtered transaction list (804
transactions, 13 shown, no next page) by giving contracts a presence index,
`contract_transactions`, like the ones accounts, assets and pools have. All four
entities can now seek a transaction list across partitions through an index.

The rule the fix followed: a bounded search that stops before exhausting its
input hands its bound to the caller; the envelope never infers "end of list"
from a count the search itself capped.

## Contract list and contract events page (2026-09-22)

Found reviewing PR #465's query diffs. Branch `fix/0381_exact-contract-pages`.

**Partition pin — worse than "ends at a boundary".** The first page is pinned to
the head's partition too, so a filtered list starts empty for anything quiet
there. On 2026-09-22 the head was ~3.7 days into partition 129:

- **Contract filter** — 10,138 of 152,191 contracts in `soroban_contracts`
  (6.7%) had a transaction in 129; the other 93.3% listed as empty. (The
  index holds 466,104 contract ids; the rest are the asset contracts of
  classic assets, whose transfers carry events since protocol 23 — e.g.
  `RLUSD`.) **Fixed:** Statement B seeks `contract_transactions` across
  partitions. Measured through the API against production: the native SAC's
  driver reads 28–31M rows in 117–137 ms, a contract quiet since partition
  128 (`CCSNFZ5R…`) 1.8M rows in 36–57 ms and now shows full pages, 3 pages
  checked, in order, no repeats. A page crosses the boundary: `CBTTQNQ7…`
  (3 transactions in 129) gets 3 rows from 129 and 17 from 128 and a next
  page, where the pinned read gave 3 rows and the end.
- **Operation-type filter** — not fixed (decided: record). In 129,
  `RESTORE_FOOTPRINT` had 0 operations (126 in 128), `REVOKE_SPONSORSHIP` 10,
  `EXTEND_FOOTPRINT_TTL` 19: an empty or short first page read as the end.
  More types just after a rollover. `operations_appearances` has no type in its
  key, so crossing partitions is a scan: one partition for a rare type read
  244M rows (466 MiB, 98 ms), all ~30 ≈ 7 bn rows per page (_estimate_). The
  fix needs an index — a `type` skip index, to be measured on one partition.

**`LIMIT 1 BY` beside a heavy payload.** `LIMIT 1 BY` walks every row it
dedups; in one statement with `topics_xdr` / `data_xdr` it reads the payload
for all of them. The native SAC's contract-events first page:

| read                                           | time       | read bytes    | memory        |
| ---------------------------------------------- | ---------- | ------------- | ------------- |
| before (one statement)                         | 431–577 ms | 1.19–1.24 GiB | 2.21–2.29 GiB |
| plain `LIMIT` ×2, dedup in Rust (not taken)    | 97–98 ms   | 222–225 MiB   | 381–385 MiB   |
| **keys with `LIMIT 1 BY`, then payload (now)** | 144–178 ms | 525–589 MiB   | 511–549 MiB   |

The over-fetch variant was rejected: when duplicates outnumber the margin the
page comes back short and reads as the last one. The chosen form stays exact.
The code comment that claimed the read "short-circuits at `LIMIT`" was false and
is corrected. House precedents: `ledgers/queries.rs` (over-fetch, measured),
the account list (keys first), `liquidity_pools/queries.rs` (PR #335, reverted:
`LIMIT 1 BY` is not a seek).

**Still open from the same review:**

- Contract list with a source or operation-type filter: `limit × 4` positions,
  then filters; fewer than `limit + 1` survivors read as the end (row above).
- `fetch_events` step 2: an event whose transaction is not found renders as
  failed, with an empty hash, dated 1970 (`unwrap_or_default()`); impossible
  after the swap by construction, but a misleading fallback.
- `EventId` built with `expect` on the ledger fitting u32 in the API path.
- Two skip indexes lost their reader: `idx_oa_contract_id` (12.60 MiB; the
  contract list reads `contract_transactions` since 0541) and `idx_oa_pool_ids`
  (348.67 MiB; the pool list reads `operation_pools`). No crate filters
  `operations_appearances` by either column — `fetch_operations` only projects
  them, `repair_tier1` uses `notEmpty(pool_ids)`, which a bloom filter does not
  serve. Drop both, like `idx_oa_asset_issuer_id` before them (operator).
- **Transaction search by account is partition-pinned too.** A source filter
  without contract or operation type runs Statement A, pinned to the head's
  partition — the transactions page's search box with a `G…` address. Of the
  344,060 accounts that sent a transaction in partition 128, 71,544 (20.8%)
  sent one in 129; the other 79.2% get an empty first page (2026-09-22). The
  account page itself reads `transaction_participants` and is not pinned. The
  fix needs a product decision first: the search lists what the account
  **sent** today; the account page lists every transaction it **takes part
  in**.
- **Asset contracts of classic assets cannot be searched.**
  `resolve_contract_surrogate` looks the address up in `soroban_contracts`
  only; 313,913 of the 466,104 contract ids in `contract_transactions` have no
  row there (e.g. the `RLUSD` asset contract, 122k transactions in partition
  128), so their list comes back empty. Same root: `fetch_event_appearances`
  renders such a contract with an empty address (`unwrap_or_default()`, the
  archive-unavailable fallback only). From the code and the id count; not
  exercised through the API.

## Dedup and filtered pages without over-fetch — options (2026-09-22)

Decided: not now; this is the target when the over-fetch items are fixed.

**Measured on production (26.3), first page, native SAC:**

| read                                   | `LIMIT 1 BY`            | `FINAL` + `LIMIT`              |
| -------------------------------------- | ----------------------- | ------------------------------ |
| contract list, `contract_transactions` | 25–27M rows, 108–149 ms | 230M rows, 3.86 GiB, 1.1–1.2 s |
| same, a contract quiet since 128       | 1.8M rows, 24–36 ms     | 1.8M rows, 30–33 ms            |
| contract events page                   | 144–178 ms (keys first) | timeout, over 30 s             |

**ClickHouse, per its source, PRs and docs** (research, 2026-09-22):

- `FINAL` is ClickHouse's own answer for exact reads of a ReplacingMergeTree,
  and 26.3 has its speed-ups on (range splitting, vertical `FINAL`, skip
  indexes under `FINAL`). But on 26.3 a descending `FINAL … ORDER BY key DESC
LIMIT n` does not read in order: it reads every matching range and sorts —
  the 230M above. Fixed in 26.9 (`optimize_read_in_reverse_order_final`,
  default on, ReplacingMergeTree only; not in the 26.8 LTS). Lazy reading of
  heavy columns under `FINAL` arrives in 26.4.
- `LIMIT 1 BY` does not stop the read early on any released version (upstream
  issue #113110 reproduces our pattern: 29.5M rows against 274k; the fix,
  PR #113565, is open).
- A filter on a non-key column with `ORDER BY key LIMIT n` stops as soon as
  `n` rows pass it, and heavy columns are read only for those rows — when
  nothing else blocks the early stop.
- Skip indexes help rare values only (`set` or `bloom_filter`); measured here:
  `RESTORE_FOOTPRINT` sits in 14 of partition 128's 29,782 granules,
  `REVOKE_SPONSORSHIP` in 152.
- Recommended shape for "list by X, filtered, newest first": the filter
  columns in the table the list is ordered by (or a second table / view).
  Projections are never used with `FINAL`.

**Target design:**

1. Upgrade to ClickHouse ≥ 26.9 and re-measure `FINAL` on the descending
   pages. If it reads in order, `FINAL` replaces both `LIMIT 1 BY` and every
   over-fetch (lists, assets, ledgers): exact pages, early stop.
2. Operation-type filter: a `type` skip index on `operations_appearances`, then
   drop the partition pin — to be measured on one partition first.
3. Contract list with a second filter: `source_id` and an operation-type mask
   on `contract_transactions`, so the filter runs inside the seek (schema
   change and refill of ~3 bn rows).

## Acceptance Criteria

- [ ] poison-pill quarantine · partition-pinned lists · overscan-refill fixed
- [ ] MINOR cleanup items resolved or explicitly deferred with reason
