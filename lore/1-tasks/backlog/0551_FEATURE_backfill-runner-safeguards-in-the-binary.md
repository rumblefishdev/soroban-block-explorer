---
id: '0551'
title: 'FEATURE: backfill-runner carries its own safeguards — no hand-rolled wrapper per run'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0540', '0279', '0322', '0145', '0542', '0541']
tags: [priority-medium, effort-medium, layer-backfill, ops, data-integrity]
links:
  - crates/backfill-runner/src/run.rs
  - crates/backfill-runner/src/sync.rs
  - docs/runbooks/backfill_derived_table_reparse_hetzner.md
  - docs/backfills.md
history:
  - date: 2026-09-14
    status: backlog
    who: karolkow
    note: >
      Filed from the 0540 value-flow map (ticket T12, raised 2026-09-07 while
      preparing the full-range backfill). Every historical re-parse runs the
      binary inside a shell wrapper whose four safeguards exist only in a
      runbook; a proposed simplification of that wrapper would have re-created
      a gap-risking failure.
---

# backfill-runner carries its own safeguards

## Summary

Running a historical re-parse needs a ~30-line shell wrapper around
`backfill-runner`, and each part of it is a remedy for a specific failure. The
wrapper is re-derived from a runbook for every run, on production, under time
pressure. Move the safeguards into the binary so a run is safe by construction.

## Context

Found while preparing the 0540 backfill: a simplification of the wrapper was
proposed and withdrawn, because it would have re-created the `channel closed`
failure. The knowledge that a 64 000-ledger run is unsafe lives in the
runbook's troubleshooting table
(`docs/runbooks/backfill_derived_table_reparse_hetzner.md`), not in the code.
The next runs that will need the wrapper are already known: the re-parse after
0542's decoder fix and 0541's fold.

| Failure                                                                                                             | Remedy today                                                                                                  | Lives in |
| ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | -------- |
| `channel closed` on dense partitions — one 64 000-ledger run outlasts the ClickHouse insert channel; **risks gaps** | slice into 16 000-ledger sub-windows, pass `--keep-partitions` so the downloaded folder survives the sub-runs | shell    |
| an index or persist failure aborts the whole run                                                                    | a 3-attempt retry loop around the binary (the binary retries only the S3 sync)                                | shell    |
| a targeted (`--only`) run cannot resume — it writes no `ledgers` marker, so the resume filter is bypassed           | a watermark file per worker; sub-windows below it are skipped                                                 | shell    |
| the binary calls the AWS CLI by bare name and dies under `nohup` / `tmux`, where `PATH` is trimmed                  | `export PATH` in the wrapper                                                                                  | shell    |

The runbook's original loop let the watermark advance past a failed window, so
a failure silently became a hole; the 0540 wrapper added "fail the run after
three tries" on top.

## Implementation

- **Chunk internally.** The sub-window is a property of the insert channel, not
  of the operator's intent: commit on a bound the binary chooses, keep the
  partition folder until its last chunk is in. `--keep-partitions` stops being
  needed for this.
- **Retry the index step** with the budget and backoff the S3 sync already has;
  when exhausted, fail the run — never advance past the window.
- **Resume a targeted run.** Either a small progress table keyed by (run label,
  range), or derive the resume point from the targeted tables themselves.
  Decide which; the second needs no new table but must not treat a
  partly-written window as done.
- **AWS CLI dependency:** check for it at start and fail with the remedy in the
  message, or drop the subprocess.

Out of scope: a worker pool and splitting a range across workers by weight
(0145; the weight rule is in `docs/backfills.md` §2).

## Measured context (2026-09-07)

One partition of 38 576 ledgers, six targeted tables: parse 70 s, persist
955 s, download ~455 s, 15.7 M edges, zero duplicate keys on a deliberate
re-run. Partition sizes across the range sample at 6.8–11.7 GiB.

## Acceptance Criteria

- [ ] A single `run` over a full dense partition completes without
      `channel closed` and without an operator-chosen sub-window
- [ ] An index or persist failure is retried, and an exhausted retry fails the
      run with the failing range named — no window is skipped
- [ ] An interrupted `--only` run restarted with the same arguments resumes at
      the first incomplete window, proven by a kill-and-restart test
- [ ] Starting without the AWS CLI on `PATH` fails immediately with the remedy
- [ ] The runbook's wrapper is reduced to the bare invocation, and the
      troubleshooting rows it replaces point at this behaviour
- [ ] **Docs updated** — `docs/backfills.md`,
      `docs/runbooks/backfill_derived_table_reparse_hetzner.md`;
      `docs/architecture/**` N/A unless the progress table is added (then
      `database-schema/**`)
