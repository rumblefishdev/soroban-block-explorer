---
id: '0549'
title: "FEATURE: store the fee payer on transactions — the index cannot attribute a fee bump's fee"
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0540', '0359', '0514']
tags:
  [
    priority-medium,
    effort-medium,
    layer-indexer,
    layer-xdr-parsing,
    data-integrity,
  ]
links: []
history:
  - date: 2026-09-13
    status: backlog
    who: karolkow
    note: >
      Found by the 0540 account reconciliation (gate 7c): an account's XLM only
      closed against ledger state when the fee leg came from the protocol's fee
      events, because transactions records the inner source and no payer.
      Measured the share of fee-bumped transactions in two windows.
---

# Store the fee payer on transactions

## Summary

For a fee-bump transaction the fee is paid by the envelope's `fee_source`, not by
the inner transaction's source. `transactions` stores only the inner source
(`source_id`) and the fee (`fee_charged`), so nothing in the index says who paid.
Any per-account fee figure read from ClickHouse charges a sponsor's payment to the
sponsored account and misses a self-bump. Fee bumps are **37.6%** of transactions
in a recent window and growing.

## What exists today (traced, not assumed)

- **Parser:** `ExtractedTransaction.fee_source` is extracted for every fee-bump
  envelope (`xdr_parser::envelope::envelope_fee_source`, `transaction.rs`).
- **Its only consumer:** `stage.rs` registers the payer in
  `transaction_participants` (task 0359 K2-4), so the transaction appears on the
  payer's account page — but a participant row carries no role, so the index
  knows the payer took part, not that it paid.
- **`transactions.source_id`** is `envelope_source`, the inner principal.
- **Transaction detail page** already shows the payer ("Fee source"), read at
  request time from the envelope XDR (`heavy.fee_bump_source`). It is the only
  surface that can; the account transaction list shows `fee_charged` on every
  row, including rows the account did not pay for.

## Measured

| Window                          | Transactions | Fee-bumped | Share     |
| ------------------------------- | ------------ | ---------- | --------- |
| ledgers 55 000 000 – 55 099 999 | 36 652 165   | 1 082 062  | 2.95%     |
| ledgers 64 000 000 – 64 099 999 | 33 070 939   | 12 435 177 | **37.6%** |

Raw row counts (unmerged duplicates included); the share is what matters.

From the 0540 reconciliation of 20 accounts against raw ledger state:

- An account mixing own and fee-bumped transactions was **5 366 606 stroops**
  off when the fee leg was `Σ fee_charged WHERE source_id = A` over non-bumped
  transactions, and exact when it came from the protocol's `fee` events.
- For accounts with no fee-bumped transaction both methods agree exactly:
  `fee_charged` is already net of the Soroban refund.

## The canonical source

CAP-67 `fee` events, emitted by the protocol for every transaction (present on the
whole ingested range, including pre-Protocol-23 ledgers in the archive's V4 meta):
native SAC, topics `["fee", payer]`, data the amount, a refund as a negative
amount. They name the account that actually paid. Reading them per account is
correct but expensive — a `topics_xdr` scan exhausted the read quota's byte
budget within an hour during 0540 — so they are the verification source, not the
read path.

## Options

- **A. A `fee_source_id` column on `transactions`** (`Nullable(Int64)`, NULL when
  not fee-bumped), written from the `fee_source` the parser already extracts.
  Cheap to read. Needs a DEFAULT in the DDL (every ADD ships with one — the insert
  validation lesson of 0310/0548) and a historical fill.
- **B. A per-account fee fact table** from the `fee` events. Exact, including
  refunds, but a second table for one attribute.

## Acceptance Criteria

- [ ] The index answers "who paid this transaction's fee" for every transaction
      on the ingested range, historical rows included — not forward-only
- [ ] Verified against the protocol's `fee` events on a sample that includes
      sponsored, self-bumped and Soroban-refunded transactions
- [ ] Any surface showing a fee next to an account says whether that account paid it
- [ ] **Docs updated** — `docs/architecture/database-schema/**`,
      `indexing-pipeline/**` per ADR 0032
