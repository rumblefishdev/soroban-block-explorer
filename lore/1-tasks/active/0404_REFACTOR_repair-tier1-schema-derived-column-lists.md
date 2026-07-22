---
id: '0404'
title: 'REFACTOR: staging rebuilds stop hand-writing column lists (`SELECT * REPLACE`) — delete the drift surface 0388 was a symptom of; test rebuild_soroban_contracts'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0388', '0394', '0406', '0228', '0379', '0400']
tags: [priority-medium, effort-small, clickhouse, repair-tier1, robustness]
links:
  - crates/backfill-runner/src/repair_tier1.rs
  - crates/backfill-runner/src/contract_type_rebuild.rs
  - crates/backfill-runner/src/wasm_upgrade_backfill.rs
history:
  - date: 2026-07-17
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned when 0388 was closed. 0388 fixed one stale column (`name` in the
      soroban_contracts repair) but not the class: all five repairs still hardcode
      their column lists, so the next schema drift reproduces the same bug. Flagged
      during the 0359 backfill review as a non-blocking follow-up; verified unowned
      2026-07-17.
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Re-verified and re-scoped. Premises all still hold on develop (`c422622c`):
      4 hardcoded lists in `repair_tier1`, no parity check, `rebuild_soroban_contracts`
      still the only rebuild without a test. Prod exposure **today is zero** — `chq`
      against `system.columns` shows all 5 tables match their lists exactly (6/6, 5/5,
      8/8, 8/8, 8/8), same as 0388's close-out check. But repair-tier1 is not dormant:
      `system.tables.metadata_modification_time` records a real run on **2026-07-16**
      (15:40:56 → 15:55:00, in `execute()` order) during the 0379 Phase-3 drain.
      Three findings changed the plan.
      **(1) The failure modes were measured, not reasoned** — ClickHouse 26.3 (prod is
      26.3.10.60), 4-column table vs 3-column INSERT list: the unlisted column comes out
      **empty, no error**. Same fixture through `SELECT l.* REPLACE (… AS col)` keeps it,
      including through `FINAL` + `LEFT JOIN` and with a `MATERIALIZED` column present.
      The positional form (`contract_type_rebuild`, `wasm_upgrade_backfill`) fails loudly
      instead (`Code 20 NUMBER_OF_COLUMNS_DOESNT_MATCH`), and 0388's own shape is
      `Code 16 NO_SUCH_COLUMN_IN_TABLE` — so `repair_tier1`'s explicit-list form is the
      **only** silent one in the crate.
      **(2) The fix already exists in the same crate** — `assets_id_backfill` and
      `nft_reclassify` are both column-list-free (`a.* REPLACE (…)` / `SELECT *`). So the
      original plan (derive from `system.columns`, or add a parity assert) would build a
      detector for a problem two siblings already make impossible. Assert = symptom, no
      list = cause. Swapped.
      **(3) The test AC is dead on arrival without 0406** — CI has no ClickHouse service
      and never sets `CLICKHOUSE_URL`, so all 27 CH-gated `#[tokio::test]`s (22 files)
      skip silently green, `rebuild_accounts` included. Linked 0406 both ways.
      Also confirmed NOT at risk, so nobody re-litigates it: `create_staging_like`'s
      `CREATE TABLE … AS` clone is byte-identical on `create_table_query` — skip indices
      and `SETTINGS` survive `EXCHANGE` (prod proof: `accounts.idx_acc_id`, added online
      2026-06-16, still present after the 07-16 swap). Projections are not a concern
      either — CH 26.3 refuses them on `ReplacingMergeTree` (`Code 344`, cf. 0353) and
      prod `system.projections` is empty.
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Trigger corrected — the parallel path this task was framed around is **retired**.
      `docs/backfills.md` records the operator runbook for it (task 0233) as
      "canceled as obsolete" in 2026-05, reason: *"no future parallel backfill is
      planned"*. `repair-tier1` recurs anyway, via the **other** trigger: rule 3 makes
      it mandatory after `run --reindex`, which is the documented answer to both "new
      derived table over history" and "bad data in place". That is what fired on
      2026-07-16 — 0379 re-parsed 13M ledgers from S3 with `--reindex`, then ran
      `repair-tier1` as step 2b (14.33M accounts corrected; it OOMed on the first
      attempt, `Code 241`). So: not dead tooling, but the reason it lives is
      `--reindex`, not parallelism. Stated here because reading "post-parallel-backfill
      repair" and concluding "parallel is retired → close this" is the correct
      inference from the old framing and the wrong conclusion.
      Standing above this task: `repair-tier1` is a **mop, and 0232 says the tap stays
      open** — the same 6 columns re-drift under live ingest because the CH writer
      can't afford the read-before-write the retired PG writer used. 0421 measures the
      result: `first_seen_ledger` wrong for **97.7% of accounts** as of 2026-07-21,
      five days after a successful repair pass. The cause-level fix is the engine
      change (AggregatingMergeTree + `SimpleAggregateFunction(min)`) proposed
      independently by 0232 (all 6 columns) and 0421 (accounts) — and it would retire
      this whole subcommand. This task is worth its (negative) diff regardless, but it
      is **downstream**: do not promote it as the fix for Tier-1 correctness.
  - date: 2026-07-22
    status: active
    who: karolkow
    note: >
      Activated for implementation. Scope as re-framed on 2026-07-21: replace the
      hardcoded column lists in `repair_tier1` with the list-free form the sibling
      backfills already use, plus the `rebuild_soroban_contracts` test (CH-gated,
      so effectively verifiable only once 0406 gives CI a ClickHouse service).
---

# REFACTOR: staging rebuilds stop hand-writing column lists

## Summary

Every whole-table rebuild in `backfill-runner` clones the live table
(`CREATE TABLE staging AS live`), fills it, and `EXCHANGE`s it in. Four of those
INSERTs — all in `repair_tier1.rs` — name their columns by hand, which is a
**second, hand-maintained copy of the schema**. When the real schema moves and
the copy doesn't, the rebuild writes a table that is missing what it never knew
about. 0388 was the loud half of that; the silent half is still live.

Kill the copy rather than watch it: `SELECT l.* REPLACE (<expr> AS <col>)`, the
form `assets_id_backfill` and `nft_reclassify` already use. No list, nothing to
drift, ~30 fewer lines.

## Context

Spawned from [0388](../archive/0388_BUG_repair-tier1-soroban-contracts-name-mismatch.md)
(one stale column removed), which sits in the 0304 → 0388 → 0392 → [0394](../archive/0394_BUG_backfill-runner-stale-name-column-sweep.md)
family — four PRs each fixing one copy of the same drift.

**When this code actually runs** (the old framing was wrong and invited a wrong
close): the multi-machine parallel path is **retired** — `docs/backfills.md`
records its runbook as canceled, _"no future parallel backfill is planned"_. The
live trigger is `run --reindex`, which rule 3 also makes `repair-tier1`-mandatory
and which the project runs whenever a new derived table needs history (0379,
2026-07-16, 13M ledgers re-parsed from S3 → `repair-tier1` as step 2b).

**Standing above this task**: `repair-tier1` is a mop and [0232](0232_FEATURE_clickhouse-tier1-live-mode-mitigation.md)
says the tap stays open — the same columns re-drift under live ingest, and
[0421](0421_BUG_first-seen-ledger-clobbered-on-every-account-write.md) measures
`first_seen_ledger` wrong for 97.7% of accounts five days after a green repair
run. The cause-level fix is the engine change (`AggregatingMergeTree` +
`SimpleAggregateFunction(min)`), which would retire this subcommand outright.
Do this task for the silent-data-loss path it closes, not as a Tier-1 fix.

Failure modes, **measured on CH 26.3** (prod 26.3.10.60), not reasoned:

| Form                     | Sites                                            | On a prod column the code doesn't know          |
| ------------------------ | ------------------------------------------------ | ----------------------------------------------- |
| explicit list            | `repair_tier1` ×4                                | **silent — column comes out empty, no error**   |
| positional, no list      | `contract_type_rebuild`, `wasm_upgrade_backfill` | loud (`Code 20 NUMBER_OF_COLUMNS_DOESNT_MATCH`) |
| `* REPLACE` / `SELECT *` | `assets_id_backfill`, `nft_reclassify`           | immune — column passes through                  |

So `repair_tier1` is the only silent one, and the pattern that fixes it is
already shipped twice in the same crate. A parity assert would _detect_ the
drift; deleting the list means there is nothing to drift. Take the second.

Not at risk, verified so it isn't re-litigated: the staging clone is
byte-identical (`create_table_query` incl. skip indices and `SETTINGS`), so
`EXCHANGE` cannot lose an index — `accounts.idx_acc_id` survived the 2026-07-16
prod run. Projections can't exist on these RMT tables at all (`Code 344`).

## Implementation

- [ ] Rewrite all 4 `repair_tier1` INSERTs as `SELECT <alias>.* REPLACE (…)`,
      deleting the column lists. `rebuild_soroban_contracts` replaces two columns
      in one `REPLACE (a AS x, b AS y)`. Verified working through `FINAL` +
      `LEFT JOIN` and with a `MATERIALIZED` column present.
- [ ] Same conversion for `contract_type_rebuild` and `wasm_upgrade_backfill`
      (positional 8-expr SELECTs over `soroban_contracts`). Not a correctness fix
      — they abort loudly — but it closes the class and deletes more than it adds.
      After this, **no** staging rebuild in the crate enumerates columns.
- [ ] Add a test for `rebuild_soroban_contracts`, mirroring
      `clickhouse_rebuild_accounts_*` (dry-run leaves live untouched + real run
      writes the corrected `deployer_id` / `deployed_at_ledger`). It is the
      function 0388 was about and the only rebuild with no test.
      **Depends on [0406](0406_CI_run-clickhouse-gated-tests-in-ci.md)** for the
      test to mean anything — CI provisions no ClickHouse, so this test will skip
      silently green exactly like the 27 that already do.
- [ ] Keep `rebuild_nfts` parameterized over `nfts` / `nfts_pending` — identical
      schemas, and with `*` the shared body stops depending on that being true.
- [ ] Nit while in the file: the `rebuild_lp_positions` doc comment says
      `isNotNull(pool_id)`; the code has `notEmpty(pool_ids)` (Array migration).

## Acceptance Criteria

- [ ] No staging-rebuild INSERT in `backfill-runner` enumerates column names.
      Grep is the check: `INSERT INTO {staging} (` returns nothing.
- [ ] A column added to any of the 5 tables passes through every repair
      **unchanged**, with no code edit and no assert to remember. Fixture-proven
      against a real CH, not argued.
- [ ] `rebuild_soroban_contracts` has coverage equivalent to `rebuild_accounts`.
      Marked done only with evidence the test **executed** (per 0394: a pass in
      0.53s is a skip) — `system.query_log` movement or 0406's visible-skip gate.
- [ ] `repair-tier1 --dry-run` row counts on prod match the pre-change run
      order-of-magnitude before any real run (unchanged operator ritual).
- [ ] Docs updated — `N/A`: ops tooling, no architecture shape change.
- [ ] API types regenerated — `N/A`: no `crates/api/**` or `Cargo.*` change.
