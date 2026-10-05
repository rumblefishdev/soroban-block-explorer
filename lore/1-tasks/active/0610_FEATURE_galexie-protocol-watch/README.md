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
daily check that compares Horizon's `core_supported_protocol_version` with the
protocol the pinned Galexie image's captive core supports, so the bump is a
planned task days before the vote instead of an overnight outage.

## Stan teraz

- Done: the Lambda, its alarm, test and runbook on
  `feat/0610-galexie-protocol-watch`; `cdk diff` adds resources only.
- Next: PR, then a deploy of `Explorer-production-CloudWatch` and one test
  message through Slack.
- In force: for a captive core the protocol number is the exact signal, not a
  proxy — a core that does not support the voted protocol always stalls, so
  there is no "waiting" tier.

## Context

The P29 stall timeline, the lead time this check would have had, how to
read the pinned captive-core version from the image config (not from tags),
and why delivery is an issue rather than Slack:
[notes/R-p29-stall-and-pinned-core-version.md](notes/R-p29-stall-and-pinned-core-version.md).

## Shape — decided 2026-10-05

A scheduled Lambda, not a GitHub workflow (the filed plan and why it was
dropped: [notes/R-github-workflow-plan-superseded.md](notes/R-github-workflow-plan-superseded.md)).

- Every 30 min it reads the image of the Galexie service's current task
  definition, that image's `STELLAR_CORE_VERSION` from its ECR config, and
  Horizon's `current_protocol_version` / `core_supported_protocol_version`.
- Our core older than either, or any read that fails → the function throws;
  the log line says LAGGING (vote ahead, bump now), BEHIND (voted) or why it
  could not check, plus whether Docker Hub's newest tag already has the core.
- The alarm watches the function's built-in `Errors` (no custom metric):
  both runs of an hour failed, missing data breaching → the existing
  SNS → Slack. Docker Hub is asked only when the core is not ready (anonymous
  pull limits per IP).
- One alarm, no tiers: BEHIND is also paged within minutes by
  `galexie-ingestion-lag`. It posts to the alarm channel; nobody is
  @-mentioned (the filed plan's assignee went with the GitHub issue).
- Code: `infra/lambdas/galexie-protocol-watch/` (plain `.mjs`, AWS SDK from
  the Node 22 runtime), wired in `infra/src/lib/stacks/galexie-protocol-watch.ts`
  from `addIngestionAlarms` — only where our own Galexie runs.

## Acceptance Criteria

- [x] A scheduled Lambda compares Horizon's current and core-supported
      protocols with the captive-core major of the image in the Galexie
      service's current task definition, read from its config in ECR, not
      from tags
- [x] Not OK, or not checkable, is a failed run; never green
- [x] A test replays the P29 timeline with fake registries: core 28 with the
      network core at 29 → LAGGING; voted → BEHIND; core 29 → OK; unknown
      image or no `STELLAR_CORE_VERSION` → cannot determine
- [x] A run against production (read-only, 2026-10-05) answers OK, and the
      27.0.0 image, absent from Docker Hub, reads as core 27 from ECR
- [ ] Deployed; one alarm message seen in Slack (ADR 0054 rule 5)
- [x] A runbook says what to do per answer: `docs/runbooks/galexie-protocol-watch.md`
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      §8.2 / §8.3, `docs/runbooks/health.md` (matrix + symptom)
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
