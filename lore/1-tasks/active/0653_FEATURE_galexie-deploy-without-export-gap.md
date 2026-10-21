---
id: '0653'
title: 'Galexie deploy without an export gap: start the new task beside the old, stop the old once the new one exports'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0650', '0605', '0034']
tags: [ingestion, galexie, ecs, deploy, effort-small, priority-medium]
links:
  - 'https://github.com/stellar/stellar-galexie/blob/main/internal/uploader.go'
history:
  - date: '2026-10-21'
    status: active
    who: karolkow
    note: >
      Created after the 0650 deploy (Galexie 29 → 30): the export stopped for
      ~30 min while the new task caught up. Owner chose the rolling deploy
      gated on Galexie's own progress metric over documenting the gap.
---

# Galexie deploy without an export gap

## Summary

Every Galexie deploy stops the ledger export for the whole captive-core
catch-up: ECS stops the old task first (`minHealthyPercent: 0`,
`maxHealthyPercent: 100`), and the new one exports nothing until it has
downloaded and applied the bucket list. This task starts the new task beside
the old one and stops the old one only when the new one has exported (or
skipped) its first ledger.

## Stan teraz

- Done: decision (owner, 2026-10-21) — rolling deploy gated on Galexie's
  `uploader_latest_ledger` metric.
- Next: the infra PR; first proof is the next Galexie deploy.

## Context

Measured on the 0650 deploy, 2026-10-21 (UTC): last file from the old task
11:26:00 (ledger 65059235); new task started 11:27:09; history archive state
and 38 buckets downloaded by ~11:46; 15.01 GB bucket apply done 11:52:06;
then replay to the tip. `production-galexie-ingestion-lag` fired 11:34.

Why two Galexies at once are safe (upstream `internal/uploader.go`):

- In `append` mode without `overwrite`, every write is
  `PutFileIfNotExists`; a ledger the other task already wrote is
  "Skipped (already exists)". A skipped write creates no S3 object, so the
  indexer's doorbell does not ring twice.
- `galexie_uploader_latest_ledger` is set after every upload **and** every
  skip (`uploader.go:139`), and stays 0 until the first one. It is served on
  `/metrics` of `admin_port` (`internal/app.go:287-290`; off when unset).

Why the current health check cannot gate this: `pgrep -x stellar-core` turns
healthy ~1 min after start, long before the export resumes, so ECS would
stop the old task as early as today.

## Implementation Plan

1. `ingestion-stack.ts`, live service: `minHealthyPercent: 100`,
   `maxHealthyPercent: 200`.
2. Galexie TOML: `admin_port`. Live health check: `stellar-core` running
   **and** `galexie_uploader_latest_ledger` > 0 on `localhost/metrics`.
   Timing at the ECS maxima (`startPeriod` 300 s, `interval` 300 s,
   `retries` 10): up to 55 min to turn healthy, against ~30 min measured.
3. Infra test asserting both.
4. `docs/deployment.md` Galexie recipe: two tasks overlap during a deploy;
   no lag alarm expected.

## Acceptance Criteria

- [ ] `cdk diff` on production: only the live service's deployment
      configuration and the live task definition (health check, entrypoint).
- [ ] Next Galexie deploy: no gap in the ledger bucket's `LastModified`
      sequence, `galexie-ingestion-lag` stays OK, the old task stops only
      after the new one logs "Skipped … (already exists)" or "Uploaded".
- [ ] **Docs updated** — `docs/deployment.md` (Galexie recipe);
      `docs/architecture/**` checked for the rollout description.
- [ ] **API types regenerated** — N/A: no `crates/api`, `Cargo.*` change.

## Notes

- Steady-state cost of the slower health check: a task whose core died
  turns unhealthy after up to 50 min instead of 1.5 min. Galexie exits when
  its captive core does (the task then stops on its own), and the lag alarm
  pages after minutes either way.
