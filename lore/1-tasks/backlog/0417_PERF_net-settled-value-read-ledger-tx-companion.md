---
id: '0417'
title: 'PERF: (ledger,tx)-leading companion for the net-settled value read — turn the scan into a seek (0393 E release-gate)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0393', '0411', '0365', '0357']
tags:
  [
    'clickhouse',
    'api',
    'perf',
    'phase-future',
    'effort-medium',
    'priority-medium',
  ]
links: []
history:
  - date: 2026-07-20
    status: backlog
    who: karolkow
    note: 'Spawned from 0393. The value read (fetch_tx_list_aggregates) SCANS the asset_id-leading operation_asset_appearances (~26M rows/page); the global tx list is polled and this endpoint family blew the read quota before (0243/0386). This is the mitigation named in the 0393 E release-gate — build the (ledger,tx)-leading companion so the read is a prefix seek. (0393 F — the tx-value-query relocation out of common/ch.rs — is owned by 0411; do it wherever the read is restructured first.)'
---

# PERF: (ledger,tx)-leading companion for the net-settled value read

## Summary

`fetch_tx_list_aggregates` (`crates/api/src/common/ch.rs`) reads net-settled
`values` for the global tx list by filtering `operation_asset_appearances` on
`(ledger_sequence, transaction_id) IN (...)`. That table is **`asset_id`-leading**,
so the filter is **not** a prefix seek — it SCANS the pruned partition (~26M
rows/page measured, vs ~16k for the op-types seek beside it), plus three un-pruned
dimension joins. This is the **global tx list** — polled — and this endpoint family
has exhausted the CH read quota before (0243/0386). Task 0393 shipped a `bloom`
skip index (`idx_oaa_transaction_id`, ~10× prune) and gated the read behind a
**release gate** (0393 README): do not expose it on prod polling until this
companion lands or a mature-partition load test clears it.

## Implementation

- Build a `(ledger_sequence, transaction_id)`-leading companion (accounts_recent /
  operation_pools pattern, cf. 0365) carrying `(ledger, tx, asset_id, net_settled)`
  so the value read is a **prefix seek**, not a partition scan. Indexer-written
  (deterministic, re-derivable by a CH re-key like 0365 Path B).
- Point `fetch_tx_list_aggregates`' value read at the companion; confirm the
  dimension joins (`assets` / `soroban_contracts` / `soroban_contract_metadata`)
  stay bounded.
- If restructuring the value read here, that is also the natural moment to do the
  **0393 F relocation** (move the tx-value query out of shared `common/ch.rs` into the
  transactions domain) — but F is **owned by task 0411**; do it wherever the read is
  restructured first.
- Re-enable the value column on the account / ledger lists (0411) once the read is a
  seek, if product wants it there.

## Acceptance Criteria

- [ ] Value read for the global tx list is a prefix seek on a `(ledger,tx)`-leading
      companion (read_rows on the order of the op-types seek, not ~26M/page).
- [ ] Companion is indexer-written + backfillable by a CH re-key (no S3 re-parse).
- [ ] 0393 E release-gate cleared — the value read is safe under prod polling.
- [ ] Load-tested on a mature (full-size) partition before prod exposure.
