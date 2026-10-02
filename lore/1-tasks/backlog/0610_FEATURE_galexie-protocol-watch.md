---
id: '0610'
title: 'Galexie protocol watch: warn days before a pubnet vote that the pinned captive core cannot apply the protocol core already supports'
type: FEATURE
status: backlog
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

**The P29 stall (UTC).** Pubnet voted protocol 29 at 2026-10-01 17:00:07,
ledger 64,717,645. Galexie 28.0.1 exported up to 64,717,644 and then logged
`History: Skipping catchup: incompatible core version or invalid local state`.
`production-galexie-ingestion-lag` fired at 17:08:56; 29.0.0 was deployed at
06:27 the next morning (0605); the first P29 export landed at 07:04. Both the
explorer and stellar-prices-api went ~14 h without new ledgers, plus ~2.5 h
of catch-up. The ECS healthcheck (`pgrep -x stellar-core`) stayed green.

**The lead time this check would have had:**

- Horizon reported `core_supported_protocol_version = 29` from about
  2026-09-22 (stellar-prices-api's XDR watch opened its issue #336 that day).
- A Galexie image with captive core 29 existed from 2026-09-24: Docker Hub
  commit tag `c927ffc` has the same digest (`sha256:538ad0fb…`) as today's
  `29.0.0`, and its config label `org.opencontainers.image.created` is
  `2026-09-24T16:53:32Z`. The `29.0.0` tag itself was last pushed
  2026-10-01 20:38, after the vote.
- So: red from 09-22, actionable from 09-24 — about a week before the vote.

**Reading the pinned version — tags do not work, the image config does.**
`infra/envs/production.json → galexieImageTag` is the manifest-list digest
`sha256:5269dfd9…`. Docker Hub's tag list no longer maps any tag to it: `29.0.0`
was re-pushed as the single-platform manifest `sha256:538ad0fb…`. A
digest-to-tag lookup therefore fails on day one. What works, anonymously:

1. token: `https://auth.docker.io/token?service=registry.docker.io&scope=repository:stellar/stellar-galexie:pull`
2. `GET https://registry-1.docker.io/v2/stellar/stellar-galexie/manifests/<pinned digest>`
   with the manifest-list and image-manifest `Accept` types → the linux/amd64
   platform manifest
3. that manifest's config blob → `Env` →
   `STELLAR_CORE_VERSION=29.0.0-3589.4eb833373.noble` → major 29

Do not use `org.opencontainers.image.version`: it is `24.04`, the Ubuntu base.
If the pinned digest is not on Docker Hub (a self-built image) or carries no
`STELLAR_CORE_VERSION`, the check must fail as "cannot determine", never pass.

**Notification.** Slack is closed for GitHub workflows here, as recorded in
stellar-prices-api's `deploy-ledger-processor.md` ("Why not Slack"): the
workspace is at its installed-app limit, the Slack GitHub app needs an org
owner, and SNS → Chatbot needs AWS credentials. Oskar settled on issue +
email for the prices watch. Note that GitHub's scheduled-workflow failure
email reaches only whoever last edited the `cron:` line.

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
