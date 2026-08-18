---
id: '0056'
title: 'A liquidity position is a holding — merge `lp_positions` into `balances`'
status: proposed
deciders: [karolkow]
related_tasks:
  [
    '0463',
    '0493',
    '0496',
    '0497',
    '0498',
    '0499',
    '0126',
    '0162',
    '0331',
    '0339',
  ]
related_adrs: ['0055', '0051', '0027']
tags: [clickhouse, data-model, balances, liquidity-pools, assets, read-path]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/377'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/405'
history:
  - date: '2026-08-17'
    status: proposed
    who: karolkow
    note: >
      Decided in the lp-holdings planning map after five research tickets:
      primary sources, the classic/Soroban split measurement, the assets
      dimension cost, the table-difference inventory, and a decision session
      that audited the SAC precedent on its own merits. Implementation is
      trigger-gated — see the triggers clause.
---

# ADR 0056: a liquidity position is a holding

**Related:**

- [ADR 0055: holding lifecycle is a column on `balances`](0055_holding-lifecycle-column-on-balances.md)
- [ADR 0051: SAC is a facet of classic_credit](0051_sac-as-facet-of-classic-credit.md)
- [Task 0499: implement the merge](../1-tasks/backlog/0499_REFACTOR_merge-lp-positions-into-balances.md)
- [Task 0493: account detail renders LP positions](../1-tasks/backlog/0493_FEATURE_account-detail-renders-lp-positions.md)

---

## Context

We store the same user-facing concept — "my liquidity position" — in two
differently shaped tables depending on which AMM implementation issued it:

- **Classic AMM** pool shares live in `lp_positions`
  (40,728 positions, 6,024 accounts; prod 2026-08-17). On chain a classic
  pool share **is a trustline**: CAP-0038 chose to extend `TrustLineEntry`
  rather than add an entry type, and `TrustLineAsset::PoolShare(PoolId)` is
  an arm of the same union as every other trustline asset. It is
  non-transferable — only deposit/withdraw change it — and it is **not a
  token**: no issuer, no code, no `assets` row today.
- **Soroban AMM** LP tokens are ordinary SEP-41 contract tokens and already
  sit in `balances` as type-3 holdings (Pool Share Token: 3,915 holders;
  Comet Pool Token: 267; prod, name-matched lower bound).

Every layer above us models one entity: CAP-0038 ("`TrustLineEntry` is
modified to store pool shares"), the XDR union, Horizon's single `balances`
array, the Go SDK's one `Balance` struct ("an account's holdings"), and SDF's
current pipeline `stellar-etl`, whose single `TrustlineOutput` carries
`liquidity_pool_id` as its own field. We are the only link with two entities.

The split's original justifications were **PostgreSQL features**: ADR 0027
gave `lp_positions` a real FK to `liquidity_pools`, a partial index
`WHERE shares > 0`, and a `CHECK` on `pool_id` length. All of them expired on
2026-05-10 when the schema was mirrored into ClickHouse, which has none of
those. Nobody made a bad call; the premises disappeared and nothing recorded
it. Task 0331's unification did not cover this because its scope was the two
balance tables it replaced, and its LP note concerned the **holder** dimension
("is a pool a holder?"), not whether a pool share is a held asset.

Costs measured against the split: the account-side read of `lp_positions` is
a full scan (its key is pool-first — task 0493 exists because of this); two
writers and two zero-conventions for one ledger entry type; a duplicated
lifecycle column (ADR 0055 had to alter both tables); and issue #405 — both
AMM kinds side by side with a filter — would be a two-table reconciliation.

## Decision

**Merge. A classic pool share becomes a holding row in `balances`, keyed by a
pool asset in the `assets` dimension. `lp_positions` is retired after
migration.**

### 1. The pool asset

- **`TokenAssetType::PoolShare = 4`.** Not the retired `2` — reusing a freed
  discriminant is the exact mechanism behind the live label defect (task
  0496). The wire label is decided with 0496; Horizon's vocabulary for it is
  `liquidity_pool_shares`.
- **Identity: a new `pool_id FixedString(32)` column on `assets`, appended to
  the sort key** in the same `ALTER … ADD COLUMN … , MODIFY ORDER BY` —
  ClickHouse permits appending a newly added column to the sorting key and it
  is metadata-only (existing parts stay valid; the column is constant in
  them). Pool rows key as `(4, '', 0, 0, pool_id)` — unique per pool; all
  existing rows keep their order.
- `ids::asset_id` gains one arm: a pool's `asset_id = hash64(pool_id bytes)`.
- The account page resolves pool display data by joining `liquidity_pools`
  directly on `pool_id`.

### 2. The position rows

`(holder_id, asset_id, amount, last_updated_ledger, closed_at_ledger)` —
nothing new. `shares Decimal128(7)` is stored by ClickHouse as an `Int128`
scaled by 10⁷, which is byte-identical to `amount` for a 7-decimal asset:
the migration of 40,728 rows is an identity copy. The `shares > 0` filter
convention (inherited from ADR 0027's partial index) dissolves into
`closed_at_ledger` per ADR 0055.

### 3. The pool-side companion

`balances` is holder-first, so the pool page's "who is in this pool" would
scan 76 M rows. Projections are refused on ReplacingMergeTree (CH 26.3,
Code 344, task 0353), so the alternate ordering is the in-house pattern —
a **refreshable MV into a plain MergeTree ordered `(asset_id, holder_id)`**,
exactly as `accounts_recent` and `balance_aggregates_mv` already do. One
companion serves three things: the pool participants page, task 0493's
account-side rendering needs, and asset-first "top holders" reads that are
impossible today.

### 4. `first_deposit_ledger` — derived, not stored

The column is live on the UI (`web/src/pages/pool-detail/PoolParticipants.tsx`)
so the wire contract stays. But it is a **historical MIN**, and MIN copies on
ReplacingMergeTree are corrupted by every parallel backfill — that is what
`repair-tier1` compensates for. The companion MV computes it at refresh from
the append-only fact table (`operations_appearances`, type 22), which a
backfill cannot corrupt. Consequences: `balances` stays free of sparse
payload, and **the `lp_positions` entry in `repair-tier1` dies**.

Measurement gate at implementation: the refresh aggregation's cost. Fallback:
a sparse `first_deposit_ledger` column on `balances`, meaningful for pool
rows only. The safety net that the repair process used to provide must be
replaced explicitly: a test on the companion's query plus the comparison
probe — a derived value with neither is risk moved, not removed.

### 5. Blast-radius exclusions, written down

The assets list and search would newly see **52,555** pool rows. Both get an
explicit exclusion (`WHERE asset_type != 4` or a deliberate opt-in for
search). `asset_sac` and `asset_enrichment` never hold pool rows — noted, not
implied. `balance_aggregates_mv` groups by `asset_id`, so pool-share assets
get their own supply rows (equal to outstanding shares — the quantity CAP-0038
itself tracks as `totalPoolShares`) and **no existing published number
changes**. Pooled reserves live in `LiquidityPoolEntry`, never in trustlines,
and are untouched.

### 6. Migration and backward completeness

The migration reuses the 0463 seed machinery and **the same checkpoint
snapshot artifact** — pool-share trustlines are in the same bucket list as
every other trustline. Rows are versioned on each entry's own
`lastModifiedLedgerSeq` (never a window boundary — task 0492), carry
provenance per 0492's convention, and closures derive by the same
set-difference with the same anomaly policy as ADR 0055's seed.

### 7. Triggers clause — decided now, implemented at first consumer

This ADR is binding immediately for **new** design (nothing may deepen the
split), but the migration itself starts only when a consumer exists:
task 0493 is scheduled, issue #405 is accepted for delivery, or Soroban-AMM
feature work begins. Until then `lp_positions` remains the classic store and
its ADR-0055 lifecycle column keeps working. Rationale: an investment without
a consumer, and the 0464-style trigger pattern already works.

## Rules established by this decision

Recorded here because until now they existed only as scars in other ADRs'
histories:

1. **`assets` rows are immutable identity; mutable payload never lives on a
   dimension row.** (The SAC lesson — ADR 0051's first attempt put facet
   columns on `assets` and they were destroyed by whole-row re-emits.)
2. **Sparse identity in the key is fine; sparse payload on a state table is
   suspect; a MIN-semantics copy on any ReplacingMergeTree is forbidden from
   now on.** Task 0497 retires the existing ones.
3. **A retired discriminant number stays dead.** (The 0496 lesson.)
4. **New tables and artifacts key on surrogates, never on the natural
   tuple.** (The audit found the side tables' 4-column joins evolution-hostile
   — task 0498.)

## Rejected alternatives

| Alternative                                                | Reason                                                                                                                                                                                                                                                                                                                             |
| ---------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Keep the split                                             | Its justifications were PostgreSQL features that expired in 2026-05-10; remaining costs (full scan, two writers, two conventions, two-table #405) buy nothing. The one real argument — a hard table boundary versus `WHERE` discipline — loses to the fact that native/classic/soroban already live behind the same soft boundary. |
| Pool StrKey in `asset_code`                                | Pushes a `LowCardinality(String)` dictionary from 177,832 short values to ~230k with 56-char entries.                                                                                                                                                                                                                              |
| `hash64(pool_id)` in the `contract_id` slot                | Was briefly preferred; rejected once the honest column proved metadata-only. It would be the schema's **first column whose value is not what its name says** (type-3's `contract_id` holds an actual contract), and it needs a surrogate added to `liquidity_pools` for resolution.                                                |
| `assets` as AggregatingMergeTree with sticky facet columns | Considered during the SAC audit and recorded here for the first time: one table, no join, but an engine migration of the central dimension and per-column merge semantics forever. Not better enough.                                                                                                                              |
| `first_deposit_ledger` stored on `balances`                | Extends the repair-tier1 tax from 40,728 to 76 M rows. Kept only as the measured fallback.                                                                                                                                                                                                                                         |
| Generalised `opened_at_ledger` on every holding            | Birth is a historical MIN (RMT-hostile, repair-bound) and pre-floor values are unknowable — the honest home for "when did it start" is the balance-history table (0464), where it is the first row of the series.                                                                                                                  |
| Dropping `first_deposit_ledger`                            | It is rendered on the pool participants table — a live feature. (Nearly declared dead by grepping a directory that does not exist; recorded so the lesson survives.)                                                                                                                                                               |

## Consequences

**Positive.** One holding mechanism across all five kinds (native, classic,
Soroban token, Soroban LP token, classic pool share); task 0493 becomes a
seek plus rendering; issue #405's filter becomes a `WHERE` clause; the
`repair-tier1` LP entry dies; no published number changes; the schema stops
contradicting the protocol.

**Negative, accepted.** Pool-page reads become ≤ refresh-interval stale
(pattern already accepted twice); the boundary becomes `WHERE` discipline —
two exclusions that every future `assets` reader must respect; ~30 lines of
task 0463's LP writer arm get rewritten; `repair-tier1` and the pool
participants query are reworked as part of implementation.
