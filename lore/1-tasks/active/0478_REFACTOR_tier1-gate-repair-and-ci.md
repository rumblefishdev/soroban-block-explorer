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

## A fifth failure appeared mid-task, which is the argument for CI

`21_get_liquidity_pools_chart` parsed on 2026-08-12 and fails on 2026-08-13.
Nothing in this task touched it: `git log` puts its last change at 4ba9424e,
task 0199, merged 2026-08-11. It carries ten Rust `format!` fragments
(`{bucket_fn}`, `{series_view}`, `{carry}`, six `{leg_*}`), so it shipped
un-parseable and nobody noticed — because no pipeline runs the gate.

That is the case for step 4 stated better than any argument: the set does not
merely contain old rot, it accrues new rot at merge time.

**21 is deliberately not fixed here.** Its placeholders are computed SQL
fragments — a function name, a view name, per-leg literals — not values. Mapping
them to parameters means reading the intent out of `liquidity_pools/queries.rs`;
guessing produces a file that parses and lies, which is worse than one that
fails loudly. It belongs with whoever holds 0199's context, or to a deliberate
session of its own.

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

## Progress

- [x] **01** — `{head}` → `$1`, runner arm supplies the head. Parses.
- [x] **08** — joins `balance_aggregates` on `asset_id`. Parses.
- [~] **09** — table reference corrected, but the file is a multi-statement
  reference collection with **no `-- @@ split @@` markers**, so the runner
  sends all of it as one query, and its arm feeds `$1` an integer where the
  statement compares against a `String` contract id. Needs the same
  split-and-renumber treatment as 22.
- [ ] **21** — new regression, see above. Not this session's to guess at.
- [ ] **22** — six bucket queries in one file, nine named placeholders
      (`:q`, `:q_hex`, `:ledger`, `:partition`, …), no split markers, and a
      runner arm passing ten positional params the file never uses. The whole
      file predates the `$N` + split convention.
- [ ] CI job

Gate: **29 of 38** parsing, up from 28 (a net +1 while 21 regressed under us).

The remaining three are one shape of work, not three: 09 and 22 need splitting
into statements with positional parameters and per-statement runner values; 21
needs its computed fragments resolved. None is a five-minute edit, and doing
them badly is worse than leaving them failing loudly.

## Acceptance criteria

- [ ] `./run_endpoint_ch.sh all --syntax-only` exits 0 with 38 of 38 parsing
- [ ] CI runs that command and fails the build when it does not
- [ ] `08` / `09` reference only tables that exist in `init.sql`, and their
      shape matches the two-phase read in `assets::queries`
- [ ] No named placeholders remain, or the runner handles them
- [ ] README states the measured result, and the failures section is gone
- [ ] **Docs updated** — the endpoint-queries README per ADR 0032; no other
      architecture doc describes this tooling
