---
id: '0035'
title: 'CDK: CloudFront, WAF, Route 53, S3 static hosting'
type: FEATURE
status: active
related_adr: ['0005']
related_tasks: ['0006', '0092', '0097']
tags: [priority-medium, effort-medium, layer-infra]
milestone: 1
links:
  - docs/architecture/infrastructure/infrastructure-overview.md
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-03-31
    status: backlog
    who: stkrolikiewicz
    note: 'Updated per ADR 0005: NestJS /api-docs → utoipa-swagger-ui'
  - date: 2026-04-03
    status: active
    who: stkrolikiewicz
    note: >
      Activated. Phase 1: CloudFront + WAF + S3 without custom domain.
      Route 53 + ACM deferred until hosted zone is set up.
---

# CDK: CloudFront, WAF, Route 53, S3 static hosting

## Summary

Define the public delivery layer using CDK: CloudFront distribution for the React SPA, WAF WebACL with managed rules and abuse controls, Route 53 hosted zone with DNS aliases, and ACM TLS certificates. API Gateway traffic does NOT route through CloudFront. CloudFront is for static content only. Swagger UI is served directly from the API (utoipa-swagger-ui `/api-docs` endpoint). Staging uses password protection via CloudFront Functions.

## Status: Backlog

**Current state:** Not started. No dependencies on other infrastructure tasks for the delivery layer definition, though WAF is attached to API Gateway in task 0097.

## Context

The block explorer serves the React SPA frontend through CloudFront with its own Route 53 alias. API Gateway handles API traffic directly and does NOT route through CloudFront — CloudFront is for static content delivery only.

WAF provides browser-facing protection without requiring API keys or secrets in the SPA bundle. The same WebACL is attached to the CloudFront distribution and to API Gateway.

### Source Code Location

- `infra/aws-cdk/lib/delivery/`

## Implementation Plan

### Step 1: CloudFront Distribution - React SPA

Define a CloudFront distribution for the React frontend:

- Origin: S3 bucket hosting the built React SPA (OAI or OAC for private bucket access)
- Default root object: `index.html`
- Error pages: 403 and 404 redirect to `index.html` with 200 status (SPA client-side routing fallback)
- Cache behavior: long TTL for static assets (JS, CSS, images with content hash), short TTL for `index.html`
- HTTPS only: redirect HTTP to HTTPS
- Price class: appropriate for target audience geography
- WAF WebACL: attached (defined in Step 3)

### Step 2: Route 53 Configuration

Define DNS routing:

- Hosted zone for the project domain
- A record (alias) pointing to the SPA CloudFront distribution (e.g., `explorer.example.com`)
- A record (alias) pointing to API Gateway (e.g., `api.example.com`)
- AAAA records (IPv6 aliases) for both

### Step 3: WAF WebACL

Define a WAF WebACL with:

- AWS Managed Rules: Common Rule Set, Known Bad Inputs, IP Reputation List
- Rate-based rule for abuse control (e.g., limit requests per IP per 5-minute window)
- Geo-restriction if needed (optional)
- Logging to CloudWatch Logs for visibility

**WAF attachment points:**

- SPA CloudFront distribution (attached here)
- API Gateway (attached in task 0097)

### Step 4: ACM TLS Certificates

Provision TLS certificates:

- CloudFront certificate: must be in us-east-1 (CloudFront requirement). Covers SPA domain.
- API Gateway certificate: in the stack's deployment region. Covers the API domain.
- Validation: DNS validation via Route 53 (automated by CDK)
- Auto-renewal: managed by ACM

### Step 5: Staging Password Protection

For the staging environment:

- Implement basic auth via CloudFront Functions or Lambda@Edge
- Protect the SPA CloudFront distribution with username/password
- Credentials stored in environment configuration (not hard-coded)
- Production distributions have no password protection

## Acceptance Criteria

- [x] SPA CloudFront distribution is defined with S3 origin and index.html fallback for client routes
- [x] API Gateway traffic does NOT route through CloudFront
- [x] WAF WebACLs are defined with managed rules, IP reputation, and rate-based abuse controls (one per scope — see Deviation)
- [x] WAF is attached to CloudFront distribution (CLOUDFRONT scope) and to API Gateway (REGIONAL scope, defined in `ApiGatewayStack`)
- [x] Route 53 hosted zone has A/AAAA aliases for frontend (DeliveryStack) and API domains (ApiGatewayStack)
- [x] ACM certificate consumed for CloudFront and API Gateway (single us-east-1 cert; see Deviation)
- [ ] DNS validation is automated via Route 53 — N/A, certificate is imported by ARN, not provisioned by CDK
- [x] Staging: CloudFront password protection is implemented via CloudFront Functions backed by KeyValueStore
- [x] Production: no password protection on CloudFront distribution
- [x] HTTP to HTTPS redirect is enabled
- [x] Staging web credentials stored outside repository in CloudFront KeyValueStore (see Deviation — chosen over Secrets Manager/SSM SecureString)
- [x] SPA build pipeline does not embed API keys, secrets, or credentials into the frontend bundle (no SPA build wired in this task)

## Deviations from Original Spec

### 1. Two WAF WebACLs instead of one shared

**Original AC**: "WAF is attached to CloudFront distribution and made available for API Gateway"

**Implemented**: Two distinct WebACLs with identical rule sets — one in `DeliveryStack` (`scope: CLOUDFRONT`) attached to the CloudFront distribution, one in `ApiGatewayStack` (`scope: REGIONAL`) attached to the API Gateway stage.

**Why**: AWS WAF design forbids attaching a `CLOUDFRONT`-scoped WebACL to a regional resource (API Gateway, ALB, AppSync) and vice versa. The "shared WebACL" assumption in the original AC is technically impossible. Two ACLs are the only valid configuration. Both ACLs carry the same managed rule groups (Common, KnownBadInputs, IpReputation) and a 2000-req/5min IP rate limit, plus dedicated CloudWatch log groups with resource policies for `delivery.logs.amazonaws.com`.

### 2. Staging credentials in CloudFront KeyValueStore, not Secrets Manager / SSM SecureString

**Original AC**: "Staging web password stored in AWS Secrets Manager or SSM Parameter Store SecureString"

**Implemented**: CloudFront KeyValueStore (`cloudfront.KeyValueStore` construct), one key `auth-token` containing pre-encoded `base64(user:password)`. The CloudFront Function reads it per-request via `cloudfront.kvs().get('auth-token')`.

**Why**: CloudFront Functions cannot make network calls — they cannot read from Secrets Manager, SSM, or any AWS API in runtime. Both Secrets Manager and SSM SecureString require either:

- (a) `valueFromLookup` at synth time → value gets baked into CFN template / Function code in plain text, defeating "stored in SecureString" intent
- (b) Lambda@Edge instead of CloudFront Function → cold start penalty, replication delay, blocked distribution deletion, ~$0.60/M requests for staging gating — overkill

CloudFront KeyValueStore is the AWS-native pattern for runtime configuration in CloudFront Functions: credentials live in a dedicated IAM-gated store, never enter git, never enter the Function code or CFN template, and rotate in seconds via `aws cloudfront-keyvaluestore put-key` without redeploy. Threat model for staging gating (shared static password protecting non-sensitive UI from indexing/casual access) does not justify Lambda@Edge complexity.

**Bootstrap requirement**: KVS is empty after first deploy. Until populated, the Function returns `503 Service Unavailable` (closed-by-default — safer than open). Populate once after first deploy:

```bash
KVS_ARN=$(aws cloudfront list-key-value-stores \
  --query "KeyValueStoreList.Items[?Name=='staging-soroban-explorer-basic-auth'].ARN" \
  --output text)
ETAG=$(aws cloudfront-keyvaluestore describe-key-value-store \
  --kvs-arn "$KVS_ARN" --query "ETag" --output text)
TOKEN=$(printf 'staging:<password>' | base64)
aws cloudfront-keyvaluestore put-key \
  --kvs-arn "$KVS_ARN" --key auth-token --value "$TOKEN" --if-match "$ETAG"
```

### 3. Single ACM certificate covers both CloudFront and API Gateway

**Original AC**: "ACM certificates are provisioned: us-east-1 for CloudFront, stack region for API Gateway"

**Implemented**: One certificate ARN in `EnvironmentConfig.certificateArn`, consumed by both `DeliveryStack` (CloudFront) and `ApiGatewayStack` (API Gateway custom domain).

**Why**: Stack region is `us-east-1` (per `envs/staging.json`), which happens to be the required region for CloudFront certs. A single wildcard cert in us-east-1 satisfies both. If the project ever moves to another region, `EnvironmentConfig` will need separate `cloudfrontCertificateArn` and `apiCertificateArn` fields, and the API cert must be re-issued in the new stack region.

### 4. Certificate is imported, not provisioned

**Original AC**: "DNS validation is automated via Route 53"

**Implemented**: Certificate already exists in ACM (managed outside CDK), imported by ARN via `acm.Certificate.fromCertificateArn`. No CDK-managed validation.

**Why**: The certificate is a pre-existing wildcard for `*.sorobanscan.rumblefish.dev`, shared across environments and managed manually. Provisioning via CDK would compete for the same DNS validation records and create lifecycle coupling between this stack and certificate renewal. Outside scope of 0035.

### 5. Relationship to research task 0006 / PR #32

PR #32 corrected research 0006 to recommend per-service stack decomposition (`ApiStack`, `IndexerStack`, `IngestionStack`, `FrontendStack`) and elimination of `DeliveryStack`. This task implements the **monolithic `DeliveryStack` + separate `ApiGatewayStack`** pattern instead, consistent with the architecture already established by tasks 0099/0033/0097 (monolithic `ComputeStack`). Per-service refactor is deferred — would require touching 4 archived tasks and ~12 backlog tasks. Tracked as future work, not in scope of 0035.

## Notes

- The SPA CloudFront distribution must handle client-side routing by returning index.html for all paths that do not match a static file. This is achieved through custom error responses (403/404 -> index.html with 200).
- WAF rules should be tuned after initial deployment based on observed traffic patterns. Start with AWS managed rules and adjust.
- CloudFront invalidation will be needed on each SPA deployment. This can be triggered in the CI/CD pipeline (task 0039).
- The staging password protection pattern (CloudFront Functions basic auth) is lightweight and does not require Lambda@Edge if the logic is simple enough.
- All domain names and hosted zone IDs must be parameterized for redeployability across different AWS accounts and domains.
- Swagger UI is served from the API directly (utoipa-swagger-ui `/api-docs` endpoint) — no separate CloudFront distribution or S3 bucket needed for API docs.
