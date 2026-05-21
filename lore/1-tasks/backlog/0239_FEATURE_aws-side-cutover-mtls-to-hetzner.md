---
id: '0239'
title: 'FEATURE: AWS-side cutover — Lambdas out-of-VPC, Galexie public subnet, mTLS to Hetzner CH, NAT GW + RDS decommission, region eu-central-1'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0216', '0227', '0228', '0234', '0240', '0249']
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

- [ ] All Lambdas successfully query CH via mTLS from public-internet
      path (no VPC config, no RDS connection).
- [ ] Galexie ECS task connects to both Stellar peer overlay AND
      Hetzner CH from public subnet (`assignPublicIp: ENABLED`).
- [ ] Caddy access logs on the Hetzner box show
      `X-Client-Subject: CN=<service>-<environment>` for each AWS service
      that exercises a CH query.
- [ ] Off-allowlist CN gets 403 at the HTTP layer (defence-in-depth
      enforcement test).
- [ ] NAT Gateway removed from production CDK; `cdk diff` confirms
      no NAT GW resource.
- [ ] RDS stack removed from production CDK; manual snapshot
      retained 30 days as final rollback insurance.
- [ ] mTLS client cert rotation strategy documented (auto-renew via
      a scheduled Lambda or follow-up task; current cert lifetime is
      365 days per `issue-client-cert.sh`).
- [ ] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.
- [ ] **Docs updated** —
      `docs/architecture/infrastructure/infrastructure-overview.md`
      updated to reflect new AWS topology (Lambda out-of-VPC,
      Galexie public subnet, no RDS, no NAT GW).

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
