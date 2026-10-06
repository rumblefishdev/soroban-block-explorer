---
id: '0610'
title: 'Galexie protocol watch: warn days before a pubnet vote that the pinned captive core cannot apply the protocol core already supports'
type: FEATURE
status: active
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

- Done: the Lambda (ECR core vs Docker Hub core), its alarm, test and
  runbook in PR #605; reshaped 2026-10-06 (see Shape).
- Next: merge, deploy `Explorer-production-CloudWatch`, one test message
  through Slack.
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

**Why the image, not the network's readiness (2026-10-06).** The first
version compared our core with Horizon's `core_supported_protocol_version`.
A review found its one page said "the network is ready" while the moment
someone can act — the image appearing — reached no one. The image date is
that moment, and it led every measured vote (first image with the new core
on Docker Hub, read from the registry 2026-10-06):

| Protocol | First image with the new core                       | Pubnet vote              | Lead       |
| -------- | --------------------------------------------------- | ------------------------ | ---------- |
| 27       | 2026-06-10 (`c97d648`, `27.0.0`)                    | 2026-07-08               | ~4 weeks   |
| 28       | 2026-08-03 (`e746a5b`, pre-release; `28.0.0` 08-14) | mid-September (estimate) | ~4–6 weeks |
| 29       | 2026-09-24 (`c927ffc`, later tagged `29.0.0`)       | 2026-10-01               | 7 days     |

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
- [ ] Deployed; one alarm message seen in Slack (ADR 0054 rule 5)
- [x] A runbook says what to do per answer: `docs/runbooks/galexie-protocol-watch.md`
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      §6 (external dependencies), §8.2, §8.3; `docs/runbooks/health.md`
- [x] **API types regenerated** — N/A: nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**`

## Notes

- Not in scope: 0367's other open candidate, a ledger-advance healthcheck in
  place of `pgrep -x stellar-core`, which stayed green through this stall.
  It self-heals a stuck core after the fact; this task prevents the stall.
- Oskar suggested covering every Stellar dependency here (e.g. the
  `stellar-xdr` behind `xdr-parser`). Galexie first; stellar-prices-api
  already watches its own `stellar-xdr` pin (its task 0325 made a vote with
  no crate report WAITING instead of BEHIND).
- P29 changed no XDR: stellar-core v29 pins the same XDR commit as v28.0.1
  (`9c9c145`), so only the captive core had to move.
