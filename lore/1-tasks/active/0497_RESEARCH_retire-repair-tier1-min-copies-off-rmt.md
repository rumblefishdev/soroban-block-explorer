---
id: '0497'
title: 'RESEARCH: retire repair-tier1 — move every MIN-semantics copy off RMT state tables'
type: RESEARCH
status: backlog
related_adr: ['0055']
related_tasks: ['0464', '0463', '0420', '0492']
tags:
  [
    backend,
    clickhouse,
    backfill-runner,
    data-integrity,
    priority-low,
    effort-medium,
  ]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from the LP-holdings decision session. The direction is decided
      there: repair-tier1 is a compensating process for MIN-semantics columns
      copied onto ReplacingMergeTree state tables, and it should die as a
      class — one entry at a time, as each copy moves to a fact-derived or
      history-derived read. The LP entry already dies with that session's
      design. This task is the per-column investigation for the rest.
---

# RESEARCH: retire repair-tier1

## The verdict already taken

`repair-tier1` exists because ReplacingMergeTree keeps the **newest** row per
key while MIN-semantics columns need the **earliest** value — so every
parallel or `--reindex` backfill silently corrupts them, and a mandatory
repair pass (indexer stopped) recomputes them from append-only fact tables
(`repair_tier1.rs:18-45`, `docs/backfills.md`).

That is a correctly built compensator for a modelling compromise. The
decision, taken in the LP-holdings map session: **the compromise goes, not
just the symptom.** Each MIN copy moved off an RMT state table kills its
repair entry; when the last entry dies, the subcommand and the runbook step
die with it. New rule recorded in the merge ADR: no new MIN-semantics copies
on RMT state tables, ever.

## Inventory to investigate, per column

| Column                                                 | Fact source (already used by the repair)                                              | Candidate route                                                                                                                                     |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lp_positions.first_deposit_ledger`                    | `operations_appearances`, type 22                                                     | **dies via the LP merge** — derived in the pool-side companion MV at refresh; fallback sparse column. Not this task's work; listed for completeness |
| `accounts.first_seen_ledger`                           | `MIN(ledger_sequence)` over `transaction_participants` (3.6 B rows)                   | the hard one — rendered on the account page, 24 M accounts; measure a companion-MV refresh vs read-time derive vs keep-until-0464                   |
| `nfts.minted_at_ledger`                                | `nft_ownership`, event_type 0                                                         | companion or read-time; measure                                                                                                                     |
| `nfts_pending.minted_at_ledger`                        | `nft_ownership_pending`, event_type 0                                                 | same, or dies if `nfts_pending` goes vestigial (task 0309 direction)                                                                                |
| `soroban_contracts.deployer_id` + `deployed_at_ledger` | no dedicated fact table — repair reads the raw pre-FINAL table, documented as fragile | worst case: may need its own small fact table, or 0464-era treatment; investigate first                                                             |

## Questions to answer

- Per column: derivation cost at read vs at MV refresh vs staying until the
  balance-history table (0464) absorbs it. Measure, do not estimate —
  `accounts.first_seen_ledger` over 3.6 B `transaction_participants` rows is
  the one that can sink a route.
- Which consumers actually render each column (check `web/src/`, not a guess —
  the LP session nearly declared a live column dead by grepping a directory
  that does not exist).
- Whether `soroban_contracts`' deployer info needs a proper fact table first —
  the repair's own docs call the current rebuild fragile.
- Sequencing with 0464: anything 0464 absorbs for free should not get its own
  machinery here.

## Findings — session of 2026-09-01/02 (answers two inventory rows)

### `nfts.minted_at_ledger` — ROUTE TAKEN: read-time, shipped

Task **0528**, merged as PR #442. The API derives the value from `nft_ownership`
(`min(ledger_sequence) WHERE event_type = 0`) and no longer reads the stored
column. Cheap because that fact table is 23 092 rows.

Measured before/after on prod: **643 of 13 932 tokens wrong → 1**, and
**13 292 / 13 292 already-correct values unchanged**, so the derivation is a
strict superset. The remaining one is a token the chain never recorded a mint
for; serving NULL there is correct.

Two implementation notes that will recur on every other column:

- `min()` over a non-Nullable column returns a **non-Nullable** type, and without
  `join_use_nulls = 1` (unavailable — `api_reader` is readonly) a LEFT JOIN miss
  fills the type DEFAULT `0`, not NULL. Un-wrapped this both fails the
  `Option<i64>` decode (500 on the endpoint) and renders a missing value as
  "ledger 0". Wrap in `nullIf(_, 0)`. Not hypothetical — `nfts` held a literal
  stored `0` that the old code displayed.
- Where the value is a sort key or cursor key, the ORDER BY, the keyset
  predicate and the cursor payload must move together or pagination stops being
  total.

### The defect is NOT backfill-only

Measured: these columns drift on **ordinary live ingest**, no parallel run
required — the indexer sees only its own batch, so any later event carries no
historic minimum and the RMT replace erases it. `nfts` was losing **~30 tokens
per day**. `docs/backfills.md` still framed this as a parallel-backfill trap;
corrected in the same session. (`crates/backfill-runner/README.md` already had it
right — "recurring mop … re-drift under live ingest".)

### `accounts.first_seen_ledger` — the route-sinking measurement, done

Corruption, 400-row deterministic sample joined to `transaction_participants`:
**14 / 400 diverge (3.5%)**, every one of them LATER than the true first
appearance → order of **570 k of 16.17 M** accounts. Consumers confirmed by
reading the code, not guessed: `AccountSummary.tsx` and `AccountsTable.tsx`.

**Correction to this task's inventory:** `transaction_participants` is
**10.73 B rows**, not 3.6 B.

Read-time derivation, page-scoped, 50 accounts, two independent prod slices:

| Slice            | Rows read   | Bytes read | Duration |
| ---------------- | ----------- | ---------- | -------- |
| first 50 by id   | 180 883 365 | 2.34 GiB   | 413 ms   |
| `id % 7919 = 13` | 116 778 764 | 27.29 GiB  | 2 708 ms |

At a 100 GB/hour quota that is **4–40 page views per hour**. **Read-time is
sunk for this column** — exactly the outcome this task warned to measure for.

Writer read-before-write (the shape the PG writer used, `LEAST(a.first_seen, …)`)
was also measured, with 100 **literal** StrKeys: 1.74 M rows / 98 MiB / 24 ms per
100 accounts, every batch. Also too expensive, and it still breaks under parallel
backfill. (An earlier `WHERE id IN (…)` variant read 17.8 M rows — `accounts`
sorts on `account_id`, not on the `id` surrogate. Worth knowing before re-measuring.)

Backfilling a maintained aggregate, by contrast, is affordable — one partition
measured and extrapolated:

| Backfill                                  | Rows    | Read     | Compute | Peak memory |
| ----------------------------------------- | ------- | -------- | ------- | ----------- |
| `accounts` ← `transaction_participants`   | 10.74 B | ~160 GiB | ~17 s   | 94 MiB      |
| `lp_positions` ← `operations_appearances` | 6.85 B  | ~46 GiB  | ~10 s   | 27 MiB      |

Memory is a non-issue (streams in sort-key order), compute is seconds. Only the
read quota binds, so chunk by partition — 90 chunks of ~7.4 GiB. The contrast
that decides it: the same aggregate costs 2.34–27.3 GiB **per page view** at read
time, and ~160 GiB **once, ever** as a backfill.

### A companion MV cannot be the route — verified by experiment

Reproduced the backfill's `FREEZE` → copy → `ATTACH PART` sequence on a local
ClickHouse 26.3:

- **Materialised views do NOT fire on `ATTACH PART`.** Control: `INSERT` of 3
  rows reached the MV target. Then 5 rows frozen from a separate table and
  attached — the source went 3 → **8**, the MV target stayed at **3**. A
  companion MV would be silently short by exactly the backfilled range, i.e. it
  would reproduce the defect it exists to remove, on the very scenario that
  opened 0228.
- **A projection travels inside the frozen part and arrives usable.** The frozen
  directory carries `pmin.proj`; after attaching, `system.projection_parts` lists
  it, the minima are correct, and the query still succeeds under
  `force_optimize_projection = 1`.
- **`AggregatingMergeTree` folds correctly and is right before merging.** Three
  parts holding 900 / 100 / 500 for one key read back as 100 **without** `FINAL`.
  That matters here specifically: prod RMT tables are chronically un-merged
  (0420), so any route that assumes merges have run is fragile — this one is not.

**So the surviving routes for the large columns are a projection or an
`AggregatingMergeTree` engine change** (the latter is what 0421 already proposes
for `accounts`). A companion MV is out unless the backfill procedure gains an
explicit re-materialise step — which puts a forgettable manual step back on the
critical path, the same failure shape as `repair-tier1` itself.

### Other columns, corruption measured

- `soroban_contracts.deployed_at_ledger` — **1 597 / 146 397 diverge (1.1%)**,
  plus 3 NULL where the value is known. Self-contained in a 185 k-row table, so
  read-time derivation is cheap here.
- `nfts_pending.minted_at_ledger` — 4 / 277 NULL, and **66 / 277 have no Mint row
  at all**, so any derivation must serve NULL for those rather than invent a
  value.

### Related tasks to reconcile before starting

**0232** (per-column mitigation) already defers to this task in premise.
**0421** (accounts rewritten with defaults) is wider than the MIN problem — it
covers `sequence_number` and `home_domain` on the same whole-row write — and
already proposes the `AggregatingMergeTree` route with a schema sketch. **0531**
was filed in this session before those were found and is a duplicate; it is
marked superseded and its evidence is the section above.

## Done means

A per-column route recorded (companion / read-time / wait-for-0464 / keep,
with the measured reason), implementation subtasks filed for the routes that
win, and an explicit statement of what remains in `repair-tier1` and until
when. The end state — the subcommand deleted, `docs/backfills.md` losing the
mandatory step — is the success criterion even if it lands incrementally.

## Progress 2026-09-25 — NFT entries retired (branch `refactor/0497-retire-nft-repair-entries`)

- **Retired:** `nfts.minted_at_ledger`, `nfts_pending.minted_at_ledger`.
  Checked before deleting: every NFT query in `crates/api` derives the value
  from `nft_ownership` (including the per-contract list); `system.query_log`
  over 14 days shows the stored column only written by the indexer and read
  by no service user.
- **Kept, and why:** `accounts.first_seen_ledger` and
  `soroban_contracts.{deployer_id, deployed_at_ledger}` — read by the API and
  corrected by nothing else. `lp_positions.first_deposit_ledger` — broken
  (0468: zeroes 95% of positions), yet the only thing correcting the live
  drift (7 of 8 sampled live rows later than the true first deposit); it
  retires with 0468's storage fix, not before.
- **Found:** the two CH-gated `accounts` tests race on the shared
  `accounts_staging_repair_tier1` table when run in parallel (one fails with
  the default thread count, all pass with `--test-threads=1`). Pre-existing.
- The `nfts.minted_at_ledger` column itself stays (dropping it is a
  production `ALTER`, formerly 0529).

### Stored NFT mint ledgers measured (2026-09-25) — the copy must go, not just its repair

Owner's rule (karolkow, 2026-09-25): an unread column is no licence to keep a
false value in the database. Production, against `min(ledger_sequence)` of
Mint rows in the matching ownership table:

| Table          | Tokens | Wrong value | NULL though the mint is known | Value with no Mint row |
| -------------- | ------ | ----------- | ----------------------------- | ---------------------- |
| `nfts`         | 14,045 | 0           | 706                           | 1                      |
| `nfts_pending` | 277    | 0           | 1                             | 63                     |

The drift now surfaces as NULL, not as a wrong number: a later batch carries
no mint (`stage.rs` merges `(None, b) => b` within a batch only) and the RMT
replace keeps that row. Retiring the repair entry without dropping the column
lets the NULLs grow. Route: drop `minted_at_ledger` from both tables (former 0529) — the fact lives in `nft_ownership*`, the copy can only drift.

### NFT mint ledger no longer stored (2026-09-25, decision karolkow 37 A)

Branch `refactor/0497-retire-nft-repair-entries`, commit `0c217dda` (after two
pure moves: NFT staging → `persist/stage/nfts.rs`, NFT query tests →
`nfts/queries/{tests,decode_smoke}.rs`). `minted_at_ledger` leaves `NftRow`,
`NftPendingRow`, the staging merge and `init.sql`; the parser keeps its
in-batch mint ledger (it selects the enrichment candidates). Considered and
rejected (37 C): a min-carrying `nft_mints` table — a second copy of what
`nft_ownership` already holds and reads cheaply (23k rows).

**Rollout — verified on a local CH 26.3 with the new writer (g9 e2e):**

| Table state                                 | New writer                               |
| ------------------------------------------- | ---------------------------------------- |
| column present, no DEFAULT (production now) | **fails** — `SchemaMismatch` on `NftRow` |
| column present, `DEFAULT NULL`              | passes                                   |
| column dropped                              | passes                                   |

1. Operator, before the indexer deploy (metadata-only, old writer unaffected):
   `ALTER TABLE nfts MODIFY COLUMN minted_at_ledger Nullable(Int64) DEFAULT NULL`
   and the same on `nfts_pending`.
2. Deploy the indexer.
3. After every old container is recycled: `DROP COLUMN minted_at_ledger` on
   BOTH tables in one sitting — `nft-reclassify` promotes with
   `INSERT INTO nfts SELECT * FROM nfts_pending`, which needs equal shapes.

`api-types` regenerated: no diff (the wire field is the derived value).

**Nothing true is lost with the column (checked 2026-09-25).** The 64 stored
values with no Mint row behind them (1 in `nfts`, 63 in `nfts_pending`) are
all `0` — not a ledger. Their tokens carry transfer rows only; `0` is the
`repair-tier1` LEFT JOIN miss writing the type default, the same trap as
0468's LP zeros. Every other stored value equals the `nft_ownership` mint.
