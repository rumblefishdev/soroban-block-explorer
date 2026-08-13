---
id: '0478'
title: 'REFACTOR: repair the four failing Tier-1 query docs and make the gate run in CI'
type: REFACTOR
status: backlog
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

- [ ] `./run_endpoint_ch.sh all --syntax-only` exits 0 with 38 of 38 parsing
- [ ] CI runs that command and fails the build when it does not
- [ ] `08` / `09` reference only tables that exist in `init.sql`, and their
      shape matches the two-phase read in `assets::queries`
- [ ] No named placeholders remain, or the runner handles them
- [ ] README states the measured result, and the failures section is gone
- [ ] **Docs updated** — the endpoint-queries README per ADR 0032; no other
      architecture doc describes this tooling
