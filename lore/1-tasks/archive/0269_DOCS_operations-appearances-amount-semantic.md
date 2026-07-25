---
id: '0269'
title: 'DOCS: clarify operations_appearances.amount semantics — CH per-op rows differ from PG ADR 0033 aggregation'
type: DOCS
status: canceled
reason: obsolete
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
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Premise refuted by the code and by prod — this task documents a
      difference that does not exist.** Went to write the docs, checked the
      indexer first as the task instructs ("derived from the actual code, not
      from analogy"), and the code says the opposite of the task.
      `stage.rs` builds `operations_appearances` by aggregating: the rows come
      from an `op_agg` HashMap keyed by (op_type, source, destination, contract,
      asset_code, asset_issuer, pool_ids, tx_hash), and each row is written with
      `application_order: agg.min_apply_order` and **`amount: agg.count`**. That
      is one row per identity-tuple per transaction carrying a count — i.e.
      exactly the ADR 0033 / 0037 Postgres semantic the task claims CH does not
      use. `application_order` is a MIN over the aggregated ops, not a per-op
      identifier, so its presence in the sort key does not imply one row per op.
      `soroban_invocations_appearances` is the same shape (`amount` starts at 1
      and increments).
      Measured on prod to be sure: **6,531,839,482 rows, of which 385,329,088
      (5.9%) carry `amount > 1`, max 100, mean 1.3882.** Per op type the split is
      decisive — offers aggregate almost always (type 3: mean 1.997 over 785M
      rows; type 12: 1.983; type 21: 1.985), while path payments do not (type 2:
      1.000; type 13: 1.035).
      **Why the task got it wrong:** its evidence query filters `WHERE type IN
      (2, 13)` — the two op types that happen to sit at ~1.0 — and generalises
      that to the table. The 0261 debugging conclusion ("do not `sum(amount)` to
      estimate transferred value") is still correct, but for the opposite reason:
      not because CH stores one row per op, but because `amount` is an operation
      COUNT in both stores and never a value.
      Consequence for scope: there is no CH-vs-PG delta to write up. What is
      worth documenting is the shared aggregate semantic plus the count-not-value
      trap. Criteria updated; the investigation criterion is closed since this
      note is its output. Someone who owns the docs should decide the new
      framing before writing.
  - date: '2026-07-22'
    status: canceled
    who: karolkow
    reason: obsolete
    note: >
      **Canceled — the delta this task exists to document does not exist.**
      Re-verified independently before closing, on a different sample and by a
      different route than the entry above, because the task's *original*
      evidence was also a measurement that could not have failed.
      Confirmed again: `stage.rs:1137` writes `amount: agg.count`, and a single
      transaction in ledgers 63,000,000–63,000,100 carries `amount = 25` at two
      separate `application_order` positions — collapsing that one row per
      operation cannot produce.
      Recording the method failure too, because it happened twice on the same
      table. The original task inferred "one row per operation" from the sort
      key `(ledger_sequence, transaction_id, application_order)`. Re-checking, I
      counted rows against distinct values of that same tuple, got equality, and
      briefly took it as confirmation — but that tuple is the table's
      `ORDER BY`, so distinctness holds by construction and the comparison could
      not have come back negative. Only reading the writer settled it.
      **Nothing to write up, so nothing is spawned.** The one durable fact —
      `amount` is an operation COUNT in every store and never a transferred
      value — belongs wherever `operations_appearances` is described, and
      `database-schema-overview.md` §4.4 already says exactly that. The doc was
      right the whole time.
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

> ⚠ **The premise below is refuted — see the 2026-07-22 history entry.**
> There is no CH-vs-PG delta to document: ClickHouse uses the _same_
> aggregate-with-count semantic as ADR 0033. The first three criteria were
> written to describe a difference that does not exist and must be rewritten
> before anyone acts on them.

- [ ] ~~`database-schema-overview.md` updated with CH-vs-PG delta callout~~ —
      **rewrite needed**: document the shared aggregate semantic and the real
      trap (`amount` is an op _count_, never a value), not a delta.
- [ ] ~~ADR 0044 amended with the schema-delta subsection~~ — **rewrite
      needed**, same reason.
- [ ] Memory note `[[ch-26-sql-gotchas]]` includes the gotcha — still valid,
      but the gotcha is "`amount` counts operations" rather than "CH differs
      from PG".
- [x] Indexer code investigated; `amount` semantics per op_type captured
      concretely — **done 2026-07-22**, and it is what refuted the premise.
      Note the paths in "Implementation" are stale: the code lives in
      `crates/db-clickhouse/src/persist/stage.rs`, not
      `crates/indexer/src/handler/persist/staging.rs`.
- [ ] **Docs updated** — covered (this task is docs).
- [ ] **API types regenerated** — N/A.

## Notes

- This is documentation hygiene, not a code bug. The CH schema
  is correct per ADR 0044; the gap is purely that ADR 0044
  doesn't explicitly call out the delta against ADR 0033.
- Priority is low — Claude in future sessions will hit the same
  confusion until the docs land. Worth a half-day eventually.
