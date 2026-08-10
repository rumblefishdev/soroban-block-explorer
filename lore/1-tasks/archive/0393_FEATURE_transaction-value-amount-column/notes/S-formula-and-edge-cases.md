---
prefix: S
title: Net-settled formula — derivation, alternatives rejected, edge cases
status: mature
---

# S — The formula and why it survives

## Definition

Per (transaction, asset):

```
delta[account] += amount   for the `to` of each transfer
delta[account] -= amount   for the `from` of each transfer
amount = max( Σ positive deltas , Σ negative deltas )
```

Plus three rules:

1. **`max` of both sides**, not just the positive side.
2. **Native XLM canonicalised to one `asset_id`** before grouping.
3. **`fee` events excluded**.

## Why net, not gross

A transaction can route the same value through intermediaries
(`A → B → C`, 100). Gross summation counts every hop (200); net counts the
value that actually changed ownership (100), because the pass-through account
`B` has delta 0 and drops out. This is verified below.

## Alternatives rejected

Let `Σ+` = sum of positive account deltas, `Σ−` = sum of negative account
deltas (magnitudes). All three candidates agree on a plain balanced transfer;
they diverge on the cases below (verified by a small model of transfers +
mint + burn).

### A) Gross (sum of transfer amounts)

Inflates on routing. `A→B→C 100` → 200. A 5-transfer routed example → 290 vs a
net of 180. The industry avoids exactly this double-count.

### B) `Σ+` only ("sum of who gained")

Misses burns / payments-to-issuer, where nobody gains:

| Case                       | `Σ+`      | `max(Σ+,Σ−)` |
| -------------------------- | --------- | ------------ |
| transfer 100               | 100       | 100          |
| chain A→B→C 100            | 100       | 100          |
| pure mint 100              | 100       | 100          |
| **pure burn / redeem 100** | **0** ❌  | **100** ✅   |
| **transfer 100 + burn 40** | **60** ❌ | **100** ✅   |
| **redeem 250 to issuer**   | **0** ❌  | **250** ✅   |

They agree on every normal transfer, chain, and mint; they diverge only on
burns / redeems, where `Σ+` under-counts (to 0 for a pure redeem).

### C) `Σ|delta|` (sum of absolute deltas)

Equals `Σ+ + Σ−`, i.e. it counts **both legs** of every transfer → doubles
normal transfers:

| Case                 | `max(Σ+,Σ−)` | `Σ\|delta\|` |
| -------------------- | ------------ | ------------ |
| transfer 100         | 100          | **200** ❌   |
| chain A→B→C 100      | 100          | **200** ❌   |
| 5-transfer routed    | 180          | **360** ❌   |
| pure mint / burn 100 | 100          | 100          |

It matches the net figure **only** for one-sided events (pure mint/burn),
because there is no second leg to double — a coincidence of representation, not
a property. The same redeem recorded as a transfer-to-issuer (two-sided) gives
`Σ|delta| = 200`, whereas `max(Σ+, Σ−) = 100` in **both** representations.
`max(Σ+, Σ−)` is representation-robust; `Σ|delta|` is not.

Representation note: the token-event decoder records burn/clawback with
`from` set and **`to = None`** (one-sided). This is why the net figure is
correct and stable for our data.

## Edge case: mint + burn + transfer in one transaction

`max(Σ+, Σ−)` measures net accumulation, so a **mint → transfer → burn** that
cancels (100 created, moved, destroyed) nets every account to 0 → figure = 0.
This is **consistent net semantics** (nothing net settled), the same way it
ignores routing hops — not a defect. A transaction that net-mints returns the
net supply increase. There is no single number that also captures gross
throughput; that is inherent to compressing create + destroy + move into one
value. The case is bounded to be a margin by measurement (see R-prod-measurements:
transactions carrying both a mint and a burn are ~0.04% of all transactions,
and the subset that actually cancels is smaller; on classic accounts it is
effectively 0%).

## Summary

`max(Σ+, Σ−)` survives every adversarial case tried — gross's double-count,
`Σ+`'s missed burns, `Σ|delta|`'s doubled transfers and representation
fragility, and the mint/burn/transfer stress — and its only non-obvious output
(0 for a fully self-cancelling mint→move→burn) is a defensible net semantic on
a <0.04% margin.
