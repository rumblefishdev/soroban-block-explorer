---
id: '0480'
title: 'RESEARCH: which tests actually run — silent skips, CI execution, coverage gaps'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0478']
tags: [testing, ci, clickhouse, audit, priority-medium, effort-medium]
links: []
history:
  - date: '2026-08-13'
    status: backlog
    who: karolkow
    note: >
      Split out of 0478. Verifying a small feature turned up a suite that
      reports success for work it never does: eleven ClickHouse-backed tests
      return early in CI because no workflow sets the variable that enables
      them, a second family of tests is gated on a DIFFERENT variable name for
      the same database, and 29 more carry `#[ignore]`. None of this is
      visible in a green build. The question is bigger than one endpoint's
      tests, so it gets its own task instead of riding along with the SQL-gate
      repair.
---

# RESEARCH: which tests actually run

## Why

A green build currently means "nothing that ran failed", not "the suite ran".
The difference is invisible from the outside, which is the dangerous part: a
test that skips looks exactly like a test that passes.

This is an audit task. Its output is a map of what executes where, plus a
ranked list of what to fix — not a sweep of fixes.

## What is already known (measured 2026-08-13, do not re-derive)

**Eleven tests gated on `CH_URL`, skipping in CI.** Across
`crates/api/src/{ledgers,liquidity_pools,network,nfts,search,transactions}`.
Each begins by reading `CH_URL`; unset means an early `return` and a pass. No
workflow ever set it — `CH_URL` first appears in `.github/` only in the
(closed) 0478 branch. The container to run them against has existed in
`docker-compose.yml` since 2026-05-10; it was simply never wired in.

**A second family, on a different variable.** `db-clickhouse`,
`backfill-runner`, `enrichment-shared`, `enrichment-worker` and
`backfill-enrichment-runner` read **`CLICKHOUSE_URL`** — the same database
under another name. Two conventions for one thing, and the second is still
entirely unreached.

Measured with the variable set against a schema-only container:

| crate             | result                                                                                                         |
| ----------------- | -------------------------------------------------------------------------------------------------------------- |
| `db-clickhouse`   | green — 90 unit tests plus four e2e suites (persist, metadata, g9 routing, lp amounts)                         |
| `backfill-runner` | 53 pass, **1 fails**: `repair_tier1::…_dry_run_leaves_live_untouched`, `Table default.accounts does not exist` |

So there is real coverage asleep here, and one genuine defect behind it.

**Three `api` tests need data, not just a schema.** The LP asset-code smokes
assert on real `USDC` and native-XLM pools and fail with "matched no pool"
against an empty instance.

**29 `#[ignore]` tests** in nine files — six needing the AWS public-blockchain
archive, one live mainnet RPC, the rest a populated local ClickHouse. Each
reason is written in the code, so these are decisions rather than neglect, but
nobody has counted them before.

**Two file-gated tests in `xdr-parser`** — one skips without a `.temp/`
directory, one without `NFT_CORPUS`.

**`cargo test` runs twice in CI.** The `Rust (clippy, test)` job, and again
inside the `TypeScript` job via the nx graph's `rust:test` target
(`cargo test --workspace`). Several minutes per build, and it makes a job's
name describe something other than what it does.

## Questions to answer

1. Which tests execute in CI today, per crate? The answer should be a count
   that someone can check, not an impression.
2. Which skip, and on what — an env var, a fixture file, `#[ignore]`?
3. Where does a skip hide a defect rather than an absent dependency? One is
   known (`repair_tier1`); assume there are more behind the untouched
   `CLICKHOUSE_URL` family.
4. Should `CH_URL` and `CLICKHOUSE_URL` be one variable? Two names for one
   database is how half a suite goes unnoticed.
5. What is worth fixture data, and what should stay a decode smoke? Fixtures
   are the single highest-leverage step — they unblock the three LP tests,
   give the other eight something to assert, and let the ignored CH tests run.
6. Should the `TypeScript` job stop running `cargo test`?
7. Where is coverage genuinely missing, as opposed to present-but-skipped?

## Deliverable

A written map (crate → what runs, what skips, why) plus a ranked list of
changes with their cost. Fixes land as their own tasks, not inside this one.

## Prior art in this repo

`refactor/0478_tier1-gate-repair` (PR 404, closed unmerged) already carries a
working version of two answers, if they turn out to be the right ones:

- the `rust` CI job starting ClickHouse from `docker-compose.yml` — reusing the
  existing definition rather than a second `services:` block — and exporting
  `CH_URL`
- a `CH_REQUIRED` marker so a job that promised a database fails loudly when it
  no longer has one, while jobs with no reason to have one still skip quietly

Both were verified green in CI before the branch was parked.
