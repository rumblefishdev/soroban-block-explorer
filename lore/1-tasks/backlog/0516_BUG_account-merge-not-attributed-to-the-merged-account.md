---
id: '0516'
title: 'BUG: account_merge is not attributed to the account being merged'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0500', '0324', '0463']
tags:
  [
    backend,
    indexer,
    operations,
    data-correctness,
    clickhouse,
    priority-low,
    effort-unknown,
  ]
history:
  - date: '2026-08-26'
    status: backlog
    who: claude
    note: >
      Spawned from 0500 future work. Found while measuring why the old
      `deleted` derivation under-detected; 0500 removed the consumer rather
      than the cause, so nothing on the read path depends on this today.
---

# BUG: a merge operation does not name the account it merges

## Symptom

`operations_appearances` carries no row naming the merged account for its own
`account_merge`. Concretely, on
`GAEGXYY63CYV34TH6HDVZ3L4WCYX7AUTLNOPFCNBR3RCQIB3MVSKLAWP`:

- its last operation is an Account Merge at ledger **57,037,462**, which is
  also its `last_seen_ledger`;
- that ledger holds **exactly one** `type = 8` appearance;
- **none of the 664 appearances in that ledger** names the account as
  `source_id` or `destination_id`;
- the account reaches its own transaction list through
  `transaction_participants` only.

So the operation exists and the account participated, but the two are not
linked in the appearances table.

## Why it is filed and not fixed

Task 0500 needed "is this account deleted?" and was getting `false` for dead
accounts. The fix there was to stop deriving the answer from operation history
and read it from the lifecycle column on the account's native holding
(ADR 0055) — one keyed lookup, complete since the 0463 checkpoint seed, and
verified against the chain on 236 accounts with no exceptions.

That removed the only known consumer. This defect is therefore invisible today
on the account page, which is why it is priority-low: nothing renders wrongly
because of it.

## What is NOT known

- **Scale.** One account was inspected in detail. Whether every
  `account_merge` is unattributed, or only some shape of them, is unmeasured.
- **Cause.** Whether `source_id` records the TRANSACTION source rather than
  the operation's explicit `sourceAccount`, whether the merged account should
  appear as `destination_id`, or whether the row is dropped earlier in the
  parser — none of this was investigated.
- **Blast radius.** Any other read that asks "which account did operation X"
  for an op with an explicit source account would inherit the same gap. Worth
  a grep over the API's operation queries before deciding this is harmless.

## Implementation

1. Measure first: over a bounded ledger window, count successful `type = 8`
   appearances whose `source_id` resolves to an account that a
   `transaction_participants` row for the same ledger also names — and the
   complement. The complement is the population.
2. Read the parser's operation-appearance emission for the explicit
   `sourceAccount` case, not just `account_merge`.
3. Decide whether the fix belongs in the writer or in the appearance model
   (a row per participating role rather than source/destination).
4. Re-measure to zero, and re-check whether anything else in the API was
   silently relying on the old shape.

## Acceptance criteria

- [ ] Scale measured and recorded BEFORE any change, over a named window
- [ ] Root cause identified in the writer, not guessed from the symptom
- [ ] Fixed, and the same window re-measured to zero
- [ ] Other operation types with an explicit `sourceAccount` checked for the
      same gap, and the result recorded either way
- [ ] Historical rows: state explicitly whether a re-parse is needed or the
      defect is forward-only
- [ ] **Docs updated** — or `N/A` with the reason
- [ ] **API types** — expected `N/A`; state it rather than leaving it blank
