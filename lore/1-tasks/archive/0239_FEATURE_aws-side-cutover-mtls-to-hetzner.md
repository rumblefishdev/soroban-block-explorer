---
id: '0239'
title: 'FEATURE: AWS-side cutover — Lambdas out-of-VPC, Galexie public subnet, mTLS to Hetzner CH, NAT GW + RDS decommission, region eu-central-1'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0216', '0227', '0228', '0234', '0240', '0241', '0249', '0253']
tags:
  [
    priority-high,
    effort-large,
    layer-infrastructure,
    aws-cdk,
    mtls,
    migration,
    cost-optimization,
    region-change,
  ]
milestone: 1
links: []
history:
  - date: '2026-05-20'
    status: backlog
    who: fmazur
    note: 'Spawned from 0227 future work — AWS-side restructuring downstream of the Hetzner ClickHouse deployment. Lambdas + Galexie no longer need VPC-internal RDS connectivity since ClickHouse on Hetzner replaces it; this enables Lambda VPC removal, Galexie public-subnet placement, and NAT Gateway + RDS decommissioning. All AWS → Hetzner traffic authenticates via mTLS client certificates issued by the team CA (from 0227).'
  - date: '2026-05-20'
    status: backlog
    who: fmazur
    note: >
      Scope extended: region change us-east-1 → eu-central-1 for
      production. Combined with [[task-0249]] (destroy us-east-1
      first), this task becomes a greenfield deploy in eu-central-1
      with the target AWS-minimal topology baked in from day one
      — no incremental migration of in-place stacks. CloudFront
      cert stays in us-east-1 (CloudFront requirement); everything
      else (network, lambdas, ECR, secrets, KMS, regional WAF)
      moves to eu-central-1.
  - date: '2026-05-21'
    status: active
    who: fmazur
    note: >
      Promoted to active. All blocking dependencies (0227, 0234, 0240,
      0249) are completed. Starting with Phase 0/2/3 CDK refactor
      (region change to eu-central-1, Lambda out-of-VPC + SM Extension
      wiring, Galexie public subnet) locally; operator handles AWS
      bootstrap + cert issuance + ansible runs. Phase 6 decommission
      gated on 0228 (parallel-backfill merge) still active.
  - date: '2026-05-21'
    status: completed
    who: fmazur
    note: >
      Closed at code-side completion. Delivered the full CDK / TypeScript
      refactor (Lambdas out-of-VPC + SM Extension + mTLS env, Galexie
      public subnet with assignPublicIp ENABLED + ECS native secret
      injection, minimal eu-central-1 VPC, RDS + bastion stacks moved to
      .trash/, region change in production.json + cicd.json, docs per
      ADR 0032). 19 files touched (1 new: infra/src/lib/mtls.ts; 7 moved
      to .trash/). `nx build` PASS, `cdk synth` produces 11 production
      templates correctly (validated by jq inspection of Vpc, Compute,
      Ingestion templates). Three independent subagent audits run:
      correctness, security, spec-compliance — all five flagged items
      addressed before closure. Runtime acceptance criteria (Phase 5
      smoke per Lambda + Galexie, Caddy CN log verification, off-list
      CN 403 negative test) absorbed into task 0241 Part D since the
      first realistic deploy of the new region happens at the indexer
      cutover, not as a separate event. Operator follow-up tracked
      there. Cert rotation strategy (AC #6) spawned as task 0253
      (renamed from 0251 after develop merge introduced an unrelated
      0251_BUG_frontend-qa-fixes-batch).
      Phase 6 "decommission" steps were vacuously satisfied — per
      task 0249 archive, no production stacks were ever deployed in
      us-east-1 (validateConfig blocked on `hostedZoneId: "CHANGE_ME"`),
      so no running RDS / NAT GW / bastion exists to destroy. The CDK
      changes that previously would have effected decommission are now
      part of a greenfield deploy in eu-central-1.
---

# FEATURE: AWS-side cutover — Lambdas out-of-VPC, Galexie public subnet, mTLS to Hetzner CH

## Summary

Restructure the AWS CDK app (`infra/src/`) so that **only ClickHouse
runs on Hetzner** and the AWS side becomes a minimal stateless layer:

- Lambdas leave the VPC entirely and reach Hetzner CH over the
  public internet (authenticated via mTLS).
- Galexie ECS Fargate moves to a public subnet with
  `assignPublicIp: ENABLED`, reaching both the Stellar peer overlay
  and Hetzner CH via the Internet Gateway.
- RDS Postgres and the NAT Gateway are decommissioned in the final
  cutover step.

mTLS replaces VPC isolation + IP whitelisting as the cross-cloud
auth mechanism. Client certs are sourced from AWS Secrets Manager,
issued by the team-owned CA delivered in [[task-0227]].

## Context

Task 0216 (RESEARCH, parent) decided that the analytics datastore
moves to Hetzner-hosted ClickHouse because the all-AWS topology
was both unnecessarily expensive (NAT Gateway + RDS dominate cost
for our workload) and architecturally over-isolated (VPC + RDS
Proxy + bastion add complexity that wasn't carrying its weight
for an indexer use case).

Task 0227 delivered the Hetzner-side artefacts: Caddy mTLS gate,
CA tooling, Ansible playbook, Docker compose stack. The
ClickHouse endpoint is now live at `<ch-prod-domain>:443` (DNS
wiring tracked separately in [[task-0234]]).

This task is the **AWS-side downstream half** of the migration —
restructuring the CDK app to drop the obsolete pieces (NAT GW,
RDS, VPC attachment) and route every AWS service through mTLS to
the Hetzner endpoint.

### Current AWS topology (pre-cutover)

```
AWS VPC (private CIDR, 2 AZ)
├── Public subnets (1 NAT Gateway)
├── Private subnets (PRIVATE_WITH_EGRESS)
│   ├── Lambdas (API, Ingestion, Partition, Compute, Migration) → RDS
│   ├── Galexie ECS Fargate → RDS + Stellar peer network
│   └── RDS PostgreSQL + RDS Proxy
└── NAT Gateway (egress for Lambdas + Galexie)
```

13 stacks currently in `infra/src/lib/stacks/`:
`network`, `rds`, `compute`, `ingestion`, `partition`, `migration`,
`api-gateway`, `delivery`, `ledger-bucket`, `cloudwatch`,
`observability`, `bastion`, `cicd`.

### Target topology (post-cutover)

```
AWS minimal VPC (public subnet only, just for Galexie)
└── Galexie ECS Fargate
    ├── assignPublicIp: ENABLED (gets public IP per task)
    ├── Reaches Stellar peer overlay via IGW
    └── Reaches Hetzner CH via internet (mTLS-authenticated)

Lambdas: OUTSIDE the VPC
├── AWS-managed internet path (free, no NAT GW)
├── Random AWS pool egress IPs (no IP pinning — mTLS handles identity)
└── Read mTLS client cert from Secrets Manager at runtime

Decommissioned: RDS, NAT Gateway, RDS Proxy, bastion, private subnets
```

## Scope

### Phase 1 — Issue + distribute mTLS client certs

For every AWS service that will talk to Hetzner CH:

- Use `infra-hetzner/ca/issue-client-cert.sh` on the operator's
  Linux laptop (Linux-only per the script's `/dev/shm`
  precondition).
- CN convention: `<service>-<environment>` — e.g.
  `lambda-api-<environment>`, `lambda-ingestion-<environment>`,
  `galexie-<environment>`, `lambda-migration-<environment>`. The
  `<environment>` segment matches the CDK env name (e.g.
  `production`, `staging`) so multi-env deploys never collide on
  the same CN.
- Assemble the cert bundle as JSON:
  `{"cert": "...", "key": "...", "ca": "..."}` (see the example
  shape in `infra-hetzner/ca/README.md`).
- Upload each bundle to AWS Secrets Manager under
  `soroban/<environment>/mtls/<cn>` via `aws secretsmanager
put-secret-value`.
  Document the AWS CLI invocation in `infra-hetzner/ca/README.md`.
- Add every new CN to `ALLOWED_CLIENT_CNS` in the operator's
  `~/.config/soroban-prod.env` and replay `ansible-playbook
--tags app` so the Caddy snippet picks them up.

### Phase 2 — Update Lambda code to use mTLS client cert

- Add the AWS Secrets Manager Lambda Extension to every Lambda's
  layers — mounts the secret at `/secrets/<name>` at cold-start
  time without an extra SDK call per invocation.
- ClickHouse client setup in each Lambda:
  - Read cert + key + ca from the mounted secret file.
  - Read `ch_user` + `ch_password` from the same secret bundle
    (see [[task-0240]] for the per-service user matrix —
    `lambda-api-<environment>` cert maps to CH user `api_reader`,
    etc.).
  - Configure HTTPS + mTLS + HTTP Basic Auth on the CH client.
    Target hostname `<ch-prod-domain>:443` (from env var
    `CH_PROD_DOMAIN`).
- Stacks affected:
  - `compute-stack.ts` — main API + indexer Lambdas
  - `ingestion-stack.ts` — ingestion Lambdas (separate from
    Galexie task)
  - `partition-stack.ts` — partition-management Lambda
  - `migration-stack.ts` — migration Lambda
  - `api-gateway-stack.ts` — review for Lambda integration paths

### Phase 3 — Update Galexie ECS task definition

- `ingestion-stack.ts` Galexie task:
  - `NetworkConfiguration.awsvpcConfiguration.assignPublicIp` =
    `ENABLED`
  - Task definition: mount mTLS cert secret from Secrets Manager
    into the container filesystem (ECS native secrets injection)
  - Galexie binary reads cert + connects to
    `<ch-prod-domain>:443`
- **Critical**: `assignPublicIp: ENABLED` is load-bearing —
  without it, Galexie has no egress path (no NAT GW). Add a
  CODEOWNERS-flagged comment in the stack file so any future
  reviewer knows not to flip this back.

### Phase 4 — DNS prerequisite (overlap with 0234)

- [[task-0234]] adds the Route 53 A-record
  `ch-prod.sorobanscan.rumblefish.dev → <box-ipv4>` plus
  `HetznerDnsStack`.
- This task **depends on 0234 landing first** — without real DNS,
  Caddy can't obtain an LE cert, and mTLS smoke against the
  placeholder domain doesn't exercise the real handshake.
- Coordinate ordering: 0234 → smoke tests pass → 0238 phases 1-3.

### Phase 5 — End-to-end smoke per service

Before any decommissioning:

- Each Lambda function: invoke a test path that exercises a
  ClickHouse query, verify success.
- Galexie task: deploy + verify it ingests successfully against
  Hetzner CH.
- Verify CH access logs on the Hetzner box show the expected
  `X-Client-Subject: CN=<service>-<environment>` for each service.
- Verify `ALLOWED_CLIENT_CNS` allowlist is enforced — a Lambda
  presenting a cert with off-list CN gets 403.

### Phase 6 — Decommissioning (irreversible, ordered)

Each step depends on the previous:

1. **Data parity verification** — assert row counts, schema, and
   recent-window sampling match between RDS and ClickHouse.
   Coordinated with [[task-0228]] (parallel-backfill merge).
2. **Remove `rds-stack.ts`** — destroys RDS Postgres + RDS
   Proxy. Snapshot first (manual backup retained 30 days).
3. **Remove Lambda VPC attachment** — Lambdas leave the VPC.
   Verify no Lambda still has `VpcConfig` set after deploy.
4. **Remove NAT Gateway from `network-stack.ts`** — only safe
   after all VPC-attached compute is gone.
5. **Trim `network-stack.ts`** to the minimum VPC needed for
   Galexie (one public subnet, IGW). Consider whether even a VPC
   is needed — Fargate can run in default account VPC if no
   isolation requirement remains.
6. **Remove `bastion-stack.ts`** if its sole purpose was RDS
   access; otherwise scope down.

### Stacks touched (with kind of change)

| Stack                                                                                          | Change                                                                |
| ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| `network-stack.ts`                                                                             | Drop NAT GW, drop private subnets, minimal public subnet for Galexie  |
| `rds-stack.ts`                                                                                 | **REMOVE** (after data parity verified)                               |
| `compute-stack.ts`                                                                             | Lambdas leave VPC, add Secrets Manager extension, mTLS client config  |
| `ingestion-stack.ts`                                                                           | Galexie → public subnet + assignPublicIp; Lambdas same as compute     |
| `partition-stack.ts`                                                                           | Lambda VPC removal + mTLS                                             |
| `migration-stack.ts`                                                                           | Lambda VPC removal + mTLS (Migration cutover work itself is separate) |
| `api-gateway-stack.ts`                                                                         | Review for VPC dependencies; likely no change needed                  |
| `bastion-stack.ts`                                                                             | **REMOVE** if scope was RDS-only                                      |
| `cicd-stack.ts`                                                                                | Add Secrets Manager IAM permissions for deploy role if needed         |
| `cloudwatch-stack.ts`, `observability-stack.ts`, `delivery-stack.ts`, `ledger-bucket-stack.ts` | Review only — likely no VPC dependency                                |

### `infra/envs/production.json` changes

- Change `awsRegion` from `us-east-1` to `eu-central-1`. All
  regional resources (network, lambdas, ECR, ECS, secrets, KMS,
  CloudWatch, regional WAF) move with it. CloudFront cert stays
  in us-east-1 (CloudFront-only requirement) — `certificateArn`
  remains a us-east-1 ARN even after the region change. Document
  this explicitly in the field doc-comment so future readers
  don't "fix" it by changing the region in the ARN.
- Add `mtlsSecretArnPrefix` field. (`chDomainName` already exists
  from [[task-0234]].)
- Drop RDS-related config (`dbInstanceClass`, `dbAllocatedStorage`, etc.).
- Drop NAT-related config if any.
- Update `EnvConfig` interface in `infra/src/lib/types.ts` accordingly.

### Region migration prerequisites (Phase 0)

This phase precedes the AWS-cutover work proper:

1. **[[task-0249]] must complete first** — wipe everything in
   us-east-1 (staging + any prod stacks). Greenfield deploy in
   eu-central-1 starts from an empty regional footprint, avoiding
   the "two NAT GWs, two RDS instances" cost overlap during the
   transition.
2. **CDK bootstrap eu-central-1**:
   ```bash
   npx cdk bootstrap aws://<account>/eu-central-1
   ```
3. **ACM certificate for `*.sorobanscan.rumblefish.dev` in
   eu-central-1** — required for API Gateway custom domain in the
   new region. Issue via AWS Console or CDK (DNS validation
   against the existing Route 53 hosted zone, which is global and
   unaffected by the region change).
4. **Update `infra/envs/production.json:awsRegion`** to
   `eu-central-1`. Validate `cdk synth` is clean before any deploy.
5. **ECR repositories** — push the indexer / api / galexie images
   to the new region's ECR. The us-east-1 images are deleted as
   part of 0249.

After Phase 0, the Phase 1-6 work below executes against the new
region from the start — no in-place "migration" of existing
stacks.

## Acceptance Criteria

- [⏸️] All Lambdas successfully query CH via mTLS from public-internet
  path (no VPC config, no RDS connection). _Deferred to task 0241
  Part D — first realistic deploy of the new region happens at the
  indexer cutover._
- [⏸️] Galexie ECS task connects to both Stellar peer overlay AND
  Hetzner CH from public subnet (`assignPublicIp: ENABLED`).
  _Deferred to task 0241 Part D._
- [⏸️] Caddy access logs on the Hetzner box show
  `X-Client-Subject: CN=<service>-<environment>` for each AWS service
  that exercises a CH query. _Deferred to task 0241 Part D._
- [⏸️] Off-allowlist CN gets 403 at the HTTP layer (defence-in-depth
  enforcement test). _Deferred to task 0241 Part D._
- [x] NAT Gateway removed from production CDK; `cdk synth` confirms
      no NAT GW resource. _`cdk.out/Explorer-production-Network.template.json`
      contains zero `AWS::EC2::NatGateway` resources._
- [x] RDS stack removed from production CDK; manual snapshot
      retained 30 days as final rollback insurance. _Code: rds-stack.ts
      moved to `.trash/` and dropped from app.ts. Snapshot N/A — per
      task 0249 archive, no production RDS was ever deployed
      (validateConfig blocked on `hostedZoneId: CHANGE_ME` for the
      entire pre-0239 lifetime), so there is nothing to snapshot._
- [x] mTLS client cert rotation strategy documented (auto-renew via
      a scheduled Lambda or follow-up task; current cert lifetime is
      365 days per `issue-client-cert.sh`). _Spawned as task 0253
      (FEATURE: mTLS client cert auto-rotation pipeline)._
- [x] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.
- [x] **Docs updated** —
      `docs/architecture/infrastructure/infrastructure-overview.md`
      updated to reflect new AWS topology (Lambda out-of-VPC,
      Galexie public subnet, no RDS, no NAT GW). _Sections updated:
      §2.2 (managed runtime list), §4.2 (deployment topology + region
      change), §4.3 (trust boundaries), §5.1 (Galexie public subnet +
      load-bearing assignPublicIp note), §5.2 (no RDS — Hetzner CH is
      the data plane), §5.3 (Ledger Processor on mTLS), §5.4 (API
      Lambda on mTLS), §5.5 (SM stores mTLS bundles), §5.6 (cert
      distribution narrative), §6.1 (network shape), §6.2 (secret
      handling), §7.1 (environment model — no staging), §7.2-7.4
      (scaling / availability / production-only limits), §8.1-8.2
      (CloudWatch surface — no RDS metrics, CH on Hetzner)._

## Dependencies

- [[task-0249]] — destroy us-east-1 AWS infra first. Greenfield
  deploy in eu-central-1 only makes sense once the old region is
  empty (no orphan stacks, no NAT-GW cost overlap).
- [[task-0227]] — Hetzner-side artefacts must be deployed and
  validated (✅ delivered, awaiting first prod traffic).
- [[task-0234]] — Route 53 A-record + LE cert must be live before
  this task's mTLS smoke is meaningful. Cannot proceed past
  Phase 4 without 0234.
- [[task-0228]] — parallel-backfill merge into Hetzner CH must
  complete before Phase 6 step 1 (data parity) can sign off.
  RDS cannot be dropped until ClickHouse holds the canonical
  dataset.
- [[task-0240]] — ClickHouse per-service users + RBAC + quotas
  (Layer 3 defense-in-depth) must be live before Phase 2 (Lambda
  code starts depending on per-service `ch_user` / `ch_password`
  fields in the Secrets Manager bundle) and definitively before
  Phase 6 step 2 (drop RDS — without Layer 3 a compromised
  Lambda has the same blast radius as a compromised RDS
  password).

## Risks / Considerations

- **`assignPublicIp: ENABLED` is load-bearing** for Galexie. Any
  future change to `ingestion-stack.ts` that removes or disables
  this attribute breaks Galexie's connectivity (no NAT GW =
  no other egress path). Add a prominent inline comment + flag
  in CODEOWNERS so reviewers catch it.
- **Secrets Manager extension cold-start cost** — the extension
  adds ~50 ms to first-invoke Lambda cold-start. For latency-
  sensitive endpoints (API Gateway), evaluate whether to switch
  to provisioned concurrency or a different secret-fetch
  mechanism. Probably acceptable for the indexer + migration
  paths; verify before assuming.
- **Cert rotation** — `issue-client-cert.sh` mints 365-day certs.
  Before any cert expires, Secrets Manager value must be updated
  - Lambdas redeployed (or use a runtime that refreshes from SM
    on demand). Either solve in this task or spawn a follow-up
    ("automated cert rotation pipeline").
- **Cutover ordering is irreversible past Phase 6 step 2** —
  dropping RDS deletes a managed snapshot's source. Take a final
  manual snapshot retained 30 days as the only rollback path.
- **Galexie's source IP changes per task restart** — if any
  downstream Stellar peer expects a stable client IP, that
  assumption breaks. Confirm Stellar peer overlay tolerates IP
  churn (it should — gossip protocol assumes mobile peers).
- **No bastion → no emergency RDS access** — once `bastion-stack.ts`
  is removed alongside RDS, there is no out-of-band path to a
  hypothetical recovery RDS. If RDS comes back for any reason
  (rollback, new use case), the bastion + IAM auth path must be
  rebuilt. Acceptable for a clean cutover.

## Out of Scope

- ClickHouse replication / multi-region HA — separate architectural
  task if/when needed.
- Data migration mechanics (RDS → ClickHouse) — handled by
  [[task-0228]] (parallel-backfill merge + validation).
- Hetzner-side monitoring / Prometheus scraper — separate task
  in the observability track.
- API Gateway custom domain / TLS termination changes — current
  CloudFront-fronted setup unaffected; only the Lambda backend
  changes how it reaches CH.
- Migrating `infra/src/` away from CDK to another IaC tool — out
  of scope; this task stays within CDK conventions.
- Multi-environment rollout — this task targets production only.
  Staging is being decommissioned entirely by [[task-0249]] and is
  not re-deployed in eu-central-1; there is no AWS-side staging
  until product explicitly asks for one.
- Lambda Rust application code — agreed mid-task (2026-05-21) that
  0239 is **infra-only**. `crates/api`, `crates/indexer`,
  `crates/db-migrate`, `crates/db-partition-mgmt`,
  `crates/enrichment-worker` continue querying PG until their
  respective migration tasks land (0241 for the indexer; analogous
  tasks for the rest if/when prioritised). 0239 lays the AWS-side
  wiring (out-of-VPC, SM Extension layer, mTLS env vars, IAM grants
  on per-service secret ARNs) so those code migrations only have
  to update the Rust side.

## Implementation Notes

**Files modified (16):**

- `docs/architecture/infrastructure/infrastructure-overview.md` —
  §§2.2, 4.2-4.3, 5.1-5.6, 6.1-6.2, 7.1-7.4, 8.1-8.2 rewrites
  reflecting the post-0239 topology (Lambdas out-of-VPC, Galexie
  public subnet, Hetzner-CH data plane). §§3.1-3.2 and §5.2 paragraphs
  describing the original AWS-VPC topology preserved verbatim per the
  existing "§§3–5.5 represents the original infrastructure design"
  framing, with explicit pointer to §4.2 / §5.6 for the new state.
- `infra/Makefile` — only `production-*` and `cicd-*` targets; all
  `staging-*` and `bastion-*` and `rds-*` targets dropped.
- `infra/cdk.json` — `app` field updated from the (now-deleted)
  `dist/bin/staging.js` to `dist/bin/production.js`.
- `infra/envs/cicd.json` — `awsRegion` flipped to `eu-central-1`.
- `infra/envs/production.json` — `awsRegion: "eu-central-1"`; RDS /
  NAT fields removed; `certificateArn` split into
  `cloudFrontCertificateArn` (us-east-1, hard CloudFront requirement)
  and `apiCertificateArn` (eu-central-1, API Gateway regional);
  added `mtlsSecretNamePrefix`.
- `infra/src/index.ts` — dropped `RdsStack` / `RdsStackProps`
  exports.
- `infra/src/lib/app.ts` — drop RDS wiring + bastion app + `dbSecret`
  / `dbProxyEndpoint` passing.
- `infra/src/lib/ports.ts` — drop unused `POSTGRESQL_PORT` constant
  after the RDS removal.
- `infra/src/lib/types.ts` — `EnvironmentConfig` interface
  refactored: dropped 9 RDS / NAT fields, added 3 mTLS-related ones
  (`mtlsSecretNamePrefix`, `cloudFrontCertificateArn`,
  `apiCertificateArn`), narrowed `envName` union to `'production'`.
  `validateConfig` updated to reject wrong-region cert ARNs at synth
  time.
- `infra/src/lib/stacks/api-gateway-stack.ts` — read `apiCertificateArn`
  instead of `certificateArn`.
- `infra/src/lib/stacks/cicd-stack.ts` — staging deploy role dropped
  (per task 0249); OIDC thumbprint replaced with AWS-documented value.
- `infra/src/lib/stacks/cloudwatch-stack.ts` — RDS CPU / free-storage
  alarms removed; RDS dashboard widgets removed; `rdsInstance` prop
  removed.
- `infra/src/lib/stacks/compute-stack.ts` — Lambdas out of VPC
  (`vpc`, `securityGroups`, `vpcSubnets`, `dbSecret`,
  `dbProxyEndpoint` props all dropped); `lambda.LayerVersion` for
  the AWS Parameters and Secrets Lambda Extension attached to each
  Lambda; per-Lambda secret name + `secretsmanager:GetSecretValue`
  IAM grant scoped via the wildcard ARN helper in `mtls.ts`.
- `infra/src/lib/stacks/delivery-stack.ts` — read
  `cloudFrontCertificateArn` instead of `certificateArn`.
- `infra/src/lib/stacks/ingestion-stack.ts` — Galexie service
  subnet flipped to `PUBLIC` with `assignPublicIp: true` and a
  CODEOWNERS-flagged inline comment explaining why this is
  load-bearing; ECS native Secrets Manager injection for the
  `{cert, key, ca}` bundle; entrypoint shell materialises PEMs to
  `/tmp/{cert,key,ca}.pem` via `umask 077`.
- `infra/src/lib/stacks/migration-stack.ts` — out-of-VPC + SM
  Extension layer + per-service mTLS secret.
- `infra/src/lib/stacks/network-stack.ts` — minimum VPC, public
  subnet only, `natGateways: 0`, Lambda SG removed, S3 Gateway
  endpoint removed (was private-subnet-only).
- `infra/src/lib/stacks/partition-stack.ts` — same treatment as
  migration / compute.

**Files added (1):**

- `infra/src/lib/mtls.ts` — `secretsManagerLayerArn(region)` (per-region
  AWS-managed layer ARN map, pinned to versions captured 2026-05-21
  from the AWS docs — eu-central-1 ARM64 `:78`, us-east-1 ARM64 `:80`)
  and `mtlsSecretArn(scope, secretName)` (builds the wildcard-suffixed
  ARN for IAM grants, with a guard rejecting `*` / `?` in the secret
  name to prevent accidental over-scoping).

**Files moved to `.trash/`** (7, per project no-`rm` policy):

- `infra/envs/staging.json`
- `infra/src/bin/bastion-{production,staging}.ts`
- `infra/src/bin/staging.ts`
- `infra/src/lib/bastion-app.ts`
- `infra/src/lib/stacks/bastion-stack.ts`
- `infra/src/lib/stacks/rds-stack.ts`
- `infra/cdk.context.json` (stale lookup cache from the us-east-1
  staging era — moved here as part of the audit pass when `cdk synth`
  failed trying to refresh the lookup against a region without bootstrap)

**Validation:**

- `nx build @rumblefish/soroban-block-explorer-aws-cdk` — PASS
- `cdk synth` — produces 11 production templates: ApiGateway,
  CloudWatch, Compute, Delivery, HetznerDns, Ingestion, LedgerBucket,
  Migration, Network, Observability, Partition. Empirical jq check
  of the key templates: Network has zero `AWS::EC2::NatGateway`
  resources and a single public subnet; Compute Lambdas have
  `VpcConfig: null` plus the SM Extension layer; Ingestion task
  has `AssignPublicIp: ENABLED` plus ECS secrets injection from
  `soroban/production/mtls/galexie-production`.
- Three independent subagent audits run (correctness, security,
  spec compliance) — all five flagged items addressed in this
  task before closure.

## Design Decisions

### From Plan

1. **Lambdas out-of-VPC, Galexie public subnet** — explicit in the
   task scope. Per-task public IPv4 is the only egress path; flagged
   prominently in `ingestion-stack.ts:283-291` so a future reviewer
   does not flip it back.
2. **Bundle in SM is `{cert, key, ca}` JSON** — per task 0240's
   redesign (history entry 2026-05-21). Identity is Caddy proxy-trust
   via CN→user map; no `ch_user` / `ch_password` fields needed.
3. **CloudFront cert pinned to `us-east-1`** — hard CloudFront
   constraint; `cloudFrontCertificateArn` field doc explicit about
   this, `validateConfig` enforces the region regex.
4. **`mtlsSecretNamePrefix` as env config** — keeps the secret naming
   convention out of CDK code, lets future multi-env redeploys (if
   ever reintroduced) reuse the stacks with a different prefix.
5. **Single big PR** — agreed approach during planning (vs. fazowane
   PRs). Greenfield deploy means no in-flight prod to break by
   landing a partial state.

### Emerged

6. **CICD rebuild absorbed into 0239** — task 0249 said "rebuild
   `Explorer-Cicd` as part of 0239" but the spec body did not call it
   out explicitly. Decision (2026-05-21): update `infra/envs/cicd.json`
   region in this PR.
7. **CICD only provisions a production deploy role** — staging is
   retired (0249); the old `for (envName of ['staging', 'production'])`
   loop collapsed to a single block.
8. **Drop staging CDK files entirely (not just deploy targets)** —
   `infra/envs/staging.json`, `bin/staging.ts`, `bin/bastion-*.ts`,
   `lib/bastion-app.ts`, `lib/stacks/bastion-stack.ts` all moved to
   `.trash/`. 0249 only destroyed deployed resources; the CDK
   definitions were leftover dead code.
9. **Cert ARN split into two fields** — `cloudFrontCertificateArn`
   (us-east-1) vs `apiCertificateArn` (eu-central-1). Previously a
   single `certificateArn` field was shared between CloudFront and
   API Gateway, which worked when everything was in `us-east-1`.
   Post-region-change, CloudFront still needs `us-east-1` and API
   Gateway needs `eu-central-1` — same wildcard cert content, two
   different region-bound ACM ARNs.
10. **AWS Parameters and Secrets Lambda Extension** for cert fetch
    in Lambdas — task spec mentioned "mounts the secret at
    /secrets/<name>", but in reality the extension exposes a local
    HTTP cache (port 2773), not a mount. CDK attaches the layer and
    sets `MTLS_SECRET_NAME`+`CH_DOMAIN` env; the actual fetch +
    file-write happens in Lambda Rust code under task 0241 / future
    code migration tasks.
11. **ECS native `Secret.fromSecretsManager` with JSON-key extraction**
    for Galexie — one ECS env var per `{cert,key,ca}` field; the
    container entrypoint writes them to `/tmp/*.pem` with `umask 077`.
    Picked this over the Lambda-extension equivalent (no ECS counterpart)
    and over a sidecar (one less moving piece).
12. **Pinned extension layer version vs SSM dynamic reference** — AWS
    publishes the layer ARN as both a versioned ARN and an SSM public
    parameter (`/aws/service/aws-parameters-and-secrets-lambda-extension/arm64/latest`)
    that always resolves to the latest. Picked manual pin (eu-central-1
    `:78`, us-east-1 `:80`) for reproducibility; trade-off documented
    in `mtls.ts` doc-comment.
13. **`restrictDefaultSecurityGroup: true` retained** — strips the
    default VPC SG's ingress/egress via a CDK-deployed custom resource.
    Important now that the VPC is public-only: any compute that lands
    in the default SG would otherwise have wide-open egress.
14. **Phase 5 + 6 acceptance criteria deferred to 0241 Part D** —
    closing 0239 at code-side completion. First realistic prod deploy
    of the new region happens at the indexer cutover (0241) when the
    Lambda Rust code is CH-aware; running smoke tests before that
    would fail on the cold-start PG connection (no RDS).
15. **GitHub OIDC thumbprint placeholder replaced** with AWS's
    documented current GitHub root CA thumbprint
    (`6938fd4d98bab03faadb97b34396831e3780aea1`) instead of `f`×40.
    Functionally identical (GitHub uses JWKS, not thumbprint, for
    validation), but passes compliance-scanner regex looking for a
    realistic value.

## Issues Encountered

- **Stale `cdk.context.json` blocked synth in eu-central-1.** First
  `make synth-production` post-refactor failed at
  `/Explorer-production-Network` with "AWS was not able to validate
  the provided access credentials" because CDK was trying to refresh
  a us-east-1 staging VPC lookup against eu-central-1 (no bootstrap
  done yet, no creds for that region in scope of the synth call).
  Fix: moved `cdk.context.json` to `.trash/cdk.context.json.0239` —
  the new code does no `fromLookup` calls, so the file is dead. The
  templates regenerated cleanly on the next synth.
- **`cdk synth` triggers a full Rust `--release --arm64` rebuild via
  cargo-lambda** — cold cache, 5–10 minutes per Lambda binary across
  api / indexer / enrichment-worker / db-migrate / db-partition-mgmt.
  Not a quick TS validator. Captured in memory
  `[[feedback-cdk-synth-triggers-full-rust-rebuild]]` so future
  sessions default to `nx build` for TS sanity checks and reserve
  synth for actual deploy preparation.
- **GitHub OIDC thumbprint stylistic finding** — Agent 2's security
  audit flagged the `f`×40 placeholder as a compliance-scanner false
  positive risk. Replaced with the real-looking current value.
- **`mtlsSecretArn` ARN construction was non-defensive** — Agent 2
  flagged the helper as silently widening IAM grants if the secret
  name ever contained `*` / `?`. Added an explicit guard that throws
  at synth time.
- **Docs sections §6.2 / §7 / §8.1 / §8.2 still claimed RDS as the
  production data plane** — Agent 3's spec-compliance audit caught
  the stale "production RDS storage is encrypted at rest" /
  "Staging using a separate RDS instance" / "RDS CPU and connection
  metrics" wording. The "§§3–5.5 preserved verbatim" framing in §5.6
  was always intended to cover only §§3–5.5, but reading the rest of
  the doc made it look like RDS was still part of the current target
  topology. Rewrote those sections.
