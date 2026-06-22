---
id: '0232'
title: 'FEATURE: ClickHouse live-mode drift mitigation for all 6 Stage 1 Tier-1 columns'
type: FEATURE
status: backlog
related_adr: ['0044']
related_tasks: ['0118', '0194', '0228']
blocked_by: ['0228']
tags:
  [
    priority-medium,
    effort-large,
    layer-data,
    clickhouse,
    enrichment,
    materialized-view,
    aggregating-merge-tree,
    live-mode,
    decision,
  ]
milestone: 2
links: []
history:
  - date: '2026-05-18'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from the 2026-05-18 CH-enrichment planning session, after
      noting that `asset_aggregates.rs` (task 0228 Stage 1) is a
      post-merge snapshot only and the same drift problem applies to
      every Tier-1 column rebuilt by `repair-tier1`. First framing
      ("MV for assets aggregates only") was based on a wrong claim
      that the other 5 monotone columns wouldn't drift in live mode —
      verified false by reading `crates/db-clickhouse/src/persist/stage.rs`.
      All 6 Stage 1 Tier-1 columns drift in live ingest under
      `ReplacingMergeTree(version)` semantics because the CH live
      writer cannot afford the read-before-write that the PG writer
      uses (`LEAST(a.first_seen_ledger, i.fs)` in
      `crates/indexer/src/handler/persist/write.rs:115`). This task is
      the single decision point for the live-mode mitigation strategy
      across all 6 columns.
---

# FEATURE: ClickHouse live-mode drift mitigation for all 6 Stage 1 Tier-1 columns

## Summary

After task 0228 lands on Hetzner, the `repair-tier1` and
`asset-aggregates` subcommands give a correct one-shot post-merge
snapshot. But because the ClickHouse live writer does not (and
cannot affordably) read-before-write each row, every column that
`repair-tier1` fixes will silently drift again once live ingest
resumes. This task picks the mitigation strategy column-by-column
and ships it.

## Status: backlog

Blocked on task 0228 landing on Hetzner. The MV / engine-swap
options below all assume the Stage 1 snapshot is in place as the
correctness baseline; this task layers the live-mode preservation on
top.

## Context

### Drift mechanism

`ReplacingMergeTree(version_column)` keeps the row with the highest
version per `ORDER BY` key on background merge. The CH live writer
emits a new row on every ledger that touches the entity (account,
position, NFT, contract). The new row carries `version = current
ledger`, which beats any prior row including the one written by
`repair-tier1`. For columns whose semantically-correct value is
"earliest" or "smallest" (not "latest"), the merge wins the wrong
way.

The PG writer side-steps this with
`UPDATE ... SET first_seen_ledger = LEAST(a.first_seen_ledger, i.fs)`
([crates/indexer/src/handler/persist/write.rs:115](../../../crates/indexer/src/handler/persist/write.rs#L115))
— a per-row read-before-write that CH RMT doesn't support inline.

### Two column classes — different fixes

The 6 Tier-1 columns split into two semantic classes. Strategy
differs.

**Class A — monotone "first-observed" (5 columns)**

| Table               | Column                              | Semantics                                                                                     |
| ------------------- | ----------------------------------- | --------------------------------------------------------------------------------------------- |
| `accounts`          | `first_seen_ledger`                 | Earliest ledger account participated in any tx; **never increases** after first correct write |
| `lp_positions`      | `first_deposit_ledger`              | Earliest ledger this `(pool, account)` had a deposit; **never increases**                     |
| `nfts`              | `minted_at_ledger`                  | Ledger of the Mint event; **never changes** after mint                                        |
| `nfts_pending`      | `minted_at_ledger`                  | Same as `nfts`                                                                                |
| `soroban_contracts` | `deployer_id`, `deployed_at_ledger` | Original deploy info; **never changes** after deploy                                          |

These columns are monotone — once set correctly, they should be
preserved against any later write that might carry a different value.

**Class B — live-fluctuating aggregates (1 column-pair)**

| Table    | Columns                        | Semantics                                                                                                |
| -------- | ------------------------------ | -------------------------------------------------------------------------------------------------------- |
| `assets` | `holder_count`, `total_supply` | Computed from current `account_balances_current` state. Changes with every transfer / trust open / close |

These columns are NOT monotone — they need recompute against the
current balance state, on a schedule.

> **RESOLVED by lore-0293 (2026-06) — Class B done (2 of 6 columns).**
> Implemented as an **event-driven AggregatingMergeTree**, NOT a scheduled
> recompute: `total_supply`/`holder_count` moved out of `assets` into
> `account_asset_balance_state` (AMT keyed by
> `(asset_code, issuer_id, account_id, asset_type)`, holding
> `argMaxState(balance, last_updated_ledger)`), maintained incrementally by
> `account_asset_balance_state_mv` on every `account_balances_current` insert.
> Reads sum `argMaxMerge` at query time, scoped to the page. No clobber, no
> schedule, no full scan; `argMax` is idempotent so re-runs never double-count.
> The old `assets.{total_supply,holder_count}` columns are kept (dead) for
> backward compat — their drop + prod engine migration is a separate cleanup
> task. Proof + design:
> `0293_RESEARCH_ch-indexer-atomicity-partial-ledger/notes/G-assets-aggregate-clobber-proof.md`.
> **Only Class A (the 5 monotone columns) remains in this task.**

## Options analysed

### Class A — three viable strategies

**A1. Engine swap to `AggregatingMergeTree` +
`SimpleAggregateFunction(min, Int64)`**

```sql
CREATE TABLE accounts (
    id Int64,
    account_id String,
    first_seen_ledger SimpleAggregateFunction(min, Int64),   -- changed
    last_seen_ledger  SimpleAggregateFunction(max, Int64),   -- changed
    sequence_number   Int64,
    home_domain       LowCardinality(Nullable(String))
)
ENGINE = AggregatingMergeTree
ORDER BY (account_id);
```

- AMT merges via `min()` / `max()` automatically on background merge.
- Live writer keeps writing per-ledger rows without read-before-write.
  Each row carries its own `first_seen_ledger = current_ledger`; AMT
  collapses by taking the MIN across the partition.
- **Eliminates the need for `repair-tier1` long-term** — columns
  self-heal under merge.
- **Cost**: schema migration on a multi-TB CH, plus writer-side
  changes if any `SimpleAggregateFunction` interactions surprise the
  staging code. Big.
- **Scope on Hetzner**: requires `EXCHANGE TABLES` migration like
  the Stage 1 pattern, but reshapes every Tier-1 table. Bigger
  than `repair-tier1` itself.

**A2. Periodic re-run of `repair-tier1` via cron / MV REFRESH**

```sql
CREATE MATERIALIZED VIEW accounts_first_seen_refresh
REFRESH EVERY 1 DAY
TO accounts
AS
SELECT
    a.id, a.account_id,
    ifNull(m.min_ledger, a.first_seen_ledger) AS first_seen_ledger,
    a.last_seen_ledger, a.sequence_number, a.home_domain
  FROM accounts FINAL AS a
  LEFT JOIN (
      SELECT account_id AS id, min(ledger_sequence) AS min_ledger
        FROM transaction_participants
       GROUP BY id
  ) AS m ON m.id = a.id;
```

- Reuses the exact `repair-tier1` query, scheduled by CH (MV) or by
  systemd timer (calling the existing subcommand).
- No schema change.
- **Cost**: each refresh full-scans the fact table. For accounts
  this means scanning `transaction_participants` (~500M rows on
  full Soroban-era mainnet — 10× heavier than the
  `account_balances_current` scan in Class B).
- **Refresh interval trade-off**: drift accumulates between
  refreshes. For monotone columns this is acceptable — the wrong
  value is "later than truth" by at most one refresh interval.

**A3. Live-writer read-before-write**

Change `stage.rs` to read existing
`{first_seen_ledger, first_deposit_ledger, minted_at_ledger,
deployer_id, deployed_at_ledger}` per affected key before INSERT,
then write `min(existing, incoming)` (or preserve-non-NULL).

- No schema change.
- No periodic refresh needed.
- **Cost**: per-partition extra SELECTs against state tables. At
  one batch per partition this is bounded but adds latency to the
  live ingest path.
- Code change is invasive across multiple persist sites in
  `crates/db-clickhouse/src/persist/stage.rs`.

### Class B — single viable strategy

**B1. Refreshable MV against `assets`** (carried over from the
first 0232 framing).

```sql
CREATE MATERIALIZED VIEW assets_aggregates_refresh
REFRESH EVERY 1 HOUR
TO assets
AS
SELECT
    a.asset_type, a.asset_code, a.issuer_id, a.contract_id, a.name,
    if(a.asset_type IN (1, 2),
       CAST(ifNull(agg.total_supply, toDecimal128(0, 7)) AS Decimal128(7)),
       a.total_supply) AS total_supply,
    if(a.asset_type IN (1, 2),
       CAST(ifNull(agg.holder_count, 0) AS Int32),
       a.holder_count) AS holder_count,
    a.icon_url
  FROM assets FINAL AS a
  LEFT JOIN (
      SELECT asset_code, issuer_id,
             countIf(balance > 0) AS holder_count,
             sum(balance) AS total_supply
        FROM account_balances_current FINAL
       WHERE asset_type IN (1, 2)
       GROUP BY asset_code, issuer_id
  ) AS agg ON agg.asset_code = a.asset_code AND agg.issuer_id = a.issuer_id;
```

Mechanics identical to the Stage 1 `asset-aggregates` subcommand,
scheduled by CH.

Class B engine swap to AMT does not fit cleanly because
`holder_count` requires `countIf(balance > 0)` over the union of
account balances — not a `min/max/sum` over per-row values. Would
need `AggregateFunction(uniqExactIf, …)` with state-modifier columns
and a state-merge read-path — heavier than refreshable MV.

## Recommendation

**Phase split — ship the cheap thing now, defer the strategic
thing.**

**Phase 1 (this task, near-term, post-go-live)**:

- **Class B → B1 (refreshable MV)**. Land first; assets aggregates
  drift fastest (every transfer) and the SELECT is cheap (~50M-row
  scan).
- **Class A → A2 (periodic refresh / cron)**. Reuse the existing
  `repair-tier1` subcommand on a daily cron. No code change, no
  schema change. Accepts up-to-1-day drift on monotone columns —
  acceptable because the drift direction is "later than truth"
  and most monotone-column writes are first-observation anyway.

**Phase 2 (separate proposal task, longer-term)**:

- **Class A → A1 (AMT engine swap)** when team commits to the
  bigger migration. Eliminates `repair-tier1` permanently and frees
  up the cron slot. Spawn as separate proposal task referenced from
  here (suggested `0233_PROPOSAL_aggregatingmergetree-for-monotone-tier1-columns`).

Rationale for picking A2 over A1 / A3 for Phase 1:

- A1 is the _correct_ architectural answer but requires a multi-TB
  schema migration that itself needs careful design — out of scope
  for "fix the drift now".
- A3 invades the hot ingest path; correctness risk at the live write
  layer. Heavier review burden than a refresh job.
- A2 reuses code we already have (`repair-tier1` from task 0228).
  Cron entry on Hetzner + done.

## Implementation Plan

### Step 1 — Class B MV (assets aggregates)

1. Add `CREATE MATERIALIZED VIEW assets_aggregates_refresh ...`
   DDL to `crates/db-clickhouse/schema/init.sql` (idempotent
   `CREATE MATERIALIZED VIEW IF NOT EXISTS`).
2. Add `OPTIMIZE TABLE assets FINAL` cron to the Hetzner runbook
   (every refresh + 5 min buffer) to collapse RMT duplicates the
   MV introduces.
3. Measure refresh duration on Hetzner; commit the chosen interval
   value (default proposal: 1 hour).

### Step 2 — Class A cron

1. Add `infra-hetzner/cron/repair-tier1-daily.cron` (or equivalent
   in the Ansible playbook from task 0227) that runs
   `backfill-runner repair-tier1 --target clickhouse` once per
   24 h.
2. Document the disk-space transient (each rebuild creates a
   staging table ≈ same size as the source) in the runbook.
3. Monitor: drift probe — query
   `SELECT count() FROM accounts WHERE first_seen_ledger >
(SELECT min(ledger_sequence) FROM transaction_participants
 WHERE account_id = accounts.id)` should be ≤ N% during steady
   state (acceptance threshold TBD by measurement).

### Step 3 — Spawn AMT proposal task (Phase 2)

Create `0233_PROPOSAL_aggregatingmergetree-for-monotone-tier1-columns.md`
capturing the A1 design: column-by-column engine swap, writer
interaction audit, migration sequence. Reference it from this
task's `Notes` section once spawned.

## Acceptance Criteria

- [ ] `CREATE MATERIALIZED VIEW assets_aggregates_refresh ...
TO assets ...` lands in `init.sql`; applies cleanly to
      Hetzner via the standard schema migration path.
- [ ] `OPTIMIZE TABLE assets FINAL` cron entry in the Hetzner
      runbook.
- [ ] PG ↔ CH parity probe on top-100 assets shows < 0.1% drift in
      steady state.
- [ ] Daily `backfill-runner repair-tier1 --target clickhouse`
      cron configured on Hetzner (Ansible playbook or systemd
      timer); runbook documents recovery if the cron fails.
- [ ] Drift-probe queries committed for each Class A column;
      operator-runnable; results within agreed thresholds.
- [ ] Separate AMT proposal task spawned and linked from this
      task's `Notes` section.
- [ ] **Docs updated** — `docs/architecture/data-pipeline/`
      describes the live-mode mitigation layer (MV + cron); CH
      schema overview gains a "drift mitigation" subsection.
- [ ] **API types regenerated** — N/A unless an audit forces an
      API shape change.

## Alternatives considered

- **Pure cron for everything (Class A + B both via repair-tier1
  rerun)** — works but heavier scan-cost for class B (full
  `account_balances_current` scan every hour) and worse staleness
  semantics than a 1-hour MV refresh. MV is cheaper + fresher for
  Class B.
- **Pure MV for everything** — A2-MV pattern (TO existing table)
  for Class A monotone columns. Equivalent to A2-cron but heavier
  scan for accounts (`transaction_participants` ≈ 500M rows). Cron
  every 24 h is cheaper than MV refresh every hour, and the slower
  drift accumulation on monotone columns tolerates 24 h staleness.
- **Read-before-write live writer (A3 for Class A)** — invasive,
  deferred. Could revisit if drift probes show A2 cron isn't
  keeping up.
- **AMT swap for everything (A1 + AMT-uniq for B)** — strategic
  endgame, separate task.

## Notes

- This task is a leaf operational task, not a go-live blocker. Stage
  1 (manual one-shot `repair-tier1` + `asset-aggregates`) is the
  go-live gate; this task smooths the live-mode steady state
  afterward.
- If the team commits to the AMT engine swap (Phase 2) before this
  task ships, Class A scope here collapses to "delete the cron, do
  the AMT swap" — no MV / cron technical debt to unwind.
- Drift-probe queries belong in the operator monitoring set;
  recommend embedding them in a Grafana / CH-native alerting
  dashboard during this task.
