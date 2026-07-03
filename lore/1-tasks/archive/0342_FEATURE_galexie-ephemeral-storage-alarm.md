---
id: '0342'
title: 'CloudWatch alarm: Galexie captive-core ephemeral storage utilization > 60%'
type: FEATURE
status: completed
related_adr: []
related_tasks: []
tags: ['phase-observability', 'effort-small', 'priority-medium', 'infra']
links: []
history:
  - date: 2026-07-02
    status: active
    who: fmazur
    note: 'Task created — spun up after the 01–02.07.2026 Galexie disk-full deadlock incident.'
  - date: 2026-07-02
    status: active
    who: fmazur
    note: 'Alarm implemented + Slack IDs moved to SSM (emerged). Deployed to prod; Slack delivery verified end-to-end via set-alarm-state (ALARM + OK both posted). Real ephemeral util ~30%.'
  - date: 2026-07-02
    status: completed
    who: fmazur
    note: 'Completed. 3 infra files changed, 0 tests. Emerged: Slack IDs → SSM (fixes latent placeholder channelId affecting all 6 alarms).'
---

# CloudWatch alarm: Galexie captive-core ephemeral storage utilization > 60%

## Summary

Add a CloudWatch alarm that fires when the Galexie live ECS/Fargate task's
ephemeral-storage utilization sustains above 60%, wired to the existing
SNS→Slack alarm topic. Gives multi-month lead time to plan a disk bump before a
merge/catchup spike hits the "No space left on device" ceiling that caused the
01–02.07.2026 ingestion outage.

## Status: Completed

**Current state:** Deployed to production and verified — SSM params set, `cdk deploy` applied, and Slack delivery confirmed end-to-end via `set-alarm-state` (both ALARM and OK posted to the channel; real utilization ~30%). Pending: commit + PR + merge to `develop` so prod == develop (deployed from the branch).

## Context

On 30.06→01.07.2026 Galexie stopped writing ledger XDR to
`production-stellar-ledger-data`. Root cause: captive-core's ephemeral disk
(30 GiB, below Stellar's 100 GB spec) filled up — `gzip: buckets/tmp/catchup-*:
No space left on device` — so catchup could never complete, its temp was never
cleaned (cleanup runs only on success), and the task deadlocked while the
`pgrep stellar-core` healthcheck kept reporting healthy. Fixed operationally by
bumping `galexieEphemeralStorage` 30 → 100 GiB.

The ~30 GiB resident footprint is captive-core's BucketList = the current live
ledger state (independently corroborated: Stellar validator docs cite
BucketListDB at 20–40 GB; bottom-up from CAP-0057's ~47M entries ≈ 19–33 GB).
It grows slowly with network state, so a static disk will eventually refill.
There was no alarm on ephemeral-storage utilization — the fill-up was invisible
until exports silently stopped. This task closes that gap.

Analysis writeup lives (uncommitted) at `.temp/README.md`.

## Implementation Plan

### Step 1: config field + validation (`infra/src/lib/types.ts`)

- Add `readonly galexieEphemeralUtilizationThreshold: number;` to
  `EnvironmentConfig` in the "Observability — CloudWatch alarms" block.
- Add a `validateConfig` check: must be `> 0 && <= 100`.

### Step 2: config value (`infra/envs/production.json`)

- Add `"galexieEphemeralUtilizationThreshold": 60`.

### Step 3: alarm (`infra/src/lib/stacks/cloudwatch-stack.ts`)

- New alarm (grouped with the existing Galexie lag alarm), metric-math
  `(used / reserved) * 100` over Container Insights
  `EphemeralStorageUtilized` / `EphemeralStorageReserved`
  (`ECS/ContainerInsights`, dims `ClusterName=${env}-ingestion`,
  `ServiceName=${env}-galexie-live`), MAXIMUM stat, 5-min period.
- `evaluationPeriods: 3`, `datapointsToAlarm: 3` (sustained ~15 min — avoids
  paging on a transient merge spike; baseline is ~30%).
- Threshold from `config.galexieEphemeralUtilizationThreshold`.
- `treatMissingData: NOT_BREACHING`; wired via the existing `withActions`
  helper (SNS→Slack alarm + OK actions).

### Step 4 (emerged): Slack IDs → SSM

- Remove `slackWorkspaceId` / `slackChannelId` from `EnvironmentConfig` and
  their `validateConfig` blocks; delete the literal IDs from `production.json`
  (must not live in the public repo).
- `CloudWatchStack` reads both from SSM at deploy via
  `ssm.StringParameter.valueForStringParameter`, names
  `/soroban-explorer/${env}/slack-workspace-id` and `.../slack-channel-id`.

Prerequisite before deploy — set the params once (out-of-band):

```bash
# real values come from Slack / `aws chatbot describe-slack-workspaces` —
# deliberately NOT stored in the repo
aws ssm put-parameter --region eu-central-1 --type String \
  --name /soroban-explorer/production/slack-workspace-id --value '<workspace-id>'
aws ssm put-parameter --region eu-central-1 --type String \
  --name /soroban-explorer/production/slack-channel-id --value '<channel-id>'
```

Deploy: `make -C infra deploy-production-cloudwatch`.

## Acceptance Criteria

- [x] `galexieEphemeralUtilizationThreshold` added to `EnvironmentConfig` +
      validated (0 < x ≤ 100).
- [x] `production.json` sets the threshold to `60`.
- [x] Alarm created on ephemeral-storage utilization %, config-driven threshold,
      wired to the existing SNS→Slack topic via `withActions`.
- [x] Metric-math on `%` (robust to future disk-size changes), sustained
      3×5 min window.
- [x] `cdk synth` for the CloudWatch stack succeeds (or lint/build passes).
- [x] **Docs updated** — N/A: observability alarm only; no change to schema, API, ingestion pipeline, or infra topology (ADR 0032).
- [x] **API types regenerated** — N/A: infra-only; no changes under crates/api, Cargo.{toml,lock}, or libs/api-types.
- [x] Slack workspace/channel IDs sourced from SSM; removed from `types.ts` and `production.json` (no IDs in repo).
- [x] SSM params set in AWS, `cdk deploy` applied, Slack delivery verified via `set-alarm-state` (ALARM + OK both posted).

## Implementation Notes

Files changed (branch `feat/0342_galexie-ephemeral-storage-alarm`, infra-only):

- `infra/src/lib/stacks/cloudwatch-stack.ts` — new `GalexieEphemeralStorageAlarm`
  (metric-math `(EphemeralStorageUtilized / EphemeralStorageReserved) * 100`,
  MAXIMUM, 5-min, sustained 3×3); Slack workspace/channel IDs now read from SSM
  via `valueForStringParameter`.
- `infra/src/lib/types.ts` — added `galexieEphemeralUtilizationThreshold`
  (+ validation); removed `slackWorkspaceId` / `slackChannelId` fields and their
  validation.
- `infra/envs/production.json` — `galexieEphemeralUtilizationThreshold: 60`;
  removed the literal Slack IDs.

Verified: `tsc --noEmit` clean, prettier clean, `cdk deploy` applied, Slack
delivery confirmed end-to-end (ALARM + OK both posted; real utilization ~30%).
SSM params set: `/soroban-explorer/production/slack-workspace-id` and
`/soroban-explorer/production/slack-channel-id`.

## Issues Encountered

- **Alarm→Slack had never worked**: the committed `slackChannelId` was a
  placeholder value, so all six pre-existing alarms silently delivered
  nowhere. Root cause, not a regression — fixed by moving real IDs to SSM.
- **Prettier 2.8.8 non-idempotent** on the task md: `**` glob patterns inside a
  wrapped inline-code span made `--write` and `--check` disagree. Fixed by
  rephrasing the acceptance-criteria lines without `**` globs.

## Design Decisions

### From Plan

1. **Metric-math on % (not a static GB threshold)**: `(used / reserved) * 100`
   so the alarm survives future ephemeral-disk resizes without a code change.
2. **Sustained 3×5 min window**: baseline is ~30%, so 60% is a plan-ahead
   threshold; the window avoids paging on a transient merge spike.

### Emerged

3. **Slack workspace/channel IDs moved to SSM**: while wiring the Slack test we
   found the committed `slackChannelId` was a placeholder, and the real IDs
   would otherwise be published in a public repo. They are identifiers, not
   credentials, but there is no reason to expose them — so they now come from
   SSM Parameter Store (plain String) at deploy, removed from `types.ts` /
   `production.json`. This also fixes a latent bug: the alarm→Slack path had
   never delivered (placeholder channel), affecting all six pre-existing
   alarms, not just this one.

## Notes

- Alert destination reuses the existing `${env}-soroban-explorer-alarms` SNS
  topic → AWS Chatbot → Slack; no new notification wiring.
- Considered but NOT pursued: persistent/resizable `storage_path` (restarts
  skip full re-catchup) and a progress-based healthcheck. Deprioritised — the
  disk-full failure mode is covered by the 100 GiB headroom + this 60% alarm;
  restart-lag was deemed not worth the architecture change (ECS-on-EC2 + EBS)
  for now.
