---
id: '0610'
title: 'Galexie protocol watch: page when a Galexie with a newer captive core is on Docker Hub and ours is not on it'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0367', '0605', '0548', '0560']
tags: [galexie, ingestion, ci, resilience, priority-high, effort-small]
links:
  - https://github.com/rumblefishdev/stellar-prices-api/blob/master/.github/workflows/xdr-protocol-watch.yml
  - https://github.com/rumblefishdev/stellar-prices-api/blob/develop/tools/scripts/verify-xdr-protocol-gap.mjs
  - https://github.com/rumblefishdev/stellar-prices-api/blob/develop/docs/runbooks/xdr-protocol-watch.md
  - https://hub.docker.com/r/stellar/stellar-galexie/tags
history:
  - date: '2026-10-02'
    status: backlog
    who: stkrolikiewicz
    note: >
      Filed from the stellar-prices-api side after the protocol 29 stall. 0367
      listed a "Protocol-upgrade watch process" as a follow-up candidate
      pending confirmation; it was never created, and 0605 calls P29 the third
      occurrence of the 0367 failure mode. On Slack (2026-10-02) Oskar
      proposed putting the check in this repo, which owns the Galexie pin and
      the fix, and Stanisław agreed; Karol was asked for his view.
  - date: '2026-10-02'
    status: active
    who: karolkow
    note: 'Activated; building v1 (scheduled workflow + tracking issue).'
  - date: '2026-10-06'
    status: completed
    who: karolkow
    note: >
      Shipped in PR #605 and deployed: a Lambda every 3 h compares the
      running Galexie's captive core (ECR image config) with the newest
      Galexie on Docker Hub; alarm on its Errors to Slack. Test 7/7 (new),
      infra suite 33/33. First alarm message reached Slack 11:19 UTC; first
      invoke answered OK (core 29 = Hub 29).
---

# Galexie protocol watch

## Summary

Every pubnet protocol vote stops Galexie unless its captive core already
supports the new protocol — even when the vote changes no XDR (P29). Today
nothing notices until the ingestion-lag alarm fires after the vote. Add a
check that pages when a Galexie with a newer captive core is on Docker Hub
and ours is not on it, so the bump is a planned task before the vote instead
of an overnight outage.

## Stan teraz

- Done: deployed 2026-10-06 (Ingestion stack: one added export;
  CloudWatch stack: the watch). Alarm message seen in Slack; first run OK.
- In force: for a captive core the major version is the exact signal — a
  core that does not support the voted protocol always stalls.

## Context

The P29 stall timeline, the lead time this check would have had, how to
read the pinned captive-core version from the image config (not from tags),
and why delivery is an issue rather than Slack:
[notes/R-p29-stall-and-pinned-core-version.md](notes/R-p29-stall-and-pinned-core-version.md).

## Shape — decided 2026-10-05, reshaped 2026-10-06

A scheduled Lambda, not a GitHub workflow (the filed plan and why it was
dropped: [notes/R-github-workflow-plan-superseded.md](notes/R-github-workflow-plan-superseded.md)).

- Every 3 h it reads the image of the Galexie service's current task
  definition and that image's `STELLAR_CORE_VERSION` from its ECR config,
  and the same for the newest `stellar/stellar-galexie` tag on Docker Hub.
- Docker Hub's core major newer than ours, or any read that fails → the
  function throws; the log says NEW CORE (with the tag) or why it could not
  check.
- The alarm watches the function's built-in `Errors` (no custom metric):
  both runs of a 6 h period failed, missing data breaching → the existing SNS → Slack.
  It posts to the alarm channel; nobody is @-mentioned.
- Code: `infra/lambdas/galexie-protocol-watch/` (plain `.mjs`, AWS SDK from
  the Node 22 runtime), wired in `infra/src/lib/stacks/galexie-protocol-watch.ts`
  from `addIngestionAlarms` — only where our own Galexie runs.

**Why the image, not the network's readiness:** the first image with the
new core led every measured vote (P27 ~4 weeks, P28 ~4–6, P29 7 days) —
[notes/R-image-lead-time-p27-p29.md](notes/R-image-lead-time-p27-p29.md).
It also drops Horizon, which SDF is retiring in favour of Stellar RPC.
Rejected: two alarms ("bump available" / "cannot check"), judged not worth
the second signal for a page that comes weeks ahead.

## Acceptance Criteria

- [x] A scheduled Lambda compares the captive-core major of the image in the
      Galexie service's current task definition (its ECR config, not tags)
      with the newest Galexie image on Docker Hub
- [x] A newer core there, or a read that fails, is a failed run; never green
- [x] A test replays P29: Hub without a newer core → OK; commit tag
      `c927ffc` with core 29 against our 28 → NEW CORE; ours 29 → OK; unknown
      image or no `STELLAR_CORE_VERSION` → cannot determine
- [x] A run against production (read-only) answers OK, and the 27.0.0 image,
      absent from Docker Hub, reads as core 27 from ECR
- [x] Deployed; one alarm message seen in Slack (ADR 0054 rule 5)
- [x] A runbook says what to do per answer: `docs/runbooks/galexie-protocol-watch.md`
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      §6 (external dependencies), §8.2, §8.3; `docs/runbooks/health.md`
- [x] **API types regenerated** — N/A: nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**`

## Design Decisions

### From Plan

1. **Core version from the image config, not tags** — the pinned ECR digest
   is not on Docker Hub for 26.1.0 and 27.0.0.

### Emerged

2. **Lambda + alarm on its built-in `Errors`, not a GitHub workflow** — no
   master-branch copy, no cron delay, the existing Slack path, no metric.
3. **Docker Hub image instead of Horizon's core-supported protocol** — see
   Shape; the release tag and GitHub release of Galexie 29 came 3.5 h after
   the vote, so only the image contents (commit tag) gave a lead.
4. **Every 3 h, alarm when both runs of a 6 h period fail** — one run per
   period could leave a period empty, which counts as failed.
5. **Node `.mjs` Lambda, not Rust** — ~150 lines of HTTP reads; the repo
   already runs Node in the origin-lock canary.

## Issues Encountered

- **Missing export on deploy**: the watch's IAM statement imports the
  Galexie service ARN, which the Ingestion stack did not export yet; a
  CloudWatch-only deploy would have failed. Deployed Ingestion first
  (output only, no resource change), then CloudWatch, both with `-e`.

## Notes

- Known limits, not tasks: the container name `Galexie` and repo
  `<env>-galexie` are literals shared with `ingestion-stack.ts`; only the
  newest Docker Hub tag is read, so a later push of an older core hides a
  newer one until the next push; the check assumes core major = protocol.

- Not in scope: 0367's other open candidate, a ledger-advance healthcheck in
  place of `pgrep -x stellar-core`, which stayed green through this stall.
  It self-heals a stuck core after the fact; this task prevents the stall.
- Oskar suggested covering every Stellar dependency here (e.g. the
  `stellar-xdr` behind `xdr-parser`). Galexie first; stellar-prices-api
  already watches its own `stellar-xdr` pin (its task 0325 made a vote with
  no crate report WAITING instead of BEHIND).
- P29 changed no XDR: stellar-core v29 pins the same XDR commit as v28.0.1
  (`9c9c145`), so only the captive core had to move.
