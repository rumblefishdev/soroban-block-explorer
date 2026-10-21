---
id: '0650'
title: 'Protocol 30: bump Galexie 29.0.0 → the first image with captive core 30, before the pubnet vote'
type: OPS
status: active
related_adr: []
related_tasks: ['0605', '0610', '0649', '0367', '0560']
tags: [ingestion, galexie, protocol-30, priority-high, effort-small]
links:
  - 'https://hub.docker.com/r/stellar/stellar-galexie/tags'
  - 'https://github.com/stellar/stellar-galexie/pull/98'
  - 'https://github.com/stellar/stellar-galexie/tree/protocol-next'
history:
  - date: '2026-10-19'
    status: backlog
    who: karolkow
    note: >
      Created as the Galexie half of protocol 30 (0649 covers stellar-xdr).
      No Galexie image with captive core 30 exists yet; the P30 code sits on
      upstream branch `protocol-next`, unpublished. Mirrors 0605, done ahead
      of the vote this time.
  - date: '2026-10-21'
    status: active
    who: karolkow
    note: >
      Started on the 0610 NEW CORE alarm: Galexie 30.0.0 on Docker Hub
      2026-10-20 21:59 UTC; steps 1-2 done.
---

# Protocol 30: Galexie bump

## Summary

Galexie exports mainnet ledgers to S3 through its captive stellar-core. A core
older than the network's protocol stops exporting at the vote — 0367, and 0605
on 2026-10-01, when Galexie 28.0.1 stalled for ~14 h at protocol 29. Protocol
30 needs a Galexie image with captive core 30 and the protocol-30
go-stellar-sdk. This task mirrors that image into ECR and pins it in
`production.json` before the pubnet vote. Core 30 also applies protocol-29
ledgers (mainnet validators already run v30.0.0 on protocol 29, 0649 notes), so
deploying before the vote is safe.

## Stan teraz

- Steps 1–2 done 2026-10-21: `30.0.0` (core `30.0.0-3642.0e49751aa`, contains
  `47e46de`) in ECR as `sha256:4c75a89b…9305d84` — single-arch amd64, so the
  ECR digest equals Hub's. Running: 29.0.0 (`sha256:5269dfd9…d495a3`).
- Pubnet vote 2026-11-23 17:00 UTC (SDF Bell guide). Next: step 3 PR, step 4.

## Context

Evidence: [notes/R-galexie-p30-image-status.md](notes/R-galexie-p30-image-status.md).

- P30 code is upstream PR #98 (commit `47e46de`, branch `protocol-next`):
  go-stellar-sdk for CAP-0084/87/88 plus core RC `30.0.0-3642.0e49751aa`.
  Not on Hub: upstream CI publishes only `main`, `release/**` and `galexie-v*`.
- The image must contain `47e46de`: a core-30 package on `main`'s old SDK is
  not enough (CAP-0088 adds mandatory `StellarValueExt` arms).
- Testnet runs no Galexie (`ledgerSource: public-lake`); only 0649 matters there.

## Implementation Plan — owner steps

All five are the owner's: ECR push and deploy change production.

### Step 1: Check the candidate image (read-only)

Take the newest Hub tag whose captive core is 30 (the 0610 alarm names it).
Set `TAG` to it — a release tag like `30.0.0`, or a sha tag.

```bash
TAG=<tag-from-hub>
docker buildx imagetools inspect "stellar/stellar-galexie:$TAG" --format '{{json .Image.Config.Env}}'
```

Expect `STELLAR_CORE_VERSION=30.0.0-…`. `30.0.0-3642.0e49751aa` is the final
build: `0e49751aa` is the commit of stellar-core's `v30.0.0` release tag
(published 2026-10-16, not a prerelease), though PR #98 shipped it earlier as
a release candidate. Then check the image contains the
P30 SDK commit (for a sha tag `TAG` is the commit):

```bash
gh api "repos/stellar/stellar-galexie/compare/47e46de...$TAG" --jq .status
```

Expect `ahead` or `identical`. `behind` / `diverged` = built without PR #98: do
not use it.

### Step 2: Mirror Docker Hub → ECR

`docs/deployment.md`, "Galexie (live ingestion)", step 1:

```bash
REPO=$(aws ssm get-parameter --region eu-central-1 --name /soroban-explorer/production/ecr-galexie-repo-uri --query Parameter.Value --output text)
```

```bash
aws ecr get-login-password --region eu-central-1 | docker login --username AWS --password-stdin "${REPO%%/*}"
```

```bash
docker pull "stellar/stellar-galexie:$TAG" && docker tag "stellar/stellar-galexie:$TAG" "$REPO:$TAG" && docker push "$REPO:$TAG"
```

### Step 3: Pin the digest ECR reports

```bash
aws ecr describe-images --region eu-central-1 --repository-name production-galexie --image-ids imageTag="$TAG" --query 'imageDetails[0].imageDigest' --output text
```

Put that value in `infra/envs/production.json → galexieImageTag` (today
`sha256:5269dfd9034948f3db8dd7595fe8aec88aae5480f712a78e2dea48a841d495a3`).
The ECR digest, never the Hub one (deployment.md explains why). Commit on a
branch, PR, merge.

### Step 4: Deploy the Ingestion stack

Before running, list what else rides on the Ingestion stack since its last
deploy (`git log <last-production-tag>..origin/master -- infra/`). Then, from
the merged `master`:

```bash
make -C infra deploy-production-ingestion
```

### Step 5: Verify

- ECS service `production-galexie` steady, one task running the new digest.
- `production-galexie-ingestion-lag` alarm OK; new objects keep landing in the
  ledger bucket.
- The 0610 watch's next run answers OK (our core = Hub core).
- After the vote: `ledgers` contiguous across the vote ledger (read-only
  `chq`), both production DLQs at 0.

**Rollback** before the vote: `galexieImageTag` back to `sha256:5269dfd9…` +
step 4. After the vote there is no Galexie rollback below core 30
(deployment.md, "previous code has a floor").

## Acceptance Criteria

- [ ] Candidate image checked: final core 30, contains `47e46de`
- [ ] Mirrored into ECR `production-galexie`; `galexieImageTag` = the digest
      read back from ECR
- [ ] Deployed before the pubnet protocol-30 vote; lag alarm OK
- [ ] After the vote: `ledgers` contiguous across the vote ledger, DLQs 0
- [ ] **Docs updated** — N/A: the Galexie recipe in `docs/deployment.md`
      already describes this procedure; schema, endpoints and pipeline unchanged
- [ ] **API types regenerated** — N/A: `crates/api`, `Cargo.*` untouched
