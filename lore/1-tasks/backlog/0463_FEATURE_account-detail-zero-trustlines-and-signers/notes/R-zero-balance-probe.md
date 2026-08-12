---
type: research
status: mature
date: '2026-08-04'
spawned_from: '0463'
---

# R — how many hidden zero rows are real trustlines

## Question

`fetch_balances` drops every `amount = 0` row. A trustline holding nothing and
a trustline that was closed are written identically, so the filter hides both.
Before designing around it: **of the rows we hide, how many are real?**

## Method

Deterministic 1-in-499 sample of holders with non-native zero rows
(`modulo(abs(holder_id), 499) = 3`), 200 accounts, each checked against
`horizon.stellar.org/accounts/{id}` — Horizon returns the complete balance
list including zero-balance trustlines, which is what makes it usable as an
oracle here. Zero network failures; 200/200 answered.

Horizon is used **as a measurement oracle only**. It is ruled out as a runtime
source — see `S-source-options.md`.

## Result — the aggregate is misleading

```
our zero rows      : 2412
live on chain      :  134
ghosts             : 2278   (94.4 %)
```

94 % sounds fatal. It is not the number that matters, because the distribution
is **bimodal**.

## Split by account

**Typical accounts — 1 to 3 zero rows (76 of the 84 that still exist):**

```
our zero rows :  93
live          :  91   → 97.8 %
```

**"Warehouse" accounts — a handful with hundreds each:**

```
873 zero rows →   0 live
625           →   6
530           →   6
 54           →   4
 24           →   9
```

Five accounts produced 2106 of the 2412 rows. The aggregate is entirely theirs.

**Merged accounts — 116 of 200 (58 %).** They no longer exist on chain, so
every row is a ghost by construction. This is not chance: zeroing every
trustline is a **precondition of `account_merge`**, so "account with many zero
rows" correlates strongly with "account that was then deleted".

## Four consequences for the design

1. **The feature matters more than assumed.** On the accounts people actually
   open, 97.8 % of what we hide is a real, live trustline.
2. **Naively unhiding would be a disaster.** One account would list 873 assets
   it has no relationship with.
3. **58 % of the calls need never happen.** The detail page already derives
   `deleted`; a merged account has nothing to verify.
4. **Our index is also incomplete, separately.** Three of the 84 live accounts
   carry MORE zero trustlines on chain than we hold rows for (9 vs 16, 1 vs 2,
   3 vs 5). Across the sample the chain has 144 live zero trustlines and we
   know 134 — **we are missing ~7 %**.

Hypothesis for (4), unverified: trustlines created before our ledger floor
(50,457,424) and untouched since, so they never crossed our stream.

## The number that killed the bulk-verify variant

Deduped ambiguous `(holder, asset)` pairs, same 1-in-499 slice:

```
67,414 in the slice  →  ~33.6 M across the table
```

At 200 keys per `getLedgerEntries` call that is **~168,000 requests** — hours
of sustained load against public infrastructure that is not an export
endpoint. A one-off RPC sweep to fill history is therefore off the table; see
`S-source-options.md`.

The same number is the strongest argument FOR read-time verification: 33.6 M
pairs exist, a page view asks about one to three. Precomputing all of them
means computing millions of answers that will never be read.

## Raw counts for context

Not deduped — includes superseded versions, so an upper bound only:

```
rows with amount = 0      : 41.9 M
of which classic credit   : 36.1 M
```
