# Stellar Block Explorer - Infrastructure Overview

> This document expands the infrastructure portion of
> [`technical-design-general-overview.md`](../technical-design-general-overview.md).
> It preserves the same hosting, deployment, and operational assumptions, but specifies the
> infrastructure model in more detail so it can later serve as input for implementation task
> planning.

---

## Table of Contents

1. [Purpose and Scope](#1-purpose-and-scope)
2. [Infrastructure Principles](#2-infrastructure-principles)
3. [Target System Topology](#3-target-system-topology)
4. [Deployment Model](#4-deployment-model)
5. [Managed Components](#5-managed-components)
6. [Networking and Security Boundary](#6-networking-and-security-boundary)
7. [Environments and Scalability](#7-environments-and-scalability)
8. [Observability and Operations](#8-observability-and-operations)
9. [Delivery Model and Workspace Boundary](#9-delivery-model-and-workspace-boundary)

---

## 1. Purpose and Scope

Infrastructure is the AWS-hosted runtime foundation of the block explorer. Its role is to
run ingestion, storage, API delivery, static frontend hosting, and operational monitoring in
one deployable system that can be operated without depending on third-party explorer
services.

This document covers the target infrastructure design only. It does not redefine frontend
behavior, backend API contracts, indexing logic, or database schema beyond the parts needed
to explain how the infrastructure is deployed and operated.

This document was originally written as an _intended_ infrastructure model, ahead of a
skeletal repository. That caveat is retired: the system is deployed and serving mainnet
traffic. Per [ADR 0032](../../../lore/2-adrs/0032_docs-architecture-evergreen-maintenance.md)
this file is evergreen — it must describe what runs, and any PR that changes the shape of
the system updates it in the same PR.

If any statement in this file conflicts with
[`technical-design-general-overview.md`](../technical-design-general-overview.md), the main
overview document takes precedence. This file is an infrastructure-focused refinement of
that source, not an independent redesign.

## 2. Infrastructure Principles

The source design implies a small set of infrastructure principles that should remain
stable unless the main document changes first.

### 2.1 Full-Stack Ownership

The block explorer runs on infrastructure owned by the project team in a dedicated AWS
sub-account. Core functionality does not depend on Horizon, Soroswap, Aquarius, Soroban
RPC, or any external explorer API.

The infrastructure is expected to host:

- canonical ledger ingestion
- explorer database storage
- public REST API delivery
- public frontend delivery
- operational visibility and alarms

### 2.2 AWS-Managed Runtime Bias

The current design favors managed AWS services over self-operated long-running platforms.

That shows up as:

- ECS Fargate for the continuously running Galexie process
- AWS Lambda for event-driven processing (Ledger Processor) and API handlers
- Hetzner-hosted ClickHouse for the production data plane (cross-cloud,
  reached over mTLS from AWS — see §5.6)
- API Gateway and CloudFront for public delivery
- Secrets Manager, CloudWatch, and X-Ray for operational concerns
- local `crates/backfill-runner` (production) or `crates/backfill-bench`
  (benchmark) CLI on a developer workstation for historical
  backfill per [ADR 0010](../../../lore/2-adrs/0010_local-backfill-over-fargate.md) —
  **not** a Fargate task; streams from Stellar public archives into the same
  `process_ledger` pipeline, writes directly to the database

This keeps the runtime model operationally narrow and aligned with the serverless/event-
driven shape of the product.

### 2.3 Event-Driven Ingestion Path

Infrastructure is designed around an asynchronous ingestion chain:

1. Galexie streams canonical ledger data from Stellar peers
2. `LedgerCloseMeta` XDR files land in S3
3. S3 object creation rings an SQS doorbell; the Ledger Processor Lambda reconciles
   against a ClickHouse cursor rather than trusting one event per file (task 0241)
4. typed summary columns + appearance-index rows + derived state are written to
   ClickHouse. **There is no per-ledger transaction** — ClickHouse has no cross-table
   ACID. Atomicity is approximated by ordering: the `ledgers` row is flushed **last**,
   after every other insert has ack'd, so it acts as the commit marker and a partial
   write is re-done by the reconcile rather than half-committed
5. list / partition-pruned API reads serve entirely from the explorer's own
   database; heavy-field detail endpoints (E3, E14) additionally fetch raw
   `.xdr.zst` from the public Stellar ledger archive at request time
   ([ADR 0029](../../../lore/2-adrs/0029_abandon-parsed-artifacts-read-time-xdr-fetch.md))

This separation is a core infrastructure assumption, not an implementation detail.

### 2.4 Progressive Reliability

Launch infrastructure is intentionally simpler than a long-term high-availability target.

The plan as originally documented was: start in a single Availability Zone, use Single-AZ
RDS at launch, expand to Multi-AZ when SLA requirements justify it.

**That progression was abandoned, not completed.** Per
[ADR 0047](../../../lore/2-adrs/0047_clickhouse-primary-api-datastore.md) the datastore is
a single self-hosted ClickHouse box at Hetzner, and it was never an RDS instance in
production. The reliability posture is correspondingly different, and the trade was made
knowingly:

- **no Multi-AZ, no managed failover** — one box. Losing it costs a restore, not a
  failover.
- **redundancy is disk-level** — mdadm RAID 1 on the server itself.
- **backup is weekly**, not continuous: Borg → Hetzner BX21, `--keep-weekly 4`. There is
  no PITR. See [`docs/backups.md`](../../backups.md) — after a restore the Lambdas do
  **not** re-deliver the rolled-back range; it must be re-ingested with `backfill-runner`.
- **the scaling lever is vertical first** — a bigger auction server, then a replicated
  second node, not a managed read replica.

## 3. Target System Topology

### 3.1 End-to-End Topology

The infrastructure currently resolves into five runtime zones:

- Stellar network inputs
- ingestion components
- transient object storage
- explorer database storage
- public delivery layer

Logical flow:

```text
Stellar peers / history archives
  -> Galexie on ECS Fargate
  -> S3 ledger object storage
  -> S3 PutObject -> SQS doorbell
  -> Ledger Processor Lambda (outside the VPC)
  -> ClickHouse on Hetzner ch-prod-01, over mTLS via Caddy
  -> API Gateway + Lambda API
  -> CloudFront-served frontend clients
```

### 3.2 Public Traffic Path

Public user traffic should follow a simple path:

- the frontend is served through CloudFront as a static React application
- the frontend calls the public REST API through API Gateway
- public browser traffic is anonymous read-only and does not carry API keys
- API Gateway invokes Lambda-based Rust/axum handlers
- handlers read from ClickHouse only

No public client should connect directly to the database or to ingestion components.

### 3.3 Ingestion Traffic Path

Canonical chain data should follow a separate path:

- Galexie connects to Stellar peers through Captive Core
- the historical backfill task reads Stellar public history archives
- both live and historical ingestion produce `LedgerCloseMeta` XDR files in the same S3
  bucket format
- the same Ledger Processor Lambda handles both paths after S3 delivery

This infrastructure design avoids separate persistence pipelines for live data and backfill.

## 4. Deployment Model

### 4.1 AWS Account Model

All infrastructure runs in a dedicated AWS sub-account owned by Rumble Fish.

This matters because the infrastructure document assumes:

- isolated ownership of runtime resources and IAM boundaries
- infrastructure lifecycle controlled by the project team
- ability to redeploy the full stack without coordinating with an external platform owner

### 4.2 Deployment Topology

Production runs in `eu-central-1` in a single Availability Zone
(`eu-central-1a`). The earlier `us-east-1` footprint was retired by
task 0249; the greenfield redeploy in eu-central-1 (task 0239)
applies the minimal-AWS topology described below from day one — no
incremental migration.

Resources by location:

- **Minimum VPC, public subnet only** — hosts ECS Fargate Galexie. No
  NAT Gateway, no private subnets, no S3 Gateway endpoint. Galexie
  tasks get a per-task public IPv4 (`assignPublicIp: ENABLED`),
  which is the only egress path.
- **Lambdas out-of-VPC** — API, Ledger Processor, type-1 enrichment
  worker. Egress goes via the AWS-managed Lambda pool (no NAT GW cost,
  no IP pinning). (The PG-era migration + partition Lambdas were removed
  in task 0241 — CH applies its schema box-side and auto-partitions.)
- **CloudFront + Route 53** — global resources, region-independent.
- **`us-east-1` retained for two resources only**: the `CDKToolkit`
  bootstrap stack and the CloudFront viewer-side ACM certificate
  (CloudFront only accepts viewer certs from us-east-1 — a hard AWS
  constraint).

The design does not assume active-active regional redundancy or a
multi-region failover plan.

### 4.3 Trust Boundaries

- **Public ingress**: CloudFront (SPA), API Gateway (REST API), Route 53.
- **Application compute**: Lambdas and the ECS Fargate Galexie task
  reach the internet directly (no VPC isolation). Cross-cloud identity
  to the Hetzner-hosted ClickHouse box is asserted by mTLS client
  certificates from AWS Secrets Manager — VPC walls are replaced with
  cryptographic identity (see §5.6).
- **Secrets**: never baked into images or source. Per-service mTLS
  bundles (`{cert, key, ca}`) live in AWS Secrets Manager and are
  fetched by Lambdas via the AWS Parameters and Secrets Lambda
  Extension and by Galexie via native ECS secrets injection.

## 5. Managed Components

### 5.1 Ingestion Components

**Galexie process**

- runs on ECS Fargate as one continuous task for live ingestion
- placed in a public subnet with `assignPublicIp: ENABLED` — per-task
  public IPv4 is the only egress path post-task-0239 (no NAT GW).
  Flipping this flag breaks ECR pull, S3 writes, peer overlay, and
  Hetzner-CH mTLS simultaneously — there is a CODEOWNERS-flagged
  inline comment in `infra/src/lib/stacks/ingestion-stack.ts` to
  catch drive-by reverts
- connects to Stellar network peers via Captive Core
- emits one `LedgerCloseMeta` file per ledger close to S3
- mTLS cert bundle (`{cert, key, ca}` for the `galexie-production` CN)
  is mounted via ECS native Secrets Manager injection — each field
  arrives as a separate container env var, materialised to PEM files
  at startup by the container entrypoint

**Historical backfill task**

- runs as a **local CLI tool** (`crates/backfill-runner` or `crates/backfill-bench`)
  on a developer workstation per [ADR 0010](../../../lore/2-adrs/0010_local-backfill-over-fargate.md)
- reads from Stellar public history archives
- invokes the same `process_ledger` code path used by the Ledger Processor Lambda

### 5.2 Storage Components

**S3 bucket `stellar-ledger-data`**

- receives `LedgerCloseMeta` XDR files
- acts as transient object storage between Galexie and the Ledger Processor Lambda
- triggers the Ledger Processor via S3 object creation events
- is governed by lifecycle retention rules because replay and incident validation depend on
  short-term artifact availability

**Production data plane — Hetzner ClickHouse (post-task-0239)**

- The block explorer's owned relational/columnar data plane lives on
  a Hetzner-hosted ClickHouse box, not in AWS. See §5.6 for the full
  description (Caddy mTLS termination, per-service RBAC, Borg backups).
- There is **no AWS-side database** in the production topology — and there never was.
  Task 0239's RDS "decommission" phase closed vacuously: production stacks were never
  deployed (`validateConfig` blocked on `hostedZoneId: "CHANGE_ME"`), so no production
  RDS existed to remove. Only a _staging_ RDS ever ran, and task 0249 destroyed the whole
  staging footprint on 2026-05-21.

**Local-dev ClickHouse**

- runs as the `clickhouse` service in `docker-compose.yml`
  (`clickhouse/clickhouse-server:26.3`), exposing HTTP `8123` and native `9000`.
  There is **no `postgres` service** — Postgres and sqlx were removed from the codebase
  in task 0244.
- holds the schema in `crates/db-clickhouse/schema/init.sql` (28 tables, 3 materialized
  views, 1 `Dictionary` as of 2026-07-22); applied idempotently by the
  `db-clickhouse-init` sidecar after `clickhouse` reports healthy, and equally by the
  Rust `db-clickhouse-init` CLI when iterating outside Docker
- the ClickHouse _pilot_ framing this section used to carry is spent. ClickHouse is no
  longer a parallel store being evaluated next to RDS — per
  [ADR 0047](../../../lore/2-adrs/0047_clickhouse-primary-api-datastore.md) it is the
  sole production datastore, and the indexer and API both write and read it. The pilot
  ADR ([0044](../../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md)) and
  [task 0204](../../../lore/1-tasks/archive/0204_FEATURE_clickhouse-pilot-crate-docker-schema/README.md)
  are history; [`clickhouse-pilot.md`](../database-schema/clickhouse-pilot.md) should be
  read the same way.

### 5.3 Processing Components

**Lambda — Ledger Processor**

- is triggered by S3 PutObject events on the Galexie-owned bucket
- downloads and parses XDR using the Rust `stellar-xdr` crate via
  `crates/xdr-parser` (per [ADR 0004](../../../lore/2-adrs/0004_rust-only-xdr-parsing.md))
- writes typed columns to the Hetzner-hosted ClickHouse over mTLS,
  authenticated as the `lambda-ingestion-production` CN → `ingestion_writer`
  CH user mapping (see §5.6). Runs OUT-of-VPC; the AWS Parameters
  and Secrets Lambda Extension fetches the cert bundle from Secrets
  Manager at cold start (no SDK call on the hot path).
- the application-layer PG→CH query migration is tracked separately
  (task 0241) — task 0239 only landed the transport (out-of-VPC +
  mTLS wiring + cert distribution)

### 5.4 API and Delivery Components

**Lambda — Rust/axum API handlers**

- serve all public REST endpoints
- read list / partition-pruned endpoints from the Hetzner-hosted
  ClickHouse over mTLS, authenticated as the `lambda-api-production`
  CN → `api_reader` CH user mapping (see §5.6). Run OUT-of-VPC;
  cert bundle delivered via the AWS Parameters and Secrets Lambda
  Extension as for the Ledger Processor.
- additionally fetch `.xdr.zst` from the **public Stellar ledger archive** for
  heavy-field endpoints (E3 `/transactions/:hash`, E14 `/contracts/:id/events`)
  and re-parse with the shared `crates/xdr-parser` at request time, per
  [ADR 0029](../../../lore/2-adrs/0029_abandon-parsed-artifacts-read-time-xdr-fetch.md)
- do not perform chain indexing and do not depend on Horizon, Soroban RPC, or
  third-party indexers for any response

**API Gateway**

- exposes the public REST API
- provides request routing, throttling, request validation, and response caching
- may issue API keys for trusted non-browser consumers, but normal explorer browsing does
  not depend on browser-embedded keys

**Edge protection — Cloudflare on the API, nothing on the frontend**

Per [ADR 0048](../../../lore/2-adrs/0048_cloudflare-edge-over-aws-waf.md) and task
0302, **both AWS WAF WebACLs were dropped** and the constructs, the us-east-1 stack
and the `enableWaf` setting were deleted from the CDK app. There is no AWS WAF in
this system and no switch to re-enable one. What replaced it is asymmetric, and the
asymmetry is deliberate:

- **API** (`api-sorobanscan.rumblefishdev.com`) — fronted by **Cloudflare**: WAF
  managed rules, unmetered DDoS, Managed Challenge, rate limiting. The AWS origin
  accepts Cloudflare only, via a secret request header verified in the application
  (`crates/api/src/common/edge_lock.rs`).
- **Frontend** (`sorobanscan.rumblefish.dev`) — **no edge filtering at all.** The
  zone stays on Route 53; the nameserver flip in task 0277 covered
  `rumblefishdev.com`, not the parent `rumblefish.dev`. This is an accepted end
  state, not a pending migration: the distribution serves a static, edge-cached
  bundle from a private S3 origin via Origin Access Control, so injection-oriented
  managed rules have no application to protect, and putting Cloudflare ahead of
  CloudFront would stack two CDNs. AWS Shield Standard still covers volumetric
  L3/L4; nothing caps HTTP requests per IP.

API Gateway throttling and the usage-plan limits are independent of all of the
above and are now the only volumetric control on the origin. Production defaults
are `50` rps / `100` burst. They are **not** unconditional: setting
`loadTesting: true` raises both to the account ceiling for a coordinated
load-test window (`LOAD_TEST_THROTTLE_RATE` / `_BURST` in
`stacks/api-gateway-stack.ts`), which leaves the public API with no rate
protection at all. `validateConfig` prints a loud banner while it is set; restore
the defaults by setting the flag back to `false` in `envs/production.json` and
redeploying `ApiGateway` as soon as the run ends.

**CloudFront CDN**

- serves the React frontend
- caches static assets and documentation assets; API responses are not assumed to traverse
  CloudFront in the initial topology
- since task 0519, also serves a second, independently-built SPA from its
  own S3 bucket (`${envName}-soroban-explorer-api-spa`) under the `/api/*`
  path on the same distribution. Gated by its own CloudFront Function
  basic-auth flag (`enableApiSpaBasicAuth`), independent of the main site's
  `enableBasicAuth`/`enableOriginSecretLock` — the two share the same
  `basicAuthFunctionCode`/KeyValueStore construct when both are enabled,
  but each behavior's gate can be toggled without affecting the other.

**Swagger UI**

- served directly from the API (utoipa-swagger-ui `/api-docs` endpoint)
- no separate S3 bucket or CloudFront distribution needed

### 5.5 Operational Components

**Secrets Manager**

- stores per-service mTLS client cert bundles (`{cert, key, ca}`) for
  AWS workloads (Lambda, Galexie) authenticating to the Hetzner-hosted
  ClickHouse. Secrets live under `${mtlsSecretNamePrefix}/<cn>`
  (e.g. `soroban/production/mtls/lambda-api-production`).
- stores any other non-browser integration secrets

**CloudWatch + X-Ray**

- provide logs, metrics, dashboards, alarms, and distributed tracing

**GitHub Actions -> AWS CDK**

- provide the infrastructure deployment pipeline
- are the documented mechanism for infrastructure-as-code rollout

### 5.6 Production ClickHouse on Hetzner (task 0216)

The local-development ClickHouse pilot described in §5.2 is graduated
to a production deployment on a Hetzner-hosted dedicated server.
Hetzner hosts the data plane only; the application API remains on AWS.

As part of this migration, the AWS-side topology is restructured:
Lambda functions are moved out of the VPC and the long-running
ingestion task is moved to a public subnet, eliminating the NAT
Gateway. Authentication between AWS-side workloads and the Hetzner-
hosted database is based on cryptographic identity (mutual TLS).

DNS for the Hetzner endpoint is provisioned via AWS Route 53. A
dedicated CDK stack (`HetznerDnsStack` in `infra/src/lib/stacks/`)
creates an A record under the `sorobanscan.rumblefish.dev` hosted
zone that points directly at the dedicated server's public IPv4 —
a literal value, not an AWS alias, because the target is non-AWS.
The record TTL is short (5 minutes) so an IP change after a box
replacement propagates quickly. The same hostname is the target of
the Let's Encrypt HTTP-01 challenge that Caddy on the box uses to
obtain its TLS certificate, so this record must exist before the
Hetzner stack can serve traffic.

High-level decisions are recorded in the
[task 0216 notes](../../../lore/1-tasks/active/0216_RESEARCH_hetzner-clickhouse-deploy/notes/S-decisions.md).

**Per-service identity and RBAC.** Within the Hetzner stack,
ClickHouse exposes per-service users (one per Lambda / Galexie /
dev consumer class) bound to capability-scoped profiles
(`read_only`, `write_no_ddl`, `migration_full`, …) and quotas
(task 0240). Caddy verifies the mTLS client cert and maps the
verified CN to the ClickHouse user it forwards as
`X-ClickHouse-User`; the cert is the credential, the CH user is
`<no_password/>` restricted to the compose bridge subnet. The
host-side admin user (`default`, used by the init sidecar and
backup script) keeps its password and is reachable only from
loopback. See
[`docs/architecture/security/clickhouse-rbac.md`](../security/clickhouse-rbac.md)
for the per-service user matrix, the CN→user mapping mechanism,
and known limitations (notably quota enforcement gap on the
proxy-trust path).

**Cert distribution AWS → Hetzner (task 0239).** Each AWS-side
workload that talks to Hetzner CH has a dedicated mTLS bundle
(`{cert, key, ca}` JSON) in AWS Secrets Manager under
`${mtlsSecretNamePrefix}/<cn>`. CN naming follows
`<service>-<environment>` (e.g. `lambda-api-production`,
`galexie-production`). Secrets are issued by
`infra-hetzner/ca/issue-client-cert.sh` on the operator laptop
(Linux-only, CA key sourced from the team password manager onto
`/dev/shm`), then uploaded via `aws secretsmanager put-secret-value`
and registered on the box by appending the CN to
`CLICKHOUSE_CN_USER_MAP` in `~/.config/soroban-prod.env` and replaying
`ansible-playbook --tags caddy_reload`. Lambdas read their bundle via
the AWS-managed "Parameters and Secrets Lambda Extension" layer
(in-memory cache at `http://localhost:2773`); Galexie reads it via
native ECS Secrets Manager injection (per-field env vars
materialised to PEM files at container startup).

**Relationship to the AWS sections above:** the AWS topology described
in §§3–5.5 represents the original infrastructure design and is
preserved verbatim. Post-CH-migration, the AWS-hosted database is
decommissioned, Lambdas exit their VPC, the ingestion task moves to a
public subnet, and the NAT Gateway is removed. A separate, future ADR
records this architectural realignment in which the Hetzner-hosted
ClickHouse becomes the production data plane.

**Region change.** As part of the same realignment, the AWS-side
production deployment moves from `us-east-1` to `eu-central-1`
(task 0239). Task 0249 destroys the entire `us-east-1` footprint
(staging in full + `Explorer-Cicd`; no production stacks ever
deployed there) so the new region starts greenfield, avoiding
cross-region cost overlap on NAT Gateway and RDS. Two AWS resources
must remain in `us-east-1` regardless of the production region:
the `CDKToolkit` bootstrap stack and the ACM certificate that backs
the CloudFront viewer-side TLS (a hard CloudFront requirement —
viewer certificates must live in `us-east-1`). Route 53 hosted
zones are global and are unaffected by the region change; only the
records inside them are recreated as the new stacks come up in
`eu-central-1`.

## 6. Networking and Security Boundary

### 6.1 Network Shape

Post-task-0239 the AWS-side runtime is intentionally stateless and
minimal. Network shape:

- **CloudFront, API Gateway** — public ingress layer (CloudFront viewer
  cert in `us-east-1`, API Gateway regional cert in `eu-central-1`).
  The AWS WAF layer that used to sit here is gone; edge protection is
  Cloudflare on the API only, with the origin locked to it —
  [ADR 0048](../../../lore/2-adrs/0048_cloudflare-edge-over-aws-waf.md),
  tasks 0277 and 0302.
- **Application Lambdas (API, Ledger Processor, type-1 enrichment
  worker)** — run OUTSIDE the VPC. Egress via AWS-managed Lambda pool.
  Identity to Hetzner CH is asserted by mTLS (no IP pinning, no VPC
  walls).
- **ECS Fargate Galexie** — public subnet, per-task public IPv4
  (`assignPublicIp: ENABLED`). Reaches the Stellar peer overlay,
  the ledger-data S3 bucket, and Hetzner CH directly via the
  Internet Gateway.

The data plane is intentionally out of AWS — Hetzner-hosted
ClickHouse is the production source of truth (see §5.6). VPC
isolation no longer protects "the database" because there is no
AWS-side database.

### 6.2 Secret Handling

Credential handling is centralised in AWS Secrets Manager.

Stored material:

- per-service mTLS client cert bundles (`{cert, key, ca}`) for AWS
  workloads authenticating to the Hetzner-hosted ClickHouse (see §5.6)
- any other non-browser integration secrets or keys

Browser-delivered frontend bundles do not contain API keys or other shared secrets.

Production transport and storage hardening baselines:

- CloudFront and API Gateway serve public traffic over HTTPS/TLS
- production S3 buckets use server-side encryption at rest; the
  `stellar-ledger-data` ledger bucket uses SSE-S3 (AES256) — its contents are
  public on-chain XDR, so KMS would only add per-object request cost on the
  high-volume ingest path (one Put per ledger + one Get per processor run)
  without buying confidentiality (task 0278)
- AWS → Hetzner ClickHouse connections require mTLS (cert-pinned at
  the CA from `infra-hetzner/ca/`); Caddy on the box terminates TLS
  and enforces the CN allowlist before forwarding to ClickHouse over
  the compose bridge subnet
- ClickHouse-on-Hetzner storage encryption is handled at the host
  level (per task 0216 — see Hetzner stack docs); AWS-side storage
  hardening no longer covers the analytics data plane

The architecture does not imply storing runtime secrets in source control, Lambda code, or
container images.

### 6.3 Public Exposure Rules

The infrastructure should keep public exposure narrow.

Publicly exposed surfaces are:

- CloudFront-hosted frontend delivery
- API Gateway-hosted REST API
- public DNS routing via Route 53
- API documentation served from utoipa-swagger-ui `/api-docs` endpoint

Protection on those surfaces is API Gateway throttling plus, on the API only, the
Cloudflare edge with the origin locked to it via a secret request header. API keys,
if issued, are for trusted automation or partner use cases and are never required
by the browser application; partner `x-api-key` callers egress through the proxied
hostname. `ch.sorobanscan` stays DNS-only (mTLS + ACME).

> **AWS WAF is not part of this picture** — both WebACLs were dropped
> ([ADR 0048](../../../lore/2-adrs/0048_cloudflare-edge-over-aws-waf.md), task
> 0302). The frontend distribution has no edge filtering; see § 5.4 for why that is
> the accepted end state rather than an open gap.

Non-public components should remain directly unreachable to external users.

### 6.4 External Dependency Boundary

External runtime dependencies are limited to read-only canonical Stellar data sources:

- Stellar network peers — live data feed for Galexie (ingest-time)
- Stellar public history archives — one-time backfill feed for the local
  `backfill-runner` or `backfill-bench` CLI run from a developer workstation per
  [ADR 0010](../../../lore/2-adrs/0010_local-backfill-over-fargate.md)
  (no Fargate task in production topology)
- Stellar public ledger archive — read-time XDR fetch for E3 / E14 at the API
  layer per [ADR 0029](../../../lore/2-adrs/0029_abandon-parsed-artifacts-read-time-xdr-fetch.md)

No other external API is required. Horizon, Soroban RPC, and third-party
indexers are explicitly not in the trust boundary.

## 7. Environments and Scalability

### 7.1 Environment Model

Current environments (post-task-0249):

- **Development** — local ClickHouse via `docker-compose.yml` for developer and CI
  workflows. Postgres is gone: no `postgres` service in the compose file, and sqlx was
  removed from the codebase in task 0244.
- **Production** — mainnet data; AWS workloads in `eu-central-1`
  (Lambdas out-of-VPC, Galexie public-subnet ECS Fargate) reaching
  the Hetzner-hosted ClickHouse data plane over mTLS.

AWS-side staging was retired by task 0249 and is not redeployed in
`eu-central-1`. Pre-production validation now happens in the dev
environment (local CH/PG) and via canary / smoke runs against
production on cert-restricted endpoints; if product re-opens the
need for a staging tier later, it would be reintroduced as a
separate task.

### 7.2 Scaling Model

The source design documents scaling by component rather than a single platform-wide
mechanism.

Current expectations:

- API Lambda scales on demand, up to the documented concurrency tier
- Ledger Processor Lambda scales per S3-triggered ledger file
- CloudFront scales automatically
- ClickHouse scaling is vertical for the foreseeable future
  (single-box Hetzner deploy per task 0216); replication / sharding
  decisions are deferred to a future ADR if/when query load demands it

### 7.3 Availability Progression

High availability is staged rather than assumed at launch.

Documented progression:

- AWS side: single AZ in `eu-central-1a` for the Galexie public subnet.
  Lambdas are AZ-agnostic (out-of-VPC, AWS-managed pool).
- Data plane: single Hetzner box on RAID 1 + Borg backups to a
  Storage Box; replication / multi-region HA deferred until SLA
  requires it.

This is important because the infrastructure doc should not imply higher availability than
what the source design currently commits to.

### 7.4 Production Limits and Protections

Post-task-0249 there is only a production AWS environment. Profile:

- public-internet availability with API throttling sized for anonymous
  browser traffic, and Cloudflare edge protection on the API hostname
- response caching and Lambda concurrency tuned for real public demand
- longer operational retention for logs, traces, and replay-relevant
  artifacts; production replay artifacts in `stellar-ledger-data`
  should be kept for at least 30 days
- SSE-S3 (AES256) encryption for the `stellar-ledger-data` S3 bucket
  (public XDR — no KMS, to avoid per-object KMS request cost, task 0278);
  KMS-backed encryption for ECR (Galexie images); CloudFront + API
  Gateway serve over HTTPS/TLS; AWS → Hetzner CH connections enforce
  mTLS at the Caddy layer on the box
- ClickHouse-on-Hetzner: Borg-encrypted backups to BX21 Storage Box,
  RAID 1 on the box, password rotation policy in
  `infra-hetzner/README.md`
- full paging and SLA-oriented alert thresholds

Exact values live in `infra/envs/production.json`.

## 8. Observability and Operations

### 8.1 Monitoring Surface

The infrastructure design already defines a monitoring baseline.

CloudWatch dashboards should expose at least:

- Galexie S3 file freshness
- Ledger Processor duration
- Ledger Processor error rate
- API latency across p50/p95/p99
- highest indexed ledger sequence versus network tip

ClickHouse server-side metrics (CPU, memory, query latency, merge
backpressure) live on the Hetzner box itself via CH's native
Prometheus endpoint (`127.0.0.1:9363`) and `system.metric_log` —
they are NOT mirrored to CloudWatch. Pull-side scraping is deferred
to a follow-up monitoring task per task 0216 future work.

### 8.2 Alerting Surface

**One engine, one path, five rules.** Every AWS-side signal alarms through
CloudWatch and reaches a human through SNS -> Chatbot -> Slack; no component
notifies anyone directly, and no second notifier is added for an AWS-side
signal. The rules below govern every alarm in the list, including ones added
later — see [ADR 0054](../../../lore/2-adrs/0054_one-alarm-engine-and-three-rules-for-alarms.md)
for why each was adopted and what it cost:

1. **CloudWatch is the engine, SNS -> Chatbot -> Slack is the path.**
   Components emit metrics; they do not notify. Thresholds, suppression and
   routing are declared together so they can be reviewed as a set. Every
   alarm's description is written for the person it wakes: what is happening,
   and where the runbook is.
2. **Alarm on change, not on level**, wherever the condition can persist —
   CloudWatch pages on transition, so a latched level alarm is silent from its
   second minute. One carve-out: a level alarm is correct where policy forces
   the steady state to zero (the DLQs, the 5xx count), because it can be
   drained and re-armed.
3. **Absence is `breaching` only where nothing else witnesses the same thing.**
   One alarm in the set uses it — Galexie ingestion lag, where "no ledgers
   landed" has no other witness.
4. **A page caused by planned work the operator just performed is cheap.**
   Pauses are not machine-readable; one knowing page per pause is the accepted
   design rather than a suppression mechanism.
5. **The delivery path is verified on every change to it, before the change is
   called done** — a deploy of the alarm stack is not finished until one
   message has travelled the whole chain and been seen in the channel. This
   rule exists because a topic-policy change once revoked CloudWatch's right
   to publish and every alarm went mute for nine days while evaluating and
   transitioning perfectly. Reading the policy declared it fixed twice; only
   sending something found it.

The deployed alarms (production; authoritative definitions in
`infra/src/lib/stacks/cloudwatch-stack.ts`):

- Galexie ingestion lag — zero doorbells sent to the ingest queue in 5 min
  (SQS `NumberOfMessagesSent`, one per S3 ledger object; the queue metric is
  the alarm signal, S3 listing is only a diagnostic cross-check), missing data
  treated as breaching
- Galexie ephemeral storage above 60% sustained 3×5 min
- Ingest backlog age above 120 s for 3 consecutive minutes (set by
  `ingestionBacklogAgeSeconds` in `infra/envs/production.json`) — the consumer-side
  counterpart to the lag alarm (a planned indexer pause pages once, knowingly)
- Ledger Processor error rate above 1% of Lambda invocations
- Indexer ClickHouse write failures — **any** post-retry hard-failure log line
  in 5 min (zero tolerance; a matching line means the whole in-band retry
  envelope was exhausted)
- Ledger Processor DLQ depth above 0
- Type-1 enrichment DLQ depth above 0
- Enrichment worker error rate above 1% of Lambda invocations
- API Gateway 5xx — **any single** 5xx in a 5-minute window (zero tolerance;
  the response to a page is to fix the class, not to raise a threshold — see
  [`docs/runbooks/api-5xx.md`](../../runbooks/api-5xx.md))
- Origin-lock canary (flag-gated, off until the Cloudflare cutover) — a direct
  origin answering instead of 403

These values are the production baseline. ClickHouse-side alerts
(query backpressure, partition merge stalls, disk usage) live on
the Hetzner box, separate from CloudWatch — see `infra-hetzner/`.

### 8.3 Recovery Assumptions

The source design documents specific operational recovery assumptions:

- Galexie is checkpoint-aware and resumes from the last exported ledger on restart
- Lambda retries S3-triggered processing automatically
- failed ledger files remain in S3 and can be replayed by re-triggering the Lambda
- schema migrations run before new Lambda code deployment in the CI/CD pipeline
- protocol upgrades are handled by bumping the pinned `stellar-xdr` Rust crate
  (per [ADR 0004](../../../lore/2-adrs/0004_rust-only-xdr-parsing.md)); the frontend consumes
  typed API responses via OpenAPI-generated TS client (task 0096).

These assumptions connect runtime infrastructure directly to safe ingestion operations.

## 9. Delivery Model and Workspace Boundary

### 9.1 Infrastructure as Code Boundary

The documented infrastructure direction is AWS CDK written in TypeScript.

Within the current workspace structure, that boundary maps to:

- `infra` for infrastructure definitions
- application packages under `crates/*` and `web` as runtime artifacts deployed by the infrastructure

The infrastructure doc should therefore be read as the target design input for the future
CDK stack, not as a claim that the full stack already exists in the repository.

### 9.2 CI/CD Model

The source design defined the delivery path as GitHub Actions → `cdk deploy`, with
environment parity across staging and production.

**That is not what happens.** Read
[`docs/deployment.md`](../../deployment.md) before acting on this section:

- there is **no staging environment**, so there is no parity to maintain
- there is **no CI-driven deploy** — `cdk deploy` is run **manually from an operator
  laptop**, and the `staging` CI path is dead (`.github/workflows/deploy-staging.yml`
  references `bin/staging.js` and `envs/staging.json`, neither of which exists;
  removal is pending in PR #338)

GitHub Actions still runs build, test and lint. It does not deploy. Treat infrastructure
rollout as a manual operations process until a production deploy workflow lands
(task 0103).

### 9.3 Public-Repo Configuration Model

Because the repository is public, infrastructure configuration must be split between
non-secret config committed to git and secrets resolved outside the repository.

Safe-to-commit infrastructure config includes:

- environment names, AWS region, and account/stack identifiers
- instance classes, cache sizes, retention periods, and scaling thresholds
- public domain names and routing structure
- feature flags and non-sensitive deployment toggles
- secret references such as parameter names, secret names, or ARNs, but not secret values

The repository should not contain:

- database passwords, staging web passwords, API keys, webhook secrets, or private keys
- `.env.prod`, `.env.staging`, or similar files containing real secret values
- copied secret payloads inside CDK context files, TypeScript constants, or GitHub workflow
  YAML

Expected secure configuration model:

- non-secret environment config lives in `infra/config/*`
- real secret values live in AWS Secrets Manager or SSM Parameter Store SecureString
- CDK consumes secret references, not hard-coded secret values
- runtime workloads (Lambda, ECS) read only the specific secrets they need through IAM
- the mTLS client certificate and key that AWS presents to Caddy live in Secrets Manager
  and are read at Lambda cold start through the Secrets Manager extension

### 9.4 CI/CD Credentials and Deployment Access

For a public repository, CI/CD authentication should avoid long-lived AWS credentials stored
in GitHub secrets.

Preferred model:

- GitHub Actions uses OIDC to assume AWS roles at deploy time
- staging and production use separate AWS roles and separate environment protections
- IAM permissions remain least-privilege and environment-scoped
- deployment workflows may know which secret to reference, but not embed the secret value

This keeps the public repository redeployable without turning the repository itself into a
secret distribution channel.

### 9.5 Open-Source Redeployability

The main design explicitly assumes the full stack can be redeployed by third parties.

That means the infrastructure design should remain:

- self-contained
- AWS-account reproducible
- free of hidden dependency on internal-only external services for core runtime behavior

### 9.6 Current Workspace State

The repository currently documents the intended infrastructure shape and reserves
`infra` as the infrastructure boundary, but does not yet contain the final deployed
runtime implementation.

That is expected. This document should serve as the detailed reference for future
infrastructure implementation planning, while
[`technical-design-general-overview.md`](../technical-design-general-overview.md) remains
the primary source of truth.
