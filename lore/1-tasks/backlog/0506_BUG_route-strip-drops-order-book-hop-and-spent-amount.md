---
id: '0506'
title: 'BUG: the route strip drops the order-book hop amount and the amount actually spent'
type: BUG
status: backlog
related_adr: ['0029']
related_tasks: ['0453', '0457', '0261']
tags: [xdr-parser, frontend, transaction-detail, priority-medium, effort-small]
links: []
history:
  - date: '2026-08-19'
    status: backlog
    who: karolkow
    note: >
      Spawned from a live path-payment investigation (tx
      be36ded71f378471b95bc0e6989049681237a2d88c3bee5b4d9d5894098e54fe,
      ledger 64,010,777). Narrow slice of 0457's first acceptance
      criterion, reachable WITHOUT effects-from-meta: the read path
      re-parses the archive ledger per request, so a parser change alone
      lights up the whole indexed history with no backfill.
---

# BUG: the route strip drops the order-book hop amount and the amount actually spent

## Summary

A multi-hop path payment renders a route strip whose numbers are incomplete
in two independent ways: hops that filled against the order book carry no
amount at all, and the amount the operation actually SPENT is never shown.
Both numbers are already present in the data the page receives — one is
discarded by the parser, the other is on the wire and unread by the
frontend. Neither needs new storage, a migration, or a backfill.

## Context

Chain-verified example (decoded from `resultXdr` with the official CLI):
a self-directed `PATH_PAYMENT_STRICT_RECEIVE` cycle
`XLM → KALE → yUSDC → USDC → HU → 6T → XLM`, six fills, five against
liquidity pools and one against the order book.

| Symptom        | What the page shows                                    | Truth from the ledger            |
| -------------- | ------------------------------------------------------ | -------------------------------- |
| order-book hop | `USDC` chip, no amount + a footnote explaining the gap | 0.0176869 USDC, offer 1853744423 |
| amount spent   | nothing — first chip is a bare `XLM`                   | 0.1156719 XLM                    |

The second one carries the meaning of the whole transaction: spent
0.1156719 XLM, received 0.1162018 XLM. Without it the strip reads as an
inexplicable "XLM for XLM" and the outcome (+0.0005299 XLM, fee 100
stroops) is invisible.

### Why both are cheap

The detail page's `details` block is NOT read from ClickHouse. Per ADR 0029
the handler fetches the parent ledger from the public archive at request
time ([`handlers.rs:388`](../../../crates/api/src/transactions/handlers.rs)
`compute_heavy`) and re-parses it
([`extractors.rs:99`](../../../crates/api/src/runtime_enrichment/stellar_archive/extractors.rs)
→ `xdr_parser::extract_operations`). A parser change therefore applies to
every transaction the archive covers, immediately, with no re-parse of our
own and no schema change. Responses are cached 300 s.

### Root cause, hop amount

[`operation.rs`](../../../crates/xdr-parser/src/operation.rs)
`append_pool_claims` walks `claim_lp_atoms` — liquidity-pool fills only.
The generic extractor `claim_atoms` already exists in the same file
(all three `ClaimAtom` arms: `V0`, `OrderBook`, `LiquidityPool`) and is
already used by `extract_counterparties`, so the order-book fill is parsed
and then thrown away one call later.

### Root cause, amount spent

`claimedAtoms` already carries `amountBought` per atom. `RouteStrip.tsx`
labels each edge with `amountSold` only, so every hop shows its OUTPUT and
the input side of the first hop has nowhere to appear.

## Implementation Plan

### Step 1: keep every claim atom, not just the pool ones

In `append_pool_claims`, iterate `claim_atoms` and branch:

- `LiquidityPool` → unchanged (`poolId`, `amountA`, both asset/amount pairs);
- `OrderBook` / `V0` → `assetSold`/`amountSold`/`assetBought`/`amountBought`
  plus `sellerId` and `offerId`; **no `poolId`, no `amountA`** — those two
  keys are what the pool-aggregation consumers key on.
- `poolIds` keeps coming from the liquidity-pool arm alone.

`V0`'s raw ed25519 seller is converted the same way `op_participants.rs`
already does it.

### Step 2: prove the pool aggregates are untouched

Both writers skip atoms without a pool id by construction —
`gross_volume_a_by_pool` requires `poolId` + `amountA`, and
`pool_fill_amounts` goes through `atom_pool_id`
([`stage.rs`](../../../crates/db-clickhouse/src/persist/stage.rs)). That is
the one way this change could silently corrupt liquidity-pool figures, so
it gets an explicit test rather than an argument.

Three existing parser tests encode the old behaviour and flip:
the mixed-route atom count, `order_book_only_path_payment_has_no_pool_claims`
(claims stop being absent; the pool-id assertion stays true, the name does
not), and the manage-buy-offer atom count.

### Step 3: show the amount spent

`RouteStrip` renders the first hop's `amountBought` on the leading chip (or
as a leading edge label). Applies to both strict-send and strict-receive.
Degraded/failed operations keep showing nothing rather than a guess.

### Step 4: retire the footnote when it stops being true

The "hops without an amount crossed the order book" note stays for genuinely
degraded responses, but `partial` must no longer trip on a route that is now
fully covered.

## Acceptance Criteria

- [ ] The reference transaction shows 0.0176869 USDC on the order-book hop
- [ ] The same operation shows 0.1156719 XLM as the amount spent, so the
      cycle's outcome is readable from the strip alone
- [ ] A test proves an order-book atom produces no liquidity-pool rows and
      no `gross_volume_a` contribution
- [ ] `poolIds` still lists pools only
- [ ] The order-book footnote no longer appears on a fully-covered route
- [ ] **Docs updated** — `N/A` — no change to the shape of the system: no
      endpoint, schema, or pipeline step changes; the payload gains keys
      inside an already-`unknown` `details` blob.
- [ ] **API types regenerated** — `N/A` — nothing under `crates/api/**`,
      `Cargo.*` or `libs/api-types/**` changes; `details` is `unknown` on
      the wire.

## Notes

- Overlaps 0457's first acceptance criterion (order-book hop amount) but
  reaches it by a different route: 0457 derives effects from
  `ledger_entry_changes`, this task keeps atoms the parser already decodes.
  Whichever lands first should tick that criterion in the other.
- 0453 deferred this deliberately as spec point D9 ("order-book segments are
  LP-invisible") — that premise was true of `claimedAtoms`, not of the XDR.
- Out of scope: order-book depth or offer state over time. That needs offer
  entries persisted as their own table plus history, and is a much larger
  piece of work — the fills here come from each transaction's own result.
