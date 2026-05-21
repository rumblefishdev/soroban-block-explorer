---
id: '0249'
title: 'FEATURE: Destroy AWS infra in us-east-1 (staging + any production stacks)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0239', '0234', '0216']
tags:
  [
    priority-high,
    effort-medium,
    layer-infrastructure,
    aws-cdk,
    destroy,
    migration,
  ]
milestone: 1
links: []
history:
  - date: '2026-05-20'
    status: backlog
    who: fmazur
    note: >
      Spawned as the predecessor to 0239. The plan is to (a) drop
      the staging environment entirely (no AWS-side staging until
      product decides otherwise) and (b) move production to
      eu-central-1 (region change handled by 0239). Both steps
      start from a clean us-east-1 — this task wipes everything
      there before 0239 re-deploys in eu-central-1.
---

# FEATURE: Destroy AWS infra in us-east-1

## Summary

Tear down every CDK-managed AWS resource currently deployed in
`us-east-1` for this project — both the staging environment and
any production stacks (HetznerDnsStack from 0234 included). Leaves
a clean AWS account scoped to nothing except the Hetzner-side
ClickHouse box on Hetzner (out of AWS scope), so [[task-0239]] can
re-deploy production fresh in `eu-central-1`.

## Context

Two independent decisions converge on a single us-east-1 wipe:

1. **No AWS-side staging.** Memory `prod-store-ch-on-hetzner`
   (2026-05-13) made ClickHouse-on-Hetzner the sole prod store and
   marked AWS RDS for decommission. Staging never gained a Hetzner
   counterpart, so keeping a staging AWS environment online just
   burns money and audit surface for no benefit. Decision:
   decommission staging completely.

2. **Move production to eu-central-1.** Latency + data residency
   improvements; team is EU-based. Decision documented as part of
   [[task-0239]]. Region change is non-incremental — CDK stacks
   are recreated, not moved.

The cleanest sequence is: wipe `us-east-1` → re-deploy prod fresh
in `eu-central-1` (via 0239). Doing them in this order avoids
straddling regions during the transition and avoids the "two RDS
instances, two NAT GWs" cost overlap.

## Scope

### Stacks to destroy in us-east-1

**Staging (all):**

- `Explorer-staging-CloudWatch`, `-ApiGateway`, `-Observability`,
  `-Delivery`, `-Ingestion`, `-Compute`, `-Partition`, `-Migration`,
  `-LedgerBucket`, `-Rds`, `-Network`
- `Explorer-staging-Bastion` (separate CDK app — `BASTION_STAGING_APP`)
- `Explorer-staging-HetznerDns` (added by [[task-0234]] — never
  intended to deploy for staging; `chDomainName: "PLACEHOLDER"`
  blocks synth, so this is most likely never deployed and the
  destroy is a no-op confirmation)

**Production us-east-1 (only what was actually deployed):**

- `Explorer-production-HetznerDns` (if it was deployed against
  us-east-1; the new region for prod is `eu-central-1`, so this
  stack is being recreated there by [[task-0239]])
- Any other `Explorer-production-*` stacks that were deployed in
  us-east-1 before the region change decision (likely none, since
  `production.json:hostedZoneId` is still `CHANGE_ME` — confirm
  via `aws cloudformation list-stacks --region us-east-1`)

**Shared in us-east-1:**

- `cicd-app.ts` (`CICD_APP`) — if it was deployed; reconsider
  whether CI/CD belongs in `us-east-1` (must stay for GitHub OIDC
  global endpoint or move with prod — decide as part of 0239)

### Pre-destroy checklist

- [ ] `aws cloudformation list-stacks --region us-east-1 --stack-status-filter CREATE_COMPLETE UPDATE_COMPLETE`
      — definitive list of what is actually deployed (the CDK
      `make destroy-*` targets address only the canonical names;
      this catches drift)
- [ ] **RDS final snapshot** for any RDS instances (`Explorer-staging-Rds`,
      possibly `Explorer-production-Rds`). Manual snapshot retained
      30 days as last-resort recovery. Memory and project ADRs say
      Postgres is being abandoned, but the historical snapshot is
      cheap insurance against an unknown data dependency surfacing
      post-destroy.
- [ ] **S3 buckets**: list contents of all project-owned buckets
      (`*-soroban-explorer-spa`, ledger-bucket variants). Confirm
      they're empty or contents are archived to a separate
      long-term bucket before `cdk destroy` (CDK destroys buckets
      only if `autoDeleteObjects: true` was set or they're empty).
- [ ] **Secrets Manager**: list `soroban-explorer/staging/*` and
      any production secrets. Set the recovery window to 0 days
      (`--force-delete-without-recovery`) ONLY after confirming
      nothing references them.
- [ ] **ECR repositories**: list image counts and last-pushed
      timestamps. Document any image tags that should be re-pushed
      to the new region as part of 0239.
- [ ] **Route 53 hosted zones**: `sorobanscan.rumblefish.dev` and
      `staging.sorobanscan.rumblefish.dev` are GLOBAL resources.
      Confirm we're keeping the zones themselves (DNS-side
      continuity) — only the records inside them go (CloudFront
      ARecords, HetznerDnsStack ARecords for staging if any). The
      production hosted zone stays; 0239 will populate it with
      eu-central-1-backed records.
- [ ] **ACM certificates in us-east-1**: any `*.sorobanscan.rumblefish.dev`
      certs that were issued for CloudFront. CloudFront REQUIRES
      certs in us-east-1, so the production cert will be re-issued
      in us-east-1 by 0239 even after the prod region change.
      Decide: leave existing cert (CloudFront-only, ARN stable) or
      destroy and re-issue.

### Destroy steps

For each stack, in reverse dependency order:

```bash
# Staging
make destroy-staging-bastion        # already in Makefile
# (rest need new Makefile targets — see "Tooling" below)
make destroy-staging-cloudwatch
make destroy-staging-apigateway
make destroy-staging-observability
make destroy-staging-delivery
make destroy-staging-ingestion
make destroy-staging-compute
make destroy-staging-partition
make destroy-staging-migration
make destroy-staging-ledger-bucket
make destroy-staging-rds            # AFTER manual snapshot
make destroy-staging-network        # AFTER all VPC-using stacks gone

# Production (only if anything was actually deployed in us-east-1)
make destroy-production-hetzner-dns
# (other production stacks likely never reached AWS — verify)
```

Each `destroy` step requires manual confirmation (`--require-approval
broadening` is for deploys; `destroy` is intentionally low-friction
but verify with `cdk diff` style review first).

### Tooling — Makefile destroy targets

Current Makefile has only `destroy-{staging,production}-bastion`.
This task adds symmetric `destroy-*` targets for every stack so
the wipe doesn't need ad-hoc `cdk destroy` invocations. Mirrors
the existing `deploy-*` per-stack target convention.

### Post-destroy verification

- [ ] `aws cloudformation list-stacks --region us-east-1` returns
      empty (or only CDK bootstrap stack `CDKToolkit`).
- [ ] `aws ec2 describe-instances --region us-east-1` returns
      empty (no bastion EC2 stragglers).
- [ ] `aws ec2 describe-nat-gateways --region us-east-1` returns
      empty (NAT GW is the most expensive forgotten resource).
- [ ] `aws ec2 describe-vpcs --region us-east-1` returns empty
      (no straggler VPCs).
- [ ] `aws s3 ls` shows no project-owned buckets in us-east-1.
- [ ] `aws ecr describe-repositories --region us-east-1` returns
      empty for project namespaces.
- [ ] `aws secretsmanager list-secrets --region us-east-1` returns
      empty for project namespaces.
- [ ] AWS Billing console: us-east-1 cost projection for next
      month drops to ~$0 (excluding shared / global resources).

## Acceptance Criteria

- [ ] Pre-destroy checklist complete (snapshots taken, S3
      contents confirmed empty / archived, ECR images documented).
- [ ] All staging CDK stacks destroyed in us-east-1.
- [ ] All production CDK stacks (if any) destroyed in us-east-1.
- [ ] Makefile updated with `destroy-staging-*` targets for every
      stack (symmetric to existing `deploy-*` targets).
- [ ] Post-destroy verification passes — `list-stacks`, NAT GW,
      VPC, S3, ECR, Secrets all empty in us-east-1.
- [ ] Route 53 hosted zone for `staging.sorobanscan.rumblefish.dev`
      decision documented (keep / delete).
- [ ] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      §5.6 note: us-east-1 footprint removed; new prod region is
      `eu-central-1` (cross-link to [[task-0239]]).

## Dependencies

- [[task-0234]] should be merged before this task runs (otherwise
  the destroy target list misses `HetznerDnsStack`). 0234 PR is
  open as of task creation.
- [[task-0228]] (parallel-backfill merge into Hetzner CH) should
  be at least far enough along that an `RDS final snapshot` is no
  longer the canonical dataset — the snapshot is recovery
  insurance, not active data.

## Out of Scope

- Re-deploying anything in `eu-central-1`. That's [[task-0239]].
- Decommissioning the Hetzner box. That stays — it's the
  production data plane.
- Migrating staging to a different region. Staging is being
  retired, not moved.
- Domain / DNS migration (Route 53 zones stay). Only individual
  records pointing at us-east-1 resources are removed alongside
  the stacks that own them.
- Destroying CDK bootstrap resources (`CDKToolkit` stack). Those
  are needed for future `cdk deploy` in the same account; leave
  in place. (If the entire AWS account is being retired, that's a
  separate decision out of this task's scope.)
