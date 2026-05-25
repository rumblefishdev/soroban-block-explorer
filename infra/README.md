# AWS CDK Infrastructure

CDK stacks for the Soroban Block Explorer. Defines all AWS resources: networking, storage, compute, delivery, and monitoring.

## Stack architecture

```
NetworkStack (VPC, subnets, security groups, VPC endpoints)
    |
StorageStack (RDS, RDS Proxy, S3, Secrets Manager)
    |
    +-- ApiStack (API Lambda, API Gateway)
    +-- IndexerStack (Rust Ledger Processor Lambda, SQS DLQ)
    +-- IngestionStack (ECS Fargate Galexie, ECR)
    +-- FrontendStack (CloudFront, WAF, Route 53)
    |
MonitoringStack (CloudWatch, alarms, X-Ray)
```

Currently implemented: **NetworkStack**.

## Prerequisites

- AWS CLI with a configured profile
- Node.js 22+
- `export AWS_PROFILE=soroban-explorer`

## Commands

From `infra/`:

```bash
# First-time setup (once per AWS account + region)
make bootstrap

# Staging
make diff-staging              # Preview changes
make deploy-staging            # Deploy all stacks
make deploy-staging-network    # Deploy single stack

# Production
make diff-production
make deploy-production
make deploy-production-network

# Bastion (separate CDK app, not included in deploy --all)
make deploy-staging-bastion    # Deploy bastion host
make destroy-staging-bastion   # Destroy bastion host
```

Or from the repository root:

```bash
npm run infra:diff:staging
npm run infra:deploy:staging
```

## Connecting to RDS

RDS is in a private subnet with no public access. Use SSM Session Manager port forwarding through a bastion host.

### Prerequisites

```bash
brew install session-manager-plugin
```

### Setup (one-time)

1. Deploy the main infrastructure (if not already done):

   ```bash
   make deploy-staging
   ```

2. Deploy the bastion host:

   ```bash
   make deploy-staging-bastion
   ```

### Open tunnel

```bash
npm run db:tunnel              # staging, localhost:15432
npm run db:tunnel -- staging 5433  # custom local port
```

### Connect with DBeaver / psql

| Field    | Value                                                            |
| -------- | ---------------------------------------------------------------- |
| Host     | `localhost`                                                      |
| Port     | `15432`                                                          |
| Database | `soroban_explorer`                                               |
| User     | From Secrets Manager: `soroban-explorer/staging/rds-credentials` |
| Password | From Secrets Manager: `soroban-explorer/staging/rds-credentials` |

To retrieve credentials: AWS Console → Secrets Manager → `soroban-explorer/staging/rds-credentials` → "Retrieve secret value".

### Teardown

Destroy the bastion when not needed ($0 when stack is destroyed):

```bash
make destroy-staging-bastion
```

### How it works

```
Your laptop (localhost:15432)
  → HTTPS (443) → AWS SSM Service
    → SSM Agent on bastion EC2
      → RDS (port 5432)
```

No SSH, no open ports, no VPN, no IP whitelisting required. Only valid AWS credentials with `ssm:StartSession` permission.

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
    --region us-east-1
```

Subsequent IP rotations (after a box replacement) use `--overwrite` on the same command — no CDK code change, no PR.

### Deploy

```bash
make deploy-production-hetzner-dns
```

`cdk synth` does NOT require AWS auth for the IP — the SSM value is rendered as a CFN dynamic reference (`{{resolve:ssm:/soroban/production/ch-ip}}`) and resolved by CloudFormation at deploy time. If the parameter is missing, the deploy fails with a CFN error pointing at the parameter name.

## Environments

Each environment has its own JSON config and CDK entry point:

| Environment | Config                 | Entry point             | VPC CIDR    | NAT             |
| ----------- | ---------------------- | ----------------------- | ----------- | --------------- |
| staging     | `envs/staging.json`    | `src/bin/staging.ts`    | 10.0.0.0/16 | Managed gateway |
| production  | `envs/production.json` | `src/bin/production.ts` | 10.1.0.0/16 | Managed gateway |

## Project structure

```
envs/
  staging.json               # Staging environment config
  production.json            # Production environment config
src/
  bin/
    staging.ts               # Main CDK app entry point — staging
    production.ts            # Main CDK app entry point — production
    bastion-staging.ts       # Bastion CDK app entry point — staging
    bastion-production.ts    # Bastion CDK app entry point — production
  lib/
    types.ts               # EnvironmentConfig interface
    app.ts                 # Main app stack wiring (createApp)
    bastion-app.ts         # Bastion app (createBastionApp)
    ports.ts               # Shared port constants
    stacks/
      network-stack.ts     # VPC, subnets, SGs, S3 VPC endpoint
      rds-stack.ts         # RDS PostgreSQL, RDS Proxy, Secrets Manager
      ledger-bucket-stack.ts # S3 bucket for ledger XDR files
      migration-stack.ts   # DB migration Lambda (custom resource)
      compute-stack.ts     # API + Indexer Lambdas
      bastion-stack.ts     # Bastion host for SSM port forwarding
Makefile                     # Deploy/synth/diff targets per environment
```

## NetworkStack resources

- VPC with /16 CIDR in us-east-1
- Public subnet (/20) with Internet Gateway
- Private subnet (/20) with NAT Gateway
- Lambda security group (outbound: RDS 5432, HTTPS 443)
- RDS security group (inbound: Lambda + ECS on 5432)
- ECS security group (outbound: HTTPS 443, RDS 5432, Stellar peers 11625)
- S3 Gateway VPC endpoint on private subnet route table

Single-AZ deployment in us-east-1a. Multi-AZ expansion requires only config changes.
