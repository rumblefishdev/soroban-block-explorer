---
id: '0249'
title: 'FEATURE: Destroy AWS infra in us-east-1 (staging + any production stacks)'
type: FEATURE
status: completed
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
  - date: '2026-05-21'
    status: active
    who: fmazur
    note: >
      Promoted to active. Phase 0 discovery (aws cli) confirmed
      us-east-1 footprint: 12 staging stacks + `Explorer-Cicd`
      (deployed) + `CDKToolkit`. Zero production stacks
      (`hostedZoneId: "CHANGE_ME"` blocked `validateConfig`).
      Decisions: (a) destroy `Explorer-Cicd` too, rebuild in
      eu-central-1 as part of 0239; (b) delete staging hosted zone
      `staging.sorobanscan.rumblefish.dev`;
      (c) delete staging ACM cert after CloudFront releases it.
      No data retention (no RDS snapshot, no S3 archive).
      `cdk destroy` will empty both staging S3 buckets automatically
      (`autoDeleteObjects: true` for non-prod envs).
  - date: '2026-05-21'
    status: completed
    who: fmazur
    note: >
      us-east-1 fully wiped. All 12 staging stacks + `Explorer-Cicd`
      + `CDKToolkit` destroyed. Non-CDK cleanup: secrets, ACM cert,
      staging Route 53 zone (after ACM validation CNAME removal),
      CDK assets S3 bucket (after versioned-object cleanup).
      Final verification: list-stacks / NAT GW / VPC / S3 / ECR /
      Secrets / ACM all empty. Two operator interventions during
      destroy: (1) manual delete of 2 orphan VPC Lambda ENIs that
      blocked Network stack delete; (2) ACM validation CNAME +
      versioned-bucket cleanup. Files: infra/Makefile (+destroys),
      docs/architecture/infrastructure/infrastructure-overview.md
      (§4.2 + §5.6 region change notes), this task .md.
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

- [x] Pre-destroy checklist complete — N/A for snapshots/archives
      (no data retention per task owner decision); S3 buckets had
      `autoDeleteObjects: true` for staging so CDK destroy emptied
      them automatically; ECR images not documented (greenfield
      rebuild in 0239).
- [x] All staging CDK stacks destroyed in us-east-1
      (12 stacks: Network, Rds, LedgerBucket, Migration, Partition,
      Compute, Ingestion, Delivery, ApiGateway, Observability,
      CloudWatch, Bastion).
- [x] All production CDK stacks destroyed in us-east-1 — zero
      production stacks were ever deployed (validateConfig blocked
      on `hostedZoneId: "CHANGE_ME"`), so this AC is vacuously
      satisfied.
- [x] Makefile updated with `destroy-staging-*` targets for every
      stack (symmetric to existing `deploy-*` targets); production
      destroy targets added for parity; `destroy-cicd` added;
      umbrella `destroy-staging` / `destroy-production` added.
- [x] Post-destroy verification passes — `list-stacks`, NAT GW,
      VPC, S3, ECR, Secrets, ACM all empty in us-east-1.
- [x] Route 53 hosted zone for `staging.sorobanscan.rumblefish.dev`
      decision documented — **deleted** (staging retired entirely,
      not coming back as AWS-side env).
- [x] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      §4.2 (launch AZ note) and §5.6 (region change paragraph) updated
      with us-east-1 footprint removal + cross-link to [[task-0239]].

## Implementation Notes

**Files modified (3):**

- `infra/Makefile` — added 13 `destroy-staging-*` targets
  (network, rds, ledger-bucket, migration, partition, compute,
  ingestion, delivery, apigateway, observability, cloudwatch,
  bastion already existed, hetzner-dns) + 13 `destroy-production-*`
  for parity + `destroy-cicd` + umbrella `destroy-staging` and
  `destroy-production`. Also added 4 missing `deploy-staging-*` /
  `deploy-production-*` targets (compute, apigateway, observability,
  cloudwatch) for true symmetry.
- `docs/architecture/infrastructure/infrastructure-overview.md` —
  §4.2 note that launch AZ `us-east-1a` is historical; §5.6 region
  change paragraph linking 0249/0239 + CloudFront cert constraint
  note (must stay in us-east-1 regardless of prod region).
- `lore/1-tasks/active/0249_*.md` — promotion + history entries.

**Destroy execution (operator-driven):**

Reverse dependency order via per-stack `make destroy-staging-*`
targets. Two manual interventions during execution (see Issues).

**Manual non-CDK cleanup:**

- `secretsmanager delete-secret --force-delete-without-recovery`
  for `soroban-explorer/staging/rds-credentials`.
- ECR `staging-galexie` was CDK-managed and removed with Ingestion
  stack — no manual delete needed (initial plan assumed manual).
- ACM cert `*.staging.sorobanscan...` deleted manually (cert was
  imported via `fromCertificateArn`, not created by stack).
- Route 53 staging zone: deleted ACM validation CNAME first
  (`_eff656...acm-validations.aws.`), then `delete-hosted-zone`.
- `CDKToolkit` stack + S3 assets bucket
  (`cdk-hnb659fds-assets-<account-id>-us-east-1`) destroyed.
  Bucket required versioning cleanup (`list-object-versions` +
  `delete-objects` for both Versions and DeleteMarkers) before
  `s3 rb` would succeed.

**Final state:** `aws s3 ls`, `aws ecr describe-repositories`,
`aws secretsmanager list-secrets`, `aws acm list-certificates`,
`aws ec2 describe-nat-gateways`, `aws ec2 describe-vpcs`, and
`aws cloudformation list-stacks` all return empty in us-east-1.

## Design Decisions

### From Plan

1. **Reverse dependency order destroy** — per task spec, started
   with top-layer stacks (CloudWatch, ApiGateway, Observability),
   ended with Network (VPC). CDK destroy auto-detects dependents
   so even single-stack targets pulled in their dependants.

2. **No RDS final snapshot** — task owner decision: zero data
   retention. Staging RDS had `dbDeletionProtection: false`
   already; no operator action needed before destroy.

3. **Makefile destroy targets symmetric to deploy-\*** — explicit
   AC requirement; mirrored existing convention.

### Emerged

4. **Destroy `Explorer-Cicd` too** (not just staging) — Phase 0
   discovery showed it was deployed. Cleaner break: rebuild fresh
   in `eu-central-1` as part of 0239. Trade-off: no CI deploy
   between 0249 and 0239 Phase 0, but the bootstrap deploy of
   eu-central-1 is operator-run from laptop anyway.

5. **Delete staging hosted zone** — was an open AC decision
   ("keep / delete"). Resolved: delete. Staging is fully retired
   and never returns as AWS-side env; ~$0.50/mo savings + cleaner
   state.

6. **Delete `CDKToolkit` in us-east-1 too** — original task said
   "leave in place". Re-evaluated: with prod moving to eu-central-1
   and only ACM cert needing to stay in us-east-1 (cert can be
   issued via `aws acm request-certificate` without CDK), CDK
   bootstrap in us-east-1 has no consumer. Destroyed for zero
   footprint. Re-bootstrap is a one-line command if ever needed
   again (`npx cdk bootstrap aws://<account>/us-east-1`).

7. **Added `Explorer-Cicd` destroy target separately** — task
   originally focused on staging/production patterns; cicd is a
   third CDK app (`CICD_APP`). Added `destroy-cicd` mirroring
   the existing `deploy-cicd`.

## Issues Encountered

- **Lambda VPC ENI cleanup delay** — `destroy-staging-network`
  failed first attempt with `DELETE_FAILED` on two private subnets
  and `LambdaSg`. Cause: two orphan ENIs (type `interface`, not
  `lambda` — older pre-Hyperplane ENIs) for `staging-soroban-explorer-api`
  remained in the VPC in `available` state, blocking subnet and
  SG deletion. Manually deleted via `ec2 delete-network-interface`
  for each ENI ID, then re-ran `make destroy-staging-network` which
  resumed from `DELETE_FAILED` and completed cleanly.

- **ACM DNS validation CNAME blocks hosted zone deletion** —
  `delete-hosted-zone` refused while the cert validation record
  `_eff656...acm-validations.aws.` was still present. Removed via
  `change-resource-record-sets` (Action DELETE) before retrying.

- **S3 bucket versioning blocks `rb`** — `cdk-hnb659fds-assets-…`
  has versioning enabled. `aws s3 rm --recursive` removed current
  versions but left old versions / delete markers. Resolution:
  `s3api list-object-versions` + `delete-objects` for both
  `Versions` and `DeleteMarkers`, then `s3 rb`.

- **Parent zone NS delegation** — `sorobanscan.rumblefish.dev`
  may still contain an NS delegation record for
  `staging.sorobanscan.rumblefish.dev` pointing at the (now
  deleted) child zone's nameservers. Left in place — DNS lookups
  for staging return SERVFAIL, which is acceptable since staging
  is retired. Cleanup can be done opportunistically next time
  someone is in the prod hosted zone.

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
