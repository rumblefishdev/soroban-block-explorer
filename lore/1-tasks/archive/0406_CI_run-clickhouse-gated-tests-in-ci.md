---
id: '0406'
title: 'CI: actually run the ClickHouse-gated tests — 25 files of e2e that no pipeline has ever executed'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0394', '0388', '0392', '0304', '0455']
tags: [priority-high, effort-small, area-ci, clickhouse, robustness]
links:
  - .github/workflows/ci.yml
history:
  - date: 2026-07-17
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned while closing 0394, whose AC2 ("all three CH-gated e2e seed and pass")
      had to be satisfied by hand because nothing else can. Verified 2026-07-17:
      **zero references to clickhouse in any file under .github/workflows/**, while
      25 files under crates/ carry `CLICKHOUSE_URL`-gated tests. This is the root
      cause of the 0388 / 0394 / 0392-#341 family — the tests that would have caught
      the stale `name` column exist, and have never run.
  - date: 2026-08-06
    status: completed
    who: karolkow
    note: >
      Closed under umbrella 0455 (defect 1, declared-vs-actual). CI rust job
      gains a ClickHouse leg: compose-provisioned 26.3 + init.sql sidecar,
      both gates (env-var self-skip + name-filtered CH-only #[ignore]d),
      count guard, --no-fail-fast. Full evidence chain run locally against a
      fresh 26.3.17.4 container: 90+6 db-clickhouse, 54 backfill-runner,
      5 ignored all green; sc.name sabotage turns the step red (Code 47,
      exit 101) and with --no-fail-fast all 9 test binaries still report.
      Deliberately NOT verified on a real Actions runner — accepted risk,
      first PR touching crates/** exercises it.
---

# CI: actually run the ClickHouse-gated tests

## Summary

The repo has a substantial ClickHouse e2e suite that **no pipeline has ever
executed**. The tests are gated on `CLICKHOUSE_URL`; CI never sets it and never
provisions a server, so every one of them skips — silently and green.

## Context

Spawned from [0394](./0394_BUG_backfill-runner-stale-name-column-sweep.md).
The pattern that task swept up is the cost of this gap, stated plainly:

- 0304 dropped the `name` column from `soroban_contracts` / `assets`.
- Broken SQL then reached **prod** and aborted real maintenance passes — 0388's
  `repair-tier1` deployer reconstruction died on `unknown column name`, and 0392's
  live indexer threw `Code 47` (20,494 failures in 7 days).
- Four separate PRs each fixed one copy. The e2e that would have caught all of
  them on the first PR **were already written** — they just never ran.

A green "Rust (clippy, test)" check is actively misleading here: it passes while
skipping every test that touches the database. PR #342's green CI said nothing at
all about the ACs it was supposed to satisfy.

## Implementation

- [x] Add a ClickHouse service to the Rust CI job (image pinned to the prod major
      — **26.3**) and set `CLICKHOUSE_URL`. Done via the dev `docker-compose.yml`
      (same image pin, same config mounts) rather than a GH `services:` block.
- [x] Apply `crates/db-clickhouse/schema/init.sql` before the suite — the
      compose `db-clickhouse-init` sidecar does it, via
      `docker compose run --rm db-clickhouse-init`.
- [x] Handle the **two different gates**: the env-var self-skip suites run in a
      dedicated step with `CLICKHOUSE_URL` set; the `#[ignore]`d tests run with
      `--ignored`, **name-filtered** (`select_sep1_chunk select_nft_chunk
filter_drops`) to the five CH-only ones — the other `#[ignore]`s need live
      mainnet RPC / S3 / issuer TOML and stay manual.
- [x] Make a skip **visible**: a count guard sums `N passed` across the
      `--ignored` leg and fails the step below 5, so a drifted name filter turns
      red instead of matching nothing and passing green.
- [x] Confirm isolation before wiring it up: CI's server is per-job throwaway,
      which satisfies the enrichment tests' `ALTER TABLE … DELETE` cleanup
      concern. `--test-threads=1` on both legs — the suites seed shared tables
      with sentinel rows and race each other otherwise.

## Acceptance Criteria

- [x] CI provisions ClickHouse and runs the gated suite, including the CH-only
      `#[ignore]`d tests, on every PR touching `crates/**` (also
      `docker-compose.yml`, added to the rust paths-filter). Wired and verified
      locally; first real Actions run pending by design (see history).
- [x] A deliberately broken column reference makes the run **red** — verified by
      re-introducing `sc.name` in `contract_type_rebuild`: `Code: 47
UNKNOWN_IDENTIFIER`, exit 101, test named with file:line and full SQL in
      the log. Verified twice (fail-fast and `--no-fail-fast` variants).
- [x] The run is visible in the job log: per-test lines plus per-binary
      `test result:` counts (90+6 db-clickhouse, 54 backfill-runner, 5 ignored),
      and the guard prints the summed ignored-leg count.
- [x] Docs updated — `N/A` (CI tooling; CLAUDE.md names this a legitimate N/A case).
- [x] API types regenerated — N/A (no `crates/api/**` change).

## Implementation Notes

One file changed: `.github/workflows/ci.yml` (+41 lines). Two steps appended to
the `rust` job (`Start ClickHouse`, `ClickHouse e2e`) and one paths-filter entry
(`docker-compose.yml`). No new workflow, no new dependency — the compose file CI
uses is the one local dev already runs.

## Design Decisions

### From Plan

1. **Image pinned to prod major 26.3** — inherited from the compose file, which
   already pins it; CI reuses the pin instead of restating it.

### Emerged

2. **Compose over GH `services:`** — the task suggested "add a ClickHouse
   service"; using `docker compose` instead means the XML config mounts
   (timeouts, RBAC users, dict source) and the schema sidecar come for free and
   cannot drift from local dev.
3. **Name-filtered `--ignored` instead of blanket `--ignored`** — the task
   grouped all `#[ignore]`s as CH-gated; inspection found most need live
   network (mainnet RPC, S3 archive, issuer TOML). Blanket `--ignored` would
   have made CI flaky on third-party endpoints. Filter + count guard instead.
4. **Separate step rather than exporting `CLICKHOUSE_URL` job-wide** — keeps
   the main `cargo test` exactly as before (skips stay skips there) and gives
   the CH suite its own visible, individually-red step; also avoids the env var
   leaking into unrelated tests (`api` reads it at runtime config).
5. **`--no-fail-fast`** (user-requested) — one red suite no longer hides the
   rest; verified all 9 binaries still report with a failure present.
6. **Closed on local evidence, no real-runner run** — user decision 2026-08-06:
   local red+green chain accepted, the arm64 runner + compose assumption is
   exercised by the first PR touching `crates/**`.

## Issues Encountered

- **`docker compose up --wait` unsuitable for the sidecar** — a one-shot
  service that exits confuses `--wait`; `docker compose run --rm
db-clickhouse-init` blocks until the schema is applied and propagates the
  exit code, so it is the deterministic form.
- **GH default shell lacks `pipefail`** — the guard pipes `cargo test | tee`;
  without `shell: bash` a cargo failure would be masked by `tee`'s exit 0.
  Step sets `shell: bash` explicitly.
- **First verification attempt ended in a full disk** (2026-08-05): image +
  volume + a full-workspace test log on a 434/460 GiB disk. Second run
  (2026-08-06) completed after cleanup; also surfaced a Docker Desktop
  self-update wedged by the disk-full, which had to be force-killed.
