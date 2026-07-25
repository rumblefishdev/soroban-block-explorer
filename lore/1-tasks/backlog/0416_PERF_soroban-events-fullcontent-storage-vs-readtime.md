---
id: '0416'
title: 'PERF/COST: re-evaluate full-content soroban_events storage (223 GiB, #1 table) vs read-time decode — ADR 0044 §4a / deferred Q6'
type: RESEARCH
status: backlog
related_adr: ['0044', '0033', '0029']
related_tasks: ['0415']
tags:
  [
    'clickhouse',
    'storage',
    'cost',
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
    note: 'Spawned while auditing 0415. soroban_events is the #1 largest table (223 GiB compressed / 2.66 TiB uncompressed / 9.8B rows); topics_xdr alone is 149 GiB. ADR 0044 §4a stored full content DELIBERATELY (drop the S3 hop for the events tab); ADR 0044 Q6 (storage-ratio success criterion) was left DEFERRED. Re-measure that tradeoff now with real numbers.'
---

# PERF/COST: re-evaluate full-content soroban_events storage vs read-time decode

## Summary

`soroban_events` is the **single largest table in the DB** — measured on prod
2026-07-20: **223 GiB compressed / 2.66 TiB uncompressed / 9.8 billion rows**,
bigger than `transactions` itself. The bulk is two display columns:
`topics_xdr` = **149 GiB**, `data_xdr` = **15 GiB** (~165 GiB of event _content_);
the rest (~58 GiB) is the appearance-index columns.

This was a **deliberate** design choice (ADR 0044 §4a), not an accident — but its
success criterion (storage ratio, Q6) was **explicitly deferred** at the time. Now
that we have the real number, re-evaluate whether full-content storage is still the
right tradeoff, or whether some of that ~165 GiB can be reclaimed.

## The biggest single lever, found 2026-07-21: ~59% of rows are `fee` events

Measured directly on prod (ledgers 63,578,050–63,578,074, `signature` column):

| signature       |       rows |     share |
| --------------- | ---------: | --------: |
| **`fee`**       | **17,802** | **58.7%** |
| `transfer`      |      7,800 |     25.7% |
| `mint`          |      3,606 |     11.9% |
| `burn`          |        962 |      3.2% |
| everything else |       ~120 |     <0.5% |

`fee` events are the CAP-67 protocol-generated fee charge/refund, emitted for
**every** transaction on the network and attributed to the native asset's SAC
address — they are not contract-authored content anyone browses. So the majority
of the #1 table is protocol bookkeeping, and evaluating the levers below without
splitting `fee` out first will mis-price every one of them.

Evaluate explicitly: do `fee` events need `topics_xdr`/`data_xdr` stored at all,
or only an aggregate/derived form? Re-run the share over a wider window before
committing — one 25-ledger window is a sample, not a distribution.

**Related read-path consequence (worth fixing regardless of the storage decision):**
because every fee event is attributed to the native SAC contract, that contract's
detail page unions a `soroban_events` arm keyed on `contract_id` — so its
transaction list is effectively _every transaction on Stellar_, and its
`recent_events` stat is a network-wide counter. Reported by the audit agent;
**not independently re-verified** — confirm before acting.

## Critical constraint (measured 2026-07-20)

**The CH `transactions` table does NOT store `result_meta_xdr`** (columns: hash,
id, inner_tx_hash, source_id, fee_charged, successful, operation_count,
application_order, ledger_sequence, has_soroban, parse_error — no XDR blobs). So
event content is **not re-derivable CH-locally** — re-decoding events means the
**S3 archive round-trip** ADR 0033 had and ADR 0044 removed. Dropping
`topics_xdr`/`data_xdr` therefore **re-introduces that latency on the events tab**;
it is not a free win. (My earlier "dedup vs stored meta" hypothesis was WRONG —
the meta is not in CH.)

## Levers to evaluate (ranked safest → most invasive)

1. **Codec / ordering re-tune (no content dropped, lowest risk).** `topics_xdr`
   compresses only ~12× vs the expected 20–40×. Try ZSTD higher level, and/or an
   ORDER BY that groups similar topic shapes for better locality. Could reclaim a
   chunk of 149 GiB with zero read-path change.
2. **TTL / retention.** Do we need all 9.8B events' full content forever? A TTL
   that drops `topics_xdr`/`data_xdr` (keeping the appearance index) past age N
   reverts old events to the ADR 0033 read-time-from-S3 path, keeps recent ones
   fast. Bounds the table's growth.
3. **The fat `transaction_id` column (47 GiB).** 9.8B unsorted Int64 — check
   whether ORDER BY / delta-coding shrinks it.
4. **Revert §4a fully → appearances + read-time S3 decode.** Frees ~165 GiB but
   restores the S3 hop on every events-tab render. Only if the events tab proves
   low-traffic enough that the latency is acceptable — **measure events-tab QPS
   first**.

## Acceptance Criteria

- [ ] Per-column + per-partition storage breakdown, and events-tab (`/contracts/
{id}/events`) real QPS / latency budget — the missing half of ADR 0044 Q6.
- [ ] A decision (new ADR or amend 0044): keep full-content, tune codec/TTL, or
      revert to appearances — with the storage-vs-latency numbers behind it.
- [ ] If any content is dropped, the events-tab read path is confirmed to degrade
      gracefully to read-time decode (S3) and the UX cost is quantified.
- [ ] No conflation with 0415: this is storage/cost, not fact-trust.
