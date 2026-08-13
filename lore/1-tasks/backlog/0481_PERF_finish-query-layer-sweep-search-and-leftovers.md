---
id: '0481'
title: 'PERF: finish the query-layer sweep — search SQL merges + 0446 leftovers'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0446', '0402']
tags: [phase-future, effort-medium, priority-low, performance, api, clickhouse]
links: []
history:
  - date: '2026-08-13'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0446 review (three review agents + audit). Everything 0446
      could win with `tokio::join!` is done; what remains needs SQL rewrites or
      sits outside 0446's scope, and none of it was urgent enough to keep that
      PR open longer while develop kept landing conflicts into the same files.
---

# Finish the query-layer sweep — search SQL merges + 0446 leftovers

## Summary

Task 0446 removed every serial wave that `tokio::join!` could remove. Four
items survived its review with a verdict of "real, but a different class of
change": the search buckets' internal phase chains (SQL merges, not
concurrency), a third hand-rolled copy of the shared account resolver, one
parallelisable pair 0446's sweep missed, and a `try_join!` holdout that
predates the join-everywhere convention.

## Context — why 0446 did not do this

Search's six buckets already run concurrently (`tokio::try_join!` in
`search::fetch_search`). The serial steps left inside buckets are
phase-1 → phase-2 chains where phase 2's `WHERE` clause IS phase 1's output
(contract-id scan → name lookup; asset rows → issuer resolve). Nothing to
overlap — the only lever is merging phases into one statement (subquery, with
the Rust-side dedup moved into CH), which is the same surgery as 0446's assets
`UNION ALL`: each rewrite needs a production differential check and a
`decode_smoke` guard.

The expected gain is also bounded by the critical path: endpoint latency =
max over buckets, so collapsing a chain inside bucket X only pays when X is
the slowest bucket for that query. ≤ one wave (~14 ms) on some queries, zero
on others. Do it for the cleanup value as much as the latency.

## Work list

- [ ] `search::search_contracts` (`crates/api/src/search/queries.rs`) —
      prefix-scan phase + bounded name-lookup phase → one statement. The
      Rust-side adjacent-duplicate collapse between the phases must move into
      the SQL with it.
- [ ] `search::search_assets` — phase-2 issuer resolve is a
      character-identical copy of `common::ch::resolve_accounts`
      (`IssuerRow` struct + hand-rolled empty-guard, ~20 lines). Replace with
      the shared helper — third such copy found; the first two fell in 0446.
      This is reuse, not a merge; independent of the phase-merge above.
- [ ] `transactions::fetch_list` — when BOTH `?source_account=` and
      `?contract_id=` filters are set, `resolve_account_surrogate` and
      `resolve_contract_surrogate` are awaited serially though they are
      independent (validated separately in the handler). Pair them; the
      early-return on "filter resolves to nothing" moves after the join.
- [ ] `search::fetch_search` — the crate's one remaining `try_join!`. The
      join-everywhere convention (0446) would swap it, BUT all six bucket
      failures are equivalent here (any bucket error = 500), which is exactly
      the case where `try_join!`'s early return is harmless. Decide and either
      swap for uniformity or document why it stays; do not leave it implicit.
- [ ] Each SQL merge verified against production the way 0446's union was:
      differential vs the two-phase result with the ledger fence PINNED to a
      literal (live-chain `max(sequence)` movement fakes mismatches), plus a
      `decode_smoke` case.

## Acceptance Criteria

- [ ] No serial wave remains in `crates/api/src/**/queries.rs` whose two sides
      are independent — verified by re-running 0446's audit method (read every
      handler → query fn → helper)
- [ ] No hand-rolled copy of `resolve_accounts` / `resolve_contracts` remains
      (grep for `LIMIT 1 BY id` outside `common/ch.rs`)
- [ ] Response payloads unchanged; existing tests green without modification

## What this is NOT

Not the txdetail transport investigation (0402 — handshake attribution,
connection reuse), and not a rerun of 0446's load-test AC. Both stay where
they are.
