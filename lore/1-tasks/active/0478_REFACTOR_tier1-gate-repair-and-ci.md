---
id: '0478'
title: 'REFACTOR: repair the four failing Tier-1 query docs and make the gate run in CI'
type: REFACTOR
status: active
related_adr: ['0032', '0044']
related_tasks: ['0331', '0445']
tags: [docs, clickhouse, ci, tooling, priority-medium, effort-medium]
links: []
history:
  - date: '2026-08-13'
    status: active
    who: karolkow
    note: >
      Surfaced while verifying 0445: the endpoint-queries README claimed "all
      34 statements pass" while the gate actually passes 28 of 38, and four
      endpoints have been failing on develop for some time. Root cause of the
      drift is that the gate is a manual script — it appears in no CI workflow —
      so nothing stops the documented SQL from diverging from the code.
  - date: '2026-08-13'
    status: backlog
    who: karolkow
    note: >
      Deferred, deliberately. The work started here reached past its own scope:
      repairing the SQL docs, wiring the gate into CI, and — via the same
      container — waking up eleven ClickHouse-backed tests that had been
      skipping in CI. That last thread is a distinct question about test
      coverage as a whole and moves to 0480; this task keeps the SQL gate.
      Partial work is preserved on branch `refactor/0478_tier1-gate-repair`
      (PR 404, closed unmerged): 01 and 08 parse again, 09's table reference is
      corrected, and the runner arm for 01 supplies the head. Nothing there is
      lost, and none of it is on develop.
      The README line this task rewrote was reverted on develop as part of the
      deferral, so the directory now states the situation in one sentence
      instead of carrying a defect list nobody had signed up to fix.
  - date: '2026-09-25'
    status: active
    who: karolkow
    note: >
      Activated. Re-verified on develop c0e85893: 01 (`{head}` brace), 08 and
      09 (join the retired `asset_aggregates`) still fail as filed; two more
      fail since — 21 (runtime `format!` fragments, 0199) and 22 (named
      placeholders added by 0374/0580). The runner cannot gate yet: `all`
      always exits 0 (`|| echo`, nine `|| true`), its list names a missing 20
      and skips 24. The partial branch conflicts with 0374 in
      liquidity_pools/queries.rs; only its 01 + runner parts are reusable, and
      its ci.yml / ch.rs changes belong to 0480. CI already starts ClickHouse
      (ci.yml, task 0406), so the gate is one extra step.
---

# REFACTOR: repair the Tier-1 query docs and gate them in CI

## Summary

Four of the 23 documented endpoint queries do not parse against the canonical
schema. Fix all four, then wire `run_endpoint_ch.sh all --syntax-only` into CI
so the set cannot rot again.

Measured 2026-08-13 with `docker compose up -d clickhouse db-clickhouse-init`
then `./run_endpoint_ch.sh all --syntax-only`: **28 of 38 statements parse**.
Identical failures on develop before task 0445 touched the directory, so none
of this is new breakage.

## The four failures, three causes

| Endpoint               | Error                                   | Cause                                                                 |
| ---------------------- | --------------------------------------- | --------------------------------------------------------------------- |
| `01_get_network_stats` | `Syntax error at '}'` on `{head} - 200` | A Rust `format!` brace survived the copy out of `network/queries.rs`. |
| `08_get_assets_list`   | `Unknown table 'asset_aggregates'`      | Table retired by 0331; the file predates the unified balance model.   |
| `09_get_assets_by_id`  | `Unknown table 'asset_aggregates'`      | Same.                                                                 |
| `22_get_search`        | `Syntax error at ':'` on `:q_hex`       | Named placeholders; `substitute_params` only handles positional `$N`. |

## Why it rotted

Two independent reasons, and the second is the load-bearing one:

1. The documented SQL is **hand-copied** from the Rust query strings. Two
   sources of truth, no mechanism keeping them in step.
2. **The gate runs nowhere.** `grep -rl run_endpoint_ch .github/workflows`
   returns nothing. A gate that no pipeline invokes cannot hold a line.

Fixing (1) without (2) buys a few weeks. This task does both.

## Scope

1. `01` — replace `{head}` with a positional parameter and give the runner arm
   the value.
2. `22` — convert `:q` / `:q_hex` to `$N`, or teach `substitute_params` the
   named form. Prefer converting the file: one convention beats two.
3. `08` / `09` — re-derive against the unified model. `balance_aggregates` is
   keyed by `asset_id` (the re-added `assets.id` surrogate), not by
   `(asset_code, issuer_id)`. The authoritative read is `assets::queries`,
   which is now two-phase (resolve keys → hydrate); the documented form should
   mirror that shape, split with `-- @@ split @@`, rather than pretending a
   single statement still covers it. Drop the pre-0331 banner once done.
4. CI — a job that starts the schema container and runs the gate on the paths
   it covers. It must fail the build on a non-zero exit.
5. README — restore an accurate Tier-1 line once the gate is green, and delete
   the `§Tier-1 failures` section this task exists to empty.

## Out of scope

**Generating the docs from the Rust queries.** That is the only change that
removes the duplication for good, and it is a bigger design decision (an
extraction convention plus a check that the generated files are current). If
this set rots again after CI is in place, that is the next step — not before.

## Acceptance criteria

- [x] `./run_endpoint_ch.sh all --syntax-only` exits 0 with every check
      passing — **67 of 67** (not "38 of 38": that count predates the runner
      checking every statement; see Implementation Notes)
- [x] CI runs that command and fails the build when it does not
      (`.github/workflows/ci.yml`, rust job, step "Endpoint queries parse
      (Tier 1)")
- [x] `08` / `09` reference only tables that exist in `init.sql`, and their
      shape matches the two-phase read in `assets::queries`
- [x] No named placeholders remain, or the runner handles them — 22 is
      positional; the only `{name}` left are 21's real `format!` fragments,
      substituted by the runner per interval
- [x] README states the measured result, and the failures section is gone
      (the section had already been reverted on develop; the Tier-1 row now
      carries the measurement and a new §Tier 1 gate describes the checks)
- [x] **Docs updated** — `docs/architecture/database-schema/endpoint-queries-clickhouse/README.md`
      updated (banner, FINAL table `balance_aggregates` row, Tier-1 row,
      §Tier 1 gate); no other architecture doc describes this tooling (N/A)

## Implementation Notes

Branch `refactor/0478_tier1-gate-ci` off develop 786e1943. Five commits:

1. **01** — `{head}` → `$1` (re-applied from the closed PR 404 branch, runner
   arm included), and the body synced with `network/queries.rs`
   (`accounts_recent` / `soroban_contracts FINAL` counts instead of
   `system.tables.total_rows`, `LIMIT 1 BY sequence`).
2. **08 / 09** — re-derived statement by statement from
   `assets/queries.rs`. 08: A seek (holder-ranked, cursor
   `(holder_rank, id)`, filters `$2`/`$3`/`$6`), B `hydrate_sql`, C
   `resolve_soroban_contracts`, D `resolve_page_issuers`. 09: A issuer seek by
   StrKey, B key seek, C SAC-wrapper seek, D hydrate, E contract context,
   F issuer by id. Both read `balance_aggregates` by `asset_id`.
3. **21 / 22 / 24** — 22: `:named` → `$1..$8`, twelve split sections, and
   synced with `search/queries.rs` (pool-by-code bucket added, deduped
   `soroban_contracts` join in the asset arms, `scm` label in the NFT bucket,
   duplicated CODE:ISSUER arm removed). 21: leg identities bound as
   `$4..$9`, `$1` now the hex string (`unhex($1)`, as the Rust binds it);
   `{bucket_fn}` / `{price_bucket_fn}` / `{series_view}` / `{carry}` stay,
   documented as a per-interval table. 24: two missing split markers.
4. **Runner** — rewritten dispatch and accounting: walks every `NN_*.sql`
   present; every split section must be checked; `;` count must equal the
   section count; leftover `$N` / `:name` / `{name}` fail by name; counters
   and exit 1 on any failure. 21 is checked for 1h / 1d / 1w, with the
   `prices.*` view replaced by an empty inline stand-in.
5. **CI** — one step after the ClickHouse e2e step, `if: !cancelled() &&
steps.clickhouse.outcome == 'success'`.

Counts, measured 2026-09-25 against local docker ClickHouse 26.3.21.7 with
`init.sql` applied: baseline on develop — 01, 08, 09, 10, 14, 21, 22 failed and
the runner exited 0 (and 20 "failed" for a missing file); after — 64
statements in 23 files, 67 checks (08 statement A twice, 21 three times), all
pass, exit 0. Negative checks: `asset_aggregates` put back into 08 → `65 of 67`,
`FAILED: 08`, exit 1; on a scratch copy an unsupplied `$5`, a `:slack`, a
missing split marker, a file without an arm and an arm skipping a statement
each FAIL with their own message, exit 1.

## Issues Encountered

- **More failures than filed.** 10 (the arm still fed the retired
  two-variant `asset_code`/`issuer_id` inputs) and 14 (arm supplied five of
  six params) also failed; 02, 06, 07, 10, 11 had statements the old arms
  never ran, and 24 had three statements in one section. Hence the coverage
  and `;`-count checks.
- **The `prices` database is not in this repo's schema.** 21 cannot plan
  against it locally or in CI (same reason `decode_smoke` skips). Stubbed —
  see Design Decisions.
- **Local compose network clash.** Another worktree's compose network holds
  the pinned 172.30.0.0/16, so `docker compose up` failed; ran with a
  scratchpad override file moving the subnet. CI is unaffected (fresh runner).

## Design Decisions

### From Plan

1. **Convert 22 to positional, not teach the runner `:name`.** One
   convention; the runner's guard now rejects `:name` outright.
2. **08 / 09 mirror the two-phase read, split with `-- @@ split @@`.**
   Page keys that Rust inlines appear as example literals (the 02
   convention), not placeholders.
3. **Gate in the existing rust job**, on the ClickHouse started for 0406.

### Emerged

4. **21's `format!` fragments stay `{name}` and the runner substitutes
   them** from a table duplicated in the 21 header, the runner arm and
   `get_pool_chart.rs`. Positionalising them would misdescribe the Rust (they
   are not bound). Drift is visible, not detected.
5. **`prices.price_usd_series*` replaced by an empty inline subquery** in the
   runner rather than creating a stub `prices` database: the gate stays
   read-only. It cannot check the view names or column types.
6. **Guards beyond the brief**: statement coverage, `;`-vs-split count,
   placeholder check, file list derived from the directory. Each closes a way
   the old runner passed silently.
7. **CI filter and docs-only shortcut touched**, not just one step: the rust
   job did not trigger on this directory, and the documentation-only
   shortcut (`^(lore|docs)/`) would have skipped a SQL-only push after a
   green run. Both now treat `endpoint-queries-clickhouse/*.{sql,sh}` as code.
8. **Content synced where the file was rewritten anyway**: 01 body, 22
   buckets. Not audited elsewhere.
9. **08 statement A checked twice** (unfiltered first page; every filter +
   cursor), so both sides of each `$N IS NULL OR …` gate are type-checked.

## Future Work

- `06_get_accounts_by_id` still documents the pre-0331 balances read
  (`account_balances_current`); it parses, so the gate cannot see it. README
  banner says so. Not filed — needs a decision on whether to refresh by hand
  or wait for generated docs.
- Generating these files from the Rust queries remains out of scope (see
  above).
