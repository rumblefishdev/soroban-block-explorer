---
id: '0495'
title: 'BUG: an operation row cannot say whether its asset is native XLM or a parse failure'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0470']
tags: [clickhouse, schema, frontend, native-xlm, data-integrity, effort-medium]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Split out of task 0470 stage 2, items 3 and 4 of its native-XLM sweep.
      The other six items were either fixed in #417, refuted by measurement,
      or already covered by task 0478. These two are what is left, and they
      are one subject: a row that lost the information, and a formatter that
      guesses at what was lost.
      Kept out of 0470 deliberately — that task is about one match rule per
      entity, and this is a schema column plus a shared display helper. Both
      are real, neither is urgent, and bundling them into a search-consistency
      task would have hidden them.
---

# BUG: an operation row cannot say whether its asset is native XLM or a parse failure

## Summary

`operations_appearances` stores an asset code but no asset type. Native XLM is
written with an empty code — and so is a row whose asset could not be parsed.
The two are byte-identical in the table, so nothing downstream can tell them
apart. The frontend fills the gap by assuming an absent code means XLM, which is
correct today only because someone verified it by hand against a different
table, and nothing keeps it correct.

## Context

Native XLM is the only Stellar asset with no code, and this project stores that
absence four different ways — the typed `asset_type = 0`, an empty string, a
surrogate id, and the literal string `"native"` on the wire. A sweep during task
0470 found that the typed form and the surrogate are consistent and pinned
together by a unit test. The residue is the empty-string form, and after #417
these are the two places it still bites.

### Item 3 — the column that is not there

The writer stores `asset_code.unwrap_or_default()` into
`operations_appearances`, and that table carries no `asset_type` and no
`asset_id`. An empty code therefore means either "this is native XLM" or "the
parser produced nothing", with no way to distinguish them at read time.

The neighbouring tables already solved this. `operation_asset_appearances` and
`lp_operation_amounts` both key the asset by the surrogate id, and the schema
header says so outright — native is a first-class key there, never an empty
sentinel. `operations_appearances` is the one that kept the sentinel.

### Item 4 — the formatter that guesses

`humanizeOp.ts` falls back to the native unit when an operation carries no code,
and `stroops.ts` does the same for any empty-or-null code it is handed. The
fallback is correct today: it was checked against `operation_asset_appearances`
and matched on every row of two samples (11 168 of 11 168, and 55 582 of
55 582). But the correctness lives in a comment rather than in a type, and
`stroops.ts` is shared by operations, pools and balances, so its default quietly
turns "asset unknown" into "XLM" for any caller that forgets to guard upstream.

This is the display failure mode the project has already ruled out elsewhere: a
plausible wrong value is worse than a visible gap.

## Why it is worth doing

Item 3 is the only finding in the whole sweep where the information is genuinely
**missing** rather than differently spelled. Everywhere else the fix was to read
the column that was already there. Here there is no column to read, so every
consumer is left guessing, and each one guesses separately.

Fixing item 3 retires item 4 as a side effect — once a row can state its asset
type, the formatter stops inferring one.

## Implementation sketch

Not decided; two shapes are plausible and the choice needs a measurement.

- **Add `asset_type`** to `operations_appearances`. Smallest column, answers the
  native-vs-missing question directly, nothing else changes.
- **Add `asset_id`** (the surrogate), matching what the two neighbouring tables
  already do. Larger column, but it makes the asset joinable rather than merely
  describable, and it removes the empty-code sentinel from the last table that
  still has one.

Either way this is a schema change on a table with a very large row count, so
the cost of the ALTER and of the historical backfill has to be measured before
committing to a shape. A forward-only fix is explicitly NOT acceptable here —
the whole point is that historical rows cannot answer the question either.

Once the column exists, the frontend fallbacks come out: `humanizeOp.ts` reads
the type, and `stroops.ts` stops defaulting an absent code to the native unit.

## Acceptance criteria

- [ ] An `operations_appearances` row states whether its asset is native, and a
      parse failure is distinguishable from native XLM at read time
- [ ] Historical rows answer the same question — no forward-only fix
- [ ] The ALTER and backfill cost is measured on production-scale data BEFORE
      the shape is chosen, and the measurement is recorded here
- [ ] `humanizeOp.ts` no longer infers the native unit from an absent code
- [ ] `stroops.ts` no longer defaults an empty or null code to the native unit;
      callers that legitimately format native pass it explicitly
- [ ] The hand-verified sample that currently justifies the fallback is replaced
      by a test, or explicitly retired with a reason
- [ ] **Docs updated** — the schema description of `operations_appearances`
      under `docs/architecture/**`, per ADR 0032
- [ ] **API types regenerated** — only if the wire shape changes

## Notes

The sweep that produced this is recorded in task 0470, under "Native XLM — the
same defect class on the remaining surfaces". Items 1, 2 and 5 shipped in #417;
item 6 was refuted by measurement; item 7 is already fixed on the unmerged 0478
branch; item 8 was called harmless and was not — the two `asset_type = 2`
mappings do belong to two different columns and both are correct, but the sweep
never asked whether a value crosses between the two spaces. It does, and task
0489 (`f4a2f2a4`) measured the cost: 16.1% of recent operations rendered
one-sided on production. The corrected entry is in 0470.
