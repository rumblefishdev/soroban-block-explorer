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

- Done: nothing yet; filed with the P29 measurements below.
- Next: build v1 (a scheduled workflow + one tracking issue); v2 is optional.
- In force: for a captive core the protocol number is the exact signal, not a
  proxy — a core that does not support the voted protocol always stalls, so
  there is no "waiting" tier.

## Context

The P29 stall timeline, the lead time this check would have had, how to
read the pinned captive-core version from the image config (not from tags),
and why delivery is an issue rather than Slack:
[notes/R-p29-stall-and-pinned-core-version.md](notes/R-p29-stall-and-pinned-core-version.md).

## Implementation Plan

### Step 1: The check (v1)

A dependency-free script plus a scheduled workflow (daily + `workflow_dispatch`),
modelled on stellar-prices-api's `verify-xdr-protocol-gap.mjs` and
`xdr-protocol-watch.yml`. Horizon `current_protocol_version` and
`core_supported_protocol_version` against the captive-core major read as above:

| Tier     | Condition                                 | Run   | Issue                                   |
| -------- | ----------------------------------------- | ----- | --------------------------------------- |
| OK       | core major ≥ core supports                | green | an open one is closed                   |
| LAGGING  | core major < core supports, not yet voted | red   | opened: "bump Galexie before the vote"  |
| BEHIND   | core major < mainnet current              | red   | opened, or LAGGING escalated by comment |
| NO CHECK | Horizon or the registry unreadable        | red   | untouched                               |

LAGGING should say whether Docker Hub already has an image whose
`STELLAR_CORE_VERSION` reaches the new protocol, so the issue tells the reader
if the bump is possible today.

### Step 2: Delivery

One tracking issue, opened once, body refreshed silently, a comment only on
LAGGING → BEHIND, closed on OK (the prices watch's lifecycle). Assign it, or
@-mention the Galexie owners in it, so the notification reaches people and not
only the cron editor's inbox.

### Step 3: Branches

The default branch is `master`, and a `schedule` trigger only runs the
default branch's copy. Put the workflow on `master` and let it check out
`develop` for the script and `production.json`, as the prices watch does.
GitHub starts these crons 5–8 h late (the prices watch: cron 06:17, actual
starts 11:25–13:56 UTC) — fine for a week of lead time.

### Step 4 (optional, v2): read production, notify Slack

A scheduled Lambda beside the existing probes reads the running
`production-galexie-live` task definition's image instead of the pinned
digest (production, not intent: on 2026-10-02 29.0.0 was deployed at 06:27
and PR #590 put it on `develop` at 07:12), publishes a metric, and an alarm goes through the existing
SNS → AWS Chatbot → Slack path (the Chatbot Slack workspace is already
authorized). A CDK change, a deploy and a `docs/architecture/**` update.

## Acceptance Criteria

- [ ] A daily scheduled workflow (and `workflow_dispatch`) compares Horizon's
      current and core-supported protocols with the captive-core major of the
      pinned Galexie digest, read from the image config, not from tags
- [ ] OK / LAGGING / BEHIND / NO CHECK as in the table; there is no
      waiting tier
- [ ] A digest that is not on Docker Hub, or an image without
      `STELLAR_CORE_VERSION`, fails as "cannot determine" — never green
- [ ] One tracking issue with the lifecycle above, reaching the Galexie
      owners directly (assignee or @-mention)
- [ ] A test replays the P29 timeline against a mock Horizon and registry:
      pin 28.0.1 with core 29 → LAGGING; with mainnet 29 → BEHIND; pin 29.0.0
      → OK; unknown digest → NO CHECK
- [ ] A runbook says what to do per tier (the Galexie recipe in
      `docs/deployment.md` already covers the bump itself)
- [ ] **Docs updated** — `docs/architecture/**` only if v2 (Step 4) is built;
      v1 adds a workflow and a script, not a change to the system's shape
- [ ] **API types regenerated** — N/A: nothing under `crates/api/**`,
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
