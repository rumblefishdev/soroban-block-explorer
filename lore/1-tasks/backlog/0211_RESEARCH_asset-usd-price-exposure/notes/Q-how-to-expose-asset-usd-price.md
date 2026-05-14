---
title: 'How should the block explorer expose per-asset USD price?'
type: question
status: seed
spawns: []
tags: [api, enrichment, off-chain, fast-change]
links: []
history:
  - date: '2026-05-12'
    status: seed
    who: karolkow
    note: 'Question created. Root question for task 0211.'
---

# How should the block explorer expose per-asset USD price?

## Context

USD price is the only off-chain field surfaced to the API whose value changes
**per-second**. Every other off-chain field the explorer carries today is
rare-change per row (SEP-1 TOML — months / years; NFT metadata — immutable
once minted; LP TVL — captured _as of a ledger boundary_ and frozen).

ADR 0043 codified the off-chain handling rule under the implicit assumption
that off-chain = rare-change. The rule maps off-chain list-endpoint fields to
"Lambda 2 typed column" and off-chain detail-only fields to "runtime type-2
fetch". Neither path serves a fast-change value cleanly:

- **Typed column** — value is stale within seconds of write. Janitor refresh
  pumps stale values into the table at a chosen cadence; sortable but
  semantically wrong.
- **Runtime type-2 fetch (detail only)** — fresh but forbidden by ADR 0043
  on list endpoints (N rows = N HTTP fetches per request).

The third path — _batched runtime fetch_ (one HTTP call returning N prices,
merged into the list response) — is not part of ADR 0043. It would require
either an amendment or a dedicated "markets aggregation" endpoint that lives
outside the list/detail dichotomy.

## What Would Answer This

A picked design (one of A/B/C/D in the parent README's table) plus:

1. Justification grounded in Oskar's API capabilities (batched? latency?
   rate limit?).
2. A cache strategy (TTL, eviction, cold-start handling).
3. Failure handling (Oskar 5xx, timeout, no-price-for-asset).
4. An ADR 0043 amendment sketch that names the "fast-change off-chain"
   category and assigns it a path.

## Why Now

Four parked attempts to add `assets.usd_price` to the schema. Each blocked on
"no consumer" or "no product ask"; the deeper block was a missing pattern.
Forcing the design discussion here prevents a fifth ad-hoc attempt.
