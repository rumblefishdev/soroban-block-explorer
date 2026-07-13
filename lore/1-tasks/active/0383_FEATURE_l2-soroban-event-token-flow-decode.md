---
id: '0383'
title: 'L2: Soroban event token-flow decode (from/to/amount + event participants)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0359']
tags: [priority-high, effort-large, layer-indexer, soroban-events]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker (§15 roadmap B). Bundles K1-3, K1-7, K2-7, K3-4, K4-3, K4-4.'
---

# L2: Soroban event token-flow decode

## Summary

Decode the actual token movements inside Soroban contracts (transfer / mint /
burn / clawback: from, to, amount, asset) from `soroban_events` and surface them
on account + asset pages. The classic-op fan-out (0359) covers classic
operations; this is the Soroban-event side.

## Context

Spawned from 0359. Verified (0359 §16): the raw event content is **already in
CH** — `soroban_events.topics_xdr` / `data_xdr` hold ScVal-decoded JSON (9.68B
rows, prod). So this is a **CH-side transform, NOT an S3 re-parse**. The decode
for `transfer` already exists (`event_filters.rs`); mint/burn/clawback are the
same SEP-41 topic shape (one address each).

## Implementation

- Add mint/burn/clawback shape parsers in `xdr-parser/src/event_filters.rs`
  (mirror `parse_transfer`; mint=[mint,to], burn=[burn,from], clawback=[clawback,from]).
- **K2-7** — register mint/burn/clawback from/to as `transaction_participants`
  (extend the event loop in `stage.rs`, today transfer-only).
- **K1-3** — decode from/to/amount/asset into a queryable form (target model:
  token-transfers table, or into `operation_asset_appearances` for SAC-wrapped
  classic assets). **Model decision required.**
- **K1-7** — the `soroban_events` RMT key excludes payload; confirm no distinct-
  event loss (0359 argued `event_index` is monotonic-unique → safe) or fix the key.
- **K3-4** — union decoded transfers into account/asset activity pages.
- **K4-3 / K4-4** — event `amount` hygiene; `invocations.amount` is a fold-count
  not a token value.

## Acceptance Criteria

- [ ] mint/burn/clawback participants registered (K2-7)
- [ ] token from/to/amount decoded + queryable (K1-3), model decided
- [ ] K1-7 loss risk confirmed-absent or key fixed
- [ ] account/asset pages union Soroban transfers (K3-4)
- [ ] no S3 re-parse required (CH transform over existing `soroban_events`)
