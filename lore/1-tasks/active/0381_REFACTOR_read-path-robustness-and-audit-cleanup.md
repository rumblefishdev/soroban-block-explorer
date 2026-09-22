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

## Acceptance Criteria

- [ ] poison-pill quarantine · partition-pinned lists · overscan-refill fixed
- [ ] MINOR cleanup items resolved or explicitly deferred with reason
