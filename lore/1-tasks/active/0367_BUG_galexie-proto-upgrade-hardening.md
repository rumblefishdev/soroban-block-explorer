---
id: '0367'
title: 'Galexie protocol-upgrade hardening: durable 27.0.0 pin + fix silent ingestion-lag alarm'
type: BUG
status: active
related_adr: []
related_tasks: []
tags: ['effort-small', 'priority-high']
links:
  - 'https://hub.docker.com/r/stellar/stellar-galexie/tags'
history:
  - date: 2026-07-09
    status: active
    who: stkrolikiewicz
    note: 'Created from 2026-07-08 galexie proto-27 stall incident (prod ingestion idle ~16h).'
---

# Galexie protocol-upgrade hardening: durable 27.0.0 pin + fix silent ingestion-lag alarm

## Summary

On 2026-07-08 ~17:00Z prod ingestion silently stopped: pubnet upgraded to
**protocol 27**, but the pinned Galexie image shipped a pre-27 stellar-core.
Captive-core kept following SCP but could not apply new-protocol ledgers
(`History: Skipping catchup: incompatible core version`), so it stalled mid-sync
and wrote nothing to S3 → the indexer starved. It ran ~16 h before a human
noticed. This task (a) durably pins the fixed image and (b) fixes the alarm that
was _supposed_ to catch this but couldn't fire.

## Status: Active

Hotfix already **live in prod** (Galexie 27.0.0 deployed manually 2026-07-09,
caught up the full gap, S3 flowing). This PR captures the pin durably and lands
the alarm fix. Remaining: deploy the CloudWatch stack + one-time alarm test.

## Context

- Galexie runs as a single ECS Fargate task (`production-galexie-live`,
  `desiredCount: 1`) exporting ledger XDR to S3; S3 → SNS → SQS → indexer Lambda.
- The image is a digest-pinned mirror of Docker Hub `stellar/stellar-galexie`
  (`galexieImageTag` in `infra/envs/production.json`; source digest in GitHub
  env `GALEXIE_IMAGE_DIGEST`). Galexie's version now tracks the protocol, so
  **27.0.0 == protocol 27** (Horizon confirmed pubnet on proto 27, core 27.1.0).
- The ECS healthcheck is `pgrep -x stellar-core` — it only proves the process is
  alive, not that ledgers advance. A stuck-but-alive core passes it, so ECS
  never restarted the task; only the `galexie-ingestion-lag` alarm could have
  caught it — and it was misconfigured (below).

## Implementation

This PR:

1. **Durable image pin** — `galexieImageTag` in `infra/envs/production.json`
   `sha256:e60c8f8d…` → `sha256:91eae7af…` (the ECR digest of the mirrored
   Galexie 27.0.0; note the ECR digest differs from the Docker Hub source digest
   `sha256:81a9e829…` because `docker push` re-serialized the manifest).

2. **Fix the silent alarm** — rework `galexie-ingestion-lag` in
   `cloudwatch-stack.ts`: switch the signal from Lambda `Invocations` to the SQS
   doorbell rate (`AWS/SQS NumberOfMessagesSent` on `production-ledger-ingest`),
   window `galexieLagMinutes` 15 → **5**, and `treatMissingData: NOT_BREACHING`
   → **BREACHING**. The invocations metric could not safely go below ~10 min: a
   single reconcile drains a backlog for up to 9 min (`RECONCILE_DEADLINE`), so
   invocation starts are ~9 min apart during any catchup — a 5-min window would
   false-fire. The doorbell rate tracks Galexie's actual output (~1 S3 object /
   ledger, ~5-6 s) independent of indexer batching, so 5-min detection is safe.
   BREACHING is required because SQS emits no datapoint when idle (metric goes
   absent, not 0) — under NOT_BREACHING the alarm can never fire on a true stop.

Deploy after merge: `make deploy-production-cloudwatch` (alarm). The image pin is
already deployed; this just prevents the next ingestion deploy from rolling back
to the broken image. Also update GitHub env `GALEXIE_IMAGE_DIGEST` (production +
staging) → `sha256:81a9e829…` so CI/staging track the same image.

## Acceptance Criteria

- [x] `galexieImageTag` pinned to Galexie 27.0.0 ECR digest in production.json
- [x] `galexie-ingestion-lag` reworked: SQS `NumberOfMessagesSent` signal,
      5-min window (`galexieLagMinutes` 15→5), `treatMissingData: BREACHING`
- [ ] CloudWatch stack deployed to prod (`make deploy-production-cloudwatch`)
- [ ] One-time alarm test: `aws cloudwatch set-alarm-state --alarm-name
    production-galexie-ingestion-lag --state-value ALARM --state-reason test`,
      confirm it posts to the Slack channel (then let it self-reset); confirm SSM
      `slack-workspace-id`/`slack-channel-id` are set on prod so delivery works
- [ ] GitHub env `GALEXIE_IMAGE_DIGEST` updated to `sha256:81a9e829…` (prod + staging)
- [x] **Docs updated** — N/A. `docs/architecture/**` describe Galexie/alarms only
      generically (no image version, no `treatMissingData` semantics). This is an
      observability data-handling fix + an image-tag config bump; neither changes
      the described topology / schema / endpoints / pipeline steps. Legitimate
      N/A per CLAUDE.md (observability tuning).
- [x] **API types regenerated** — N/A. No change under `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.

## Issues Encountered

- **ECR digest ≠ Docker Hub digest.** Mirroring `docker pull …@sha256:81a9e829`
  then `docker push` to ECR produced a _different_ manifest digest
  (`sha256:91eae7af`) because Docker re-serialized the manifest. `galexieImageTag`
  is resolved via `fromEcrRepository`, so it must hold the **ECR** digest. Always
  read the landed digest back (`aws ecr batch-get-image … imageId.imageDigest`)
  rather than reusing the Hub digest.
- **Recovery is a full state restore.** A fresh Fargate task starts with empty
  ephemeral storage, so catchup re-downloaded + applied ~16 GB of BucketList
  state (~4 min) before replaying the ~16 h / ~11k-ledger gap. This ~20-min
  warmup is paid on every Galexie restart/deploy. Franklin Templeton history
  archives (`FT_SCV_*`) 404'd on some checkpoints — benign, core cross-checks
  other archives and says so explicitly.

## Design Decisions

### From Plan

1. **Pin the ECR digest, not a moving tag.** Matches existing convention
   (production.json already held a `sha256:` ECR digest) and keeps deploys
   reproducible / supply-chain-safe. `latest` / auto-bump rejected.

### Emerged

2. **Bundled pin + alarm fix in one PR.** Same incident, both tiny infra
   changes. The pin captures already-deployed prod state (safe to merge now);
   the alarm needs a follow-up deploy + test but is not a merge blocker. Split
   on request.
3. **Switched the alarm to a direct galexie-write signal (SQS doorbell rate),
   not just a `treatMissingData` flip.** The user wanted 5-min detection; the
   invocations proxy can't safely go below ~10 min (9-min `RECONCILE_DEADLINE`
   → invocation starts ~9 min apart during backlog). SQS `NumberOfMessagesSent`
   on the ingest queue tracks Galexie's output directly and is confounded
   neither by indexer batching nor by indexer pauses. Chose it over the S3
   `PutRequests` alternative because SQS metrics are free (S3 request metrics
   cost ~$3-5/mo) and verified in prod that SNS→SQS deliveries do increment it
   (`[748, 389, 134]` over three 5-min buckets during backlog drain).
4. **Comment carries a "do NOT revert to NOT_BREACHING" warning.** The bug was
   subtle and looks like a reasonable anti-flap setting; without the rationale a
   future cleanup could reintroduce it.

## Future Work

Candidates for follow-up backlog tasks (not auto-created — pending confirmation):

- **Ledger-advance healthcheck:** replace/augment `pgrep -x stellar-core` with a
  check that ledgers are advancing, so a stuck-but-alive core self-heals via ECS
  restart instead of waiting on the lag alarm + a human.
- **Protocol-upgrade watch process:** subscribe to SDF protocol announcements so
  the Galexie/core bump is a planned ~20-min task before each pubnet upgrade,
  not an outage. (Core bump per protocol is inherent to captive-core; the
  outage was not.)
- **Persistent BucketList state (EFS):** skip the ~16 GB full-state restore on
  every restart/deploy to cut the ~20-min warmup. Weigh cost/complexity.

## Notes

- Incident timeline + commands: see chat + memory `project_galexie_protocol_upgrade_stall`.
- Digests: Docker Hub source `sha256:81a9e829…c120b6`; ECR pinned `sha256:91eae7af…3c82c8`.
