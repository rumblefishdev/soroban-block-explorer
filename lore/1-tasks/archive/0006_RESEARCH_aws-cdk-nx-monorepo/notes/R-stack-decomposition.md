---
title: 'CDK stack decomposition and dependency ordering'
type: research
status: developing
spawned_from: null
spawns: []
tags: [cdk, stacks, architecture]
links:
  - https://docs.aws.amazon.com/cdk/v2/guide/stacks.html
history:
  - date: 2026-03-26
    status: developing
    who: stkrolikiewicz
    note: 'Initial decomposition based on architecture docs and deployment ordering'
---

# CDK stack decomposition and dependency ordering

## Approach: Layer-Based Stacks

One stack per infrastructure layer, not per service. This matches the deployment ordering and minimizes cross-stack references while keeping each stack focused.

## Recommended Stack Structure

> Updated 2026-03-27: Compute split into per-service stacks. Delivery merged with compute — API Gateway sits with its Lambda, CloudFront sits with its S3 origin. Cleaner ownership, fewer cross-stack refs.

```
infra/aws-cdk/
├── bin/
│   └── app.ts                    # CDK app entry — instantiates all stacks
├── lib/
│   ├── stacks/
│   │   ├── network-stack.ts      # VPC, subnets, SGs, VPC endpoints, NAT Gateway
│   │   ├── storage-stack.ts      # RDS, RDS Proxy, S3 buckets, Secrets Manager
│   │   ├── api-stack.ts          # API Lambda, API Gateway, ACM cert
│   │   ├── indexer-stack.ts      # Rust Ledger Processor Lambda (cargo-lambda-cdk), S3 trigger, SQS DLQ
│   │   ├── ingestion-stack.ts    # ECS Fargate (Galexie), ECR, EventBridge scheduler
│   │   ├── frontend-stack.ts     # CloudFront, WAF, Route 53, frontend S3 deploy
│   │   ├── monitoring-stack.ts   # CloudWatch dashboards, alarms, X-Ray
│   │   └── ci-stack.ts           # OIDC provider, deploy IAM roles (per ADR-0001)
│   ├── config/
│   │   ├── index.ts              # Config loader
│   │   ├── staging.ts            # Staging-specific values
│   │   └── production.ts         # Production-specific values
│   └── constructs/
│       ├── rust-lambda.ts        # Rust Lambda construct wrapper (cargo-lambda-cdk)
│       ├── nestjs-lambda.ts      # NestJS Lambda construct wrapper
│       └── galexie-service.ts    # ECS Fargate Galexie construct
├── cdk.json
├── package.json
└── tsconfig.json
```

## Deployment Order (Stack Dependencies)

```
CiStack (OIDC, deploy roles)
    ↓
NetworkStack (VPC, subnets, SGs, endpoints)
    ↓
StorageStack (RDS, RDS Proxy, S3 buckets, Secrets Manager)
    ↓ ↓ ↓ ↓
    │ │ │ └─ FrontendStack (CloudFront, WAF, Route 53, frontend S3 deploy)
    │ │ └─── IngestionStack (ECS Fargate Galexie, ECR, EventBridge)
    │ └───── IndexerStack (Rust Ledger Processor Lambda, S3 trigger, SQS DLQ)
    └─────── ApiStack (API Lambda, API Gateway, ACM cert)
                 ↓
          MonitoringStack (CloudWatch, alarms, X-Ray)
```

### Why per-service compute stacks:

1. **API Gateway belongs with API Lambda** — tightly coupled, same deployment lifecycle. Splitting them into compute + delivery creates unnecessary cross-stack refs.
2. **CloudFront belongs with frontend** — configuration includes origin (S3), WAF, Route 53 — all delivery-specific. Not coupled to any Lambda.
3. **Indexer is independent** — Rust Lambda with S3 trigger and DLQ. Can be deployed/rolled back without touching the API.
4. **Ingestion is independent** — ECS Fargate + ECR. Different scaling, different lifecycle than Lambdas.
5. **Parallel deployment** — after StorageStack, all 4 service stacks can deploy in parallel.

### Cross-Stack References

| From            | To             | What                                       |
| --------------- | -------------- | ------------------------------------------ |
| StorageStack    | NetworkStack   | VPC, private subnets, security groups      |
| ApiStack        | NetworkStack   | VPC, subnets, SGs                          |
| ApiStack        | StorageStack   | RDS Proxy endpoint, secret ARNs            |
| IndexerStack    | NetworkStack   | VPC, subnets, SGs                          |
| IndexerStack    | StorageStack   | RDS Proxy endpoint, S3 bucket ARN, secrets |
| IngestionStack  | NetworkStack   | VPC, subnets, SGs                          |
| IngestionStack  | StorageStack   | ECR (if shared), S3 bucket                 |
| FrontendStack   | StorageStack   | api-docs S3 bucket                         |
| MonitoringStack | ApiStack       | API Lambda name, API Gateway ID            |
| MonitoringStack | IndexerStack   | Processor Lambda name, DLQ ARN             |
| MonitoringStack | IngestionStack | ECS service name                           |
| MonitoringStack | StorageStack   | RDS instance ID                            |

## Stack vs Construct Decision

**Stacks** = deployment units (CloudFormation stacks). Independent deploy, rollback, IAM boundaries.

**Constructs** = reusable components within a stack. Shared patterns (e.g., `RustLambda` construct used in ComputeStack).

Rule: if two resources MUST be deployed together → same stack. If they CAN be deployed independently → separate stacks.

## Alternative Considered: Single Stack

A single "ExplorerStack" with all resources.

**Pros:** No cross-stack references, simpler initial setup.
**Cons:** All-or-nothing deployment, 500-resource CloudFormation limit risk, slow deploys, can't independently update monitoring without redeploying compute.

**Rejected** for a system with ~20+ AWS resources that will grow.

## Alternative Considered: Per-Service Stacks

Separate stacks for each service (LambdaProcessorStack, GalexieStack, RdsStack, etc.).

**Pros:** Maximum isolation.
**Cons:** Too many stacks (~10+), complex cross-stack reference web, slow parallel deployment, harder to reason about.

**Rejected** as over-granular for a team of 2.

## Multi-AZ Expansion

The stack structure supports Multi-AZ expansion without replacement:

- `NetworkStack` parameters control AZ count
- `StorageStack` RDS `multiAz` flag is a config toggle
- No stack restructuring needed — just config change in `staging.ts` / `production.ts`
