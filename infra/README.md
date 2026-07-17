# AWS CDK Infrastructure

CDK stacks for the Soroban Block Explorer — the AWS plane (networking,
compute, ingestion, delivery, monitoring). This file is the **stack
reference**; for the operational "what command ships what" guide see
**[`docs/deployment.md`](../docs/deployment.md)**.

> **Data plane note:** the production database is **Hetzner-hosted
> ClickHouse** ([`infra-hetzner/`](../infra-hetzner/README.md)), not AWS
> RDS. RDS and the Postgres migration/bastion stacks were removed (tasks
> 0239 / 0241).

## Stacks

Production app — `src/bin/production.ts`, config `envs/production.json`:

```
NetworkStack             minimal VPC, public subnet only (no NAT, no RDS)
LedgerBucketStack        S3 stellar-ledger-data (Galexie → indexer)
ComputeStack             API + Indexer + Enrichment Lambdas + SQS/DLQ
IngestionStack           Galexie ECS Fargate + ECR
DeliveryStack            CloudFront + SPA S3 bucket
CloudFrontWafStack       CloudFront WebACL (conditional on enableWaf)
ApiGatewayStack          REST API Gateway (cache, throttle, mTLS lock)
ObservabilityStack       X-Ray / logging
CloudWatchStack          alarms + dashboards
HetznerDnsStack          Route 53 A record → CH box IP
CloudflareBootstrapStack TF-state bucket for infra/cloudflare/
```

## Prerequisites

- AWS CLI with a named profile that has deploy rights (`export AWS_PROFILE=…`).
  Region is read from `envs/production.json` (`eu-central-1`) — do not pass `--region`.
- Node.js 22+ (`.nvmrc`), `npm ci` at the repo root.
- Rust toolchain + `cargo-lambda` + `zig` — the API/indexer/enrichment Lambdas
  are Rust, cross-compiled at synth time (`ComputeStack` fails to build without them).

## Commands

Production is the only environment; there is **no staging** (retired by task
0249). From `infra/`:

```bash
make bootstrap                 # first-time only, once per AWS account + region
make deploy-cicd               # first-time only, shared OIDC/CICD roles

make diff-production           # preview all stacks
make deploy-production         # deploy ALL stacks
make deploy-production-compute # deploy a single stack (here: API/indexer/enrichment)
```

The full per-stack target list is in the [`Makefile`](Makefile). The
operational guide (which command for which change, gotchas, verification) is
**[`docs/deployment.md`](../docs/deployment.md)**.

## Database access

The production database is **Hetzner-hosted ClickHouse**, reached over mTLS —
not AWS RDS. RDS, the bastion host, and the SSM port-forward tunnel described
in earlier versions of this file were removed (tasks 0239 / 0241). For CH
access and credentials see
[`infra-hetzner/README.md`](../infra-hetzner/README.md) and
[`docs/architecture/security/clickhouse-rbac.md`](../docs/architecture/security/clickhouse-rbac.md).

## HetznerDnsStack — Route 53 record for the production ClickHouse box

`HetznerDnsStack` provisions a Route 53 A record targeting the
Hetzner-hosted ClickHouse server's public IPv4. The FQDN comes
verbatim from the `chDomainName` field in the env config
(`envs/${env}.json`) — for example, production today resolves to
`ch.sorobanscan.rumblefish.dev`. Caddy on the box uses this name
for the Let's Encrypt HTTP-01 challenge; AWS-side workloads use
it as the mTLS endpoint.

### One-time setup (per environment, before first deploy)

The box IPv4 is intentionally NOT in `envs/${env}.json` — that matches the existing `inventory.ini`-is-gitignored convention. It lives in SSM Parameter Store:

```bash
aws ssm put-parameter \
    --name /soroban/production/ch-ip \
    --value <dedicated-server-ipv4> \
    --type String \
    --region eu-central-1        # MUST match awsRegion — the stack resolves
                                 # the param in its own region (hetzner-dns-stack.ts)
```

Subsequent IP rotations (after a box replacement) use `--overwrite` on the same command — no CDK code change, no PR.

### Deploy

```bash
make deploy-production-hetzner-dns
```

`cdk synth` does NOT require AWS auth for the IP — the SSM value is rendered as a CFN dynamic reference (`{{resolve:ssm:/soroban/production/ch-ip}}`) and resolved by CloudFormation at deploy time. If the parameter is missing, the deploy fails with a CFN error pointing at the parameter name.

## Environments

Each environment has its own JSON config and CDK entry point:

| Environment   | Config                 | Entry point             | Notes                                |
| ------------- | ---------------------- | ----------------------- | ------------------------------------ |
| production    | `envs/production.json` | `src/bin/production.ts` | the only runtime env; `eu-central-1` |
| cicd (shared) | `envs/cicd.json`       | `src/bin/cicd.ts`       | OIDC/CICD roles; `make deploy-cicd`  |

AWS-side staging was retired by task 0249 — there is no `staging.json` or
`bin/staging.ts`.

## Project structure

```
envs/
  production.json          # Production environment config
  cicd.json                # Shared CICD/OIDC config
src/
  bin/
    production.ts          # Main CDK app entry point — production
    cicd.ts                # CICD app entry point (OIDC deploy roles)
  lib/
    types.ts               # EnvironmentConfig interface
    app.ts                 # Main app stack wiring (createApp)
    cicd-app.ts            # CICD app wiring
    ports.ts               # Shared port constants
    stacks/                # one file per stack (see "Stacks" above)
Makefile                   # deploy/synth/diff targets — production + cicd
```

## NetworkStack resources

Post-task-0239 the VPC is intentionally minimal (`eu-central-1`, single AZ):

- VPC with a public subnet + Internet Gateway only
- **No NAT Gateway, no private subnet, no RDS** — Lambdas run out-of-VPC and
  Galexie gets a per-task public IPv4 (`assignPublicIp: ENABLED`), which is
  its only egress path
- ECS security group for the Galexie task (egress: HTTPS 443, Stellar peers
  11625, Hetzner CH over mTLS)

AWS → Hetzner ClickHouse identity is asserted by mTLS, not VPC isolation —
there is no AWS-side database to wall off. See
[`docs/architecture/infrastructure/infrastructure-overview.md`](../docs/architecture/infrastructure/infrastructure-overview.md).
