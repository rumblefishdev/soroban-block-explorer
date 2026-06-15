---
id: '0269'
title: 'DOCS: clarify operations_appearances.amount semantics — CH per-op rows differ from PG ADR 0033 aggregation'
type: DOCS
status: backlog
related_adr: ['0033', '0044']
related_tasks: ['0261', '0266']
tags: [priority-low, effort-small, docs, clickhouse, milestone-2]
milestone: 2
links:
  - lore/2-adrs/0033_soroban-events-appearances-read-time-detail.md
  - lore/2-adrs/0044_clickhouse-pilot-parallel-store.md
  - docs/architecture/database-schema/database-schema-overview.md
history:
  - date: '2026-05-25'
    status: backlog
    who: stkrolikiewicz
    note: >
      Discovered during 0261 investigation. The CH
      `operations_appearances` table uses a per-op sort key
      (`ledger_sequence, transaction_id, application_order`) and
      therefore stores ONE row per operation — diverging from the
      Postgres schema baked into ADR 0033 + 0037 where one row
      represents the aggregated `(tx, type, …)` identity tuple
      with `amount = count` of matching ops.

      The empirical evidence: in a 250 K-row sample of CH
      `operations_appearances` filtered to type ∈ {2, 13}
      (path payments), `amount` averages ~1.0 (max 68 outlier).
      That is incompatible with the ADR 0033 "count of
      appearances" semantic, which would average across many ops
      per row. ADR 0044 (CH pilot) does not call out this delta
      explicitly; future maintainers (including the next Claude
      session) need the divergence documented before reading
      `amount` as a count again.
---

# DOCS: clarify operations_appearances.amount semantics — CH vs PG schema delta

## Summary

The CH `operations_appearances` table stores one row per operation
(per-op sort key), whereas the Postgres version per ADR 0033 / 0037
stores one row per aggregated identity tuple with `amount = count`.
This task captures the CH-specific semantic in the docs so the
next reader does not assume PG-style aggregation when reasoning
about CH data.

## Why this surfaced

Task 0261 investigation hit a wrong-assumption loop: a query
running `sum(amount)` over CH `operations_appearances` to estimate
"transferred value" returned values nowhere near real stroop
counts. The wrong assumption was that `amount` mirrored the PG
ADR 0033 / 0037 semantic (count of ops per aggregated identity
row).

Empirically:

```sql
SELECT type, asset_code, count() AS n, avg(amount) AS avg_amt, max(amount) AS max_amt
FROM operations_appearances
WHERE type IN (2, 13)
GROUP BY type, asset_code
ORDER BY n DESC
LIMIT 20;
```

returns `avg_amt ≈ 1.000–1.07` for ~250 M path-payment rows. That
is consistent with **one row per op** (most ops appear in the
table exactly once, very rare ops repeat across multiple events
for the same identity tuple), and NOT consistent with the PG
"aggregate identity row with N ops per row" semantic.

The CH schema (per `SHOW CREATE TABLE operations_appearances`)
confirms:

```sql
ENGINE = ReplacingMergeTree
ORDER BY (ledger_sequence, transaction_id, application_order)
PARTITION BY intDiv(ledger_sequence, 500000)
```

`application_order` is part of the sort key — that is the per-op
identifier in Stellar's XDR `Transaction.operations[]` array. Sort
key uniqueness ⇒ one row per op.

## Scope

### 1. Update `docs/architecture/database-schema/database-schema-overview.md`

Add a "CH schema delta" callout to the `operations_appearances`
section explaining:

- CH version is ReplacingMergeTree, sort key
  `(ledger_sequence, transaction_id, application_order)`
- One row per op (per-op grain), NOT aggregated like the PG version
- `amount` field reflects op-body semantic (typically a per-op
  value or count of inner sub-effects depending on op_type) —
  document the actual semantic per op_type by reading the
  indexer write path
- Per-op uniqueness invariant: re-inserting the same
  `(ledger, tx, app_order)` triple with corrected fields triggers
  ReplacingMergeTree dedup; that is the partial-migration pattern
  used by task 0266

### 2. Amend ADR 0044 (CH pilot parallel store)

Add a "Schema deltas vs ADR 0033 / 0037" subsection enumerating
every CH table whose semantic differs from its PG counterpart.
`operations_appearances` is the headline delta; verify whether
`soroban_events_appearances` and
`soroban_invocations_appearances` follow the same per-op model in
CH (likely yes, but confirm by `SHOW CREATE TABLE` on the box).

### 3. Memory note

Update `[[ch-26-sql-gotchas]]` with a "PG ADR != CH" gotcha:
"ADR 0033 / 0037 describes the PG schema. CH `operations_appearances`
(and likely the other `_appearances` tables) use per-op sort keys,
not aggregated identity tuples — sum(amount) is a per-op sum, not
a count of ops."

## Investigation step (precondition to docs)

Before writing the docs, dig into the indexer code:

- `crates/indexer/src/handler/persist/staging.rs` —
  `OpTyped::from_details`, the per-op-type extraction map
- Identify what value goes into `amount` per op_type
  (PathPayment, Payment, LP ops, ChangeTrust, …)
- Confirm `soroban_events_appearances` + `soroban_invocations_appearances`
  CH schemas

The docs should be derived from the actual code, not from
analogy to ADR 0033.

## Acceptance Criteria

- [ ] `database-schema-overview.md` updated with CH-vs-PG delta
      callout for `operations_appearances`.
- [ ] ADR 0044 amended with the schema-delta subsection.
- [ ] Memory note `[[ch-26-sql-gotchas]]` includes the gotcha.
- [ ] Indexer code investigated; `amount` semantics per op_type
      captured concretely.
- [ ] **Docs updated** — covered (this task is docs).
- [ ] **API types regenerated** — N/A.

## Notes

- This is documentation hygiene, not a code bug. The CH schema
  is correct per ADR 0044; the gap is purely that ADR 0044
  doesn't explicitly call out the delta against ADR 0033.
- Priority is low — Claude in future sessions will hit the same
  confusion until the docs land. Worth a half-day eventually.
