# Deployment Guide

Single entry point for shipping changes to production. **"I changed X →
run Y → verify Z."**

Deep-dives live in the per-layer READMEs (linked below). This file does
**not** duplicate them — it ties them together and is the source of truth
for _which_ command ships _what_.

> **Important — there is no staging environment and no CI-driven deploy.**
> Production is the only environment, and every deploy is run **manually
> from an operator laptop**. See [§ No staging, no CI](#no-staging-no-ci)
> before trusting any `staging` command you find elsewhere in the repo.

---

## TL;DR — what do I run?

| I changed…                                                                  | Run                                                                    | Plane      |
| --------------------------------------------------------------------------- | ---------------------------------------------------------------------- | ---------- |
| API handler (`crates/api`)                                                  | `make -C infra deploy-production-compute`                              | AWS CDK    |
| Indexer / Ledger Processor (`crates/indexer`, `xdr-parser`)                 | `make -C infra deploy-production-compute`                              | AWS CDK    |
| Enrichment worker (`crates/…enrichment…`)                                   | `make -C infra deploy-production-compute`                              | AWS CDK    |
| Frontend SPA (`web/`) — **content**                                         | `make -C infra deploy-production-web`                                  | AWS CDK    |
| Galexie version bump                                                        | edit `galexieImageTag` + mirror to ECR + `deploy-production-ingestion` | AWS CDK    |
| API Gateway (cache / throttle / mTLS lock)                                  | `make -C infra deploy-production-apigateway`                           | AWS CDK    |
| CloudWatch alarms / dashboards                                              | `make -C infra deploy-production-cloudwatch`                           | AWS CDK    |
| CloudFront / SPA delivery **infra** (not content)                           | `make -C infra deploy-production-delivery`                             | AWS CDK    |
| Route 53 record for the CH box (IP change)                                  | `make -C infra deploy-production-hetzner-dns`                          | AWS CDK    |
| Anything else in `infra/src` (multiple stacks)                              | `make -C infra deploy-production` (all) — but see gotchas              | AWS CDK    |
| ClickHouse box config / `docker-compose.prod.yml` / Caddy / users / backups | Ansible — see [§ The machine](#the-machine-hetzner-clickhouse)         | Hetzner    |
| Cloudflare edge (DNS / WAF / mTLS)                                          | Terraform — see [§ Cloudflare edge](#cloudflare-edge)                  | Cloudflare |

Always preview first: `make -C infra diff-production`.

---

## The three deploy planes

Production is deployed across **three independent planes**, all driven
from a laptop. Nothing is wired to CI.

1. **AWS (CDK)** — the bulk: Lambdas (API / indexer / enrichment), Galexie
   (ECS), CloudFront + SPA, API Gateway, alarms, DNS. Driven by
   `make -C infra deploy-production-*`. This document + [`infra/README.md`](../infra/README.md).
2. **Hetzner ClickHouse box (Ansible)** — the production data plane (CH +
   Caddy mTLS + backups). Driven by `ansible-playbook`. Deep-dive:
   [`infra-hetzner/README.md`](../infra-hetzner/README.md).
3. **Cloudflare edge (Terraform)** — DNS record + WAF + API origin mTLS
   lock for the API host. Deep-dive: [`infra/cloudflare/README.md`](../infra/cloudflare/README.md).

They are decoupled: a backend fix touches only plane 1; a CH schema/config
change touches only plane 2; they are shipped separately.

---

## No staging, no CI

AWS-side staging was **retired by task 0249** — production is the only AWS
environment (`eu-central-1`). The following are **dead** and must not be
used or resurrected without first re-introducing a staging env:

- `.github/workflows/deploy-staging.yml` — references `bin/staging.js` and
  `envs/staging.json`, which **no longer exist**. Last successful run:
  April 2026. It will fail.
- `scripts/staging-deploy.sh` and the `staging-*` git-tag trigger (ADR 0009).
- `npm run infra:{deploy,diff,synth}:staging` and any `make deploy-staging*`
  target — **no such Make targets exist**; they error immediately.

If you find a `staging` command in an old README or your shell history,
it is stale. The real path is `make -C infra deploy-production-*`, below.

---

## Operator prerequisites (one-time laptop setup)

**AWS plane:**

- AWS CLI with a named profile that has deploy rights:
  ```bash
  aws configure --profile soroban-explorer
  export AWS_PROFILE=soroban-explorer
  ```
  Region is read from `infra/envs/production.json → awsRegion`
  (`eu-central-1`); you do not pass `--region`.
- Node `22.22.0` (`.nvmrc`) + `npm ci` at the repo root.
- **Rust toolchain + `cargo-lambda` + `zig`** — the API / indexer /
  enrichment Lambdas are Rust, cross-compiled at CDK synth time. Without
  these, `deploy-production-compute` fails at build:
  ```bash
  rustup toolchain install stable
  pip install cargo-lambda        # or: brew install cargo-lambda
  ```
- **First time per AWS account** (not per deploy):
  ```bash
  make -C infra bootstrap          # CDK bootstrap (account + region)
  make -C infra deploy-cicd        # shared OIDC / CICD roles (envs/cicd.json)
  ```

**Hetzner plane:** `ansible-core`, `openssl`, `hcloud` (pip), plus the
password-manager entries and your mTLS cert — full list in
[`infra-hetzner/README.md` § Prerequisites](../infra-hetzner/README.md#prerequisites).

**Cloudflare plane:** Terraform + a zone-scoped `CLOUDFLARE_API_TOKEN` —
see [`infra/cloudflare/README.md`](../infra/cloudflare/README.md).

---

## AWS CDK — stacks and commands

The production app (`node dist/bin/production.js`, config
`infra/envs/production.json`) deploys these stacks. All commands run from
the repo root as `make -C infra <target>` (or `cd infra && make <target>`).

| Stack (`Explorer-production-…`) | Contains                                                             | Make target                              |
| ------------------------------- | -------------------------------------------------------------------- | ---------------------------------------- |
| `Network`                       | Minimal VPC, public subnet (post-0239, no NAT/RDS)                   | `deploy-production-network`              |
| `LedgerBucket`                  | S3 `stellar-ledger-data` (Galexie → indexer)                         | `deploy-production-ledger-bucket`        |
| `Compute`                       | **API + Indexer + Enrichment Lambdas** + SQS queues/DLQ              | `deploy-production-compute`              |
| `Ingestion`                     | Galexie ECS Fargate task + ECR repo                                  | `deploy-production-ingestion`            |
| `Delivery`                      | CloudFront distribution + SPA S3 bucket                              | `deploy-production-delivery`             |
| `ApiGateway`                    | REST API Gateway, caching, throttle, mTLS/edge lock                  | `deploy-production-apigateway`           |
| `Observability`                 | X-Ray / logging config                                               | `deploy-production-observability`        |
| `CloudWatch`                    | Alarms + dashboards                                                  | `deploy-production-cloudwatch`           |
| `HetznerDns`                    | Route 53 A record → CH box IP (from SSM `/soroban/production/ch-ip`) | `deploy-production-hetzner-dns`          |
| `CloudflareBootstrap`           | TF-state bucket for `infra/cloudflare/`                              | `deploy-production-cloudflare-bootstrap` |

Frontend **content** is separate: `deploy-production-web`
(build → S3 sync → CloudFront invalidation).

### Gotchas — read before you deploy

- **`make deploy-production` deploys `--all`.** It will push every pending
  change across _every_ stack. For a single change, prefer the per-stack
  target.
- **Per-stack `make` targets also deploy that stack's dependencies**
  (CDK default). If a dependency stack has an unrelated pending change, it
  ships too. To deploy **exactly one stack** and nothing else, run raw with
  `--exclusively`:
  ```bash
  make -C infra build     # compile CDK first
  cd infra && npx cdk --app "node dist/bin/production.js" \
      deploy Explorer-production-CloudWatch --exclusively --require-approval broadening
  ```
  (Real lesson: shipping a CloudWatch-only alarm change without
  `--exclusively` dragged in a half-finished Compute lambda change.)
- **CDK does not delete a stack you removed from the app.** Deleting a stack's
  code makes it vanish from `cdk ls` and from `deploy --all`, while the real stack
  keeps existing in CloudFormation and keeps billing. Worse, `cdk destroy <name>`
  resolves names from the **synthesized app**, so once the code is gone CDK reports
  no matching stack and cannot delete it either. The only route is raw
  CloudFormation, and if the stack exported anything, in this order:

  1. deploy the consumer stack(s) first, so nothing references the export any more;
  2. confirm the export parameter is released — a cross-region export writer
     refuses to remove a parameter a consumer still claims, and the whole stack
     delete fails with it. The parameter lives in the **consuming** region, not the
     producing one:
     ```bash
     aws ssm get-parameters-by-path --region eu-central-1 --path /cdk/exports/ --recursive
     ```
  3. then delete:
     ```bash
     aws cloudformation delete-stack --region us-east-1 --stack-name <StackName>
     ```

  (Real case: `Explorer-production-CloudFrontWaf` in `us-east-1`, task 0302. Its
  WebACL ARN was consumed by `Delivery` in `eu-central-1`.)

- **`--require-approval broadening`** — the deploy pauses for confirmation
  if the change broadens IAM or security-group rules. Review, then approve.
- **Preview:** `make -C infra diff-production`, or raw `cdk diff <stack>`.

---

## Component recipes (the ones with nuance)

### Backend Lambdas — Compute stack

All three Rust Lambdas (API, Ledger Processor/indexer, type-1 enrichment
worker) live in `Explorer-production-Compute`:

```bash
make -C infra deploy-production-compute
```

**Pausing the indexer or the enrichment worker.** Both consume an SQS
queue via an event-source-mapping (ESM); pausing = stopping that
consumption. Messages keep landing in the queue and are **not dropped**
(the S3→SQS notification is not gated on the pause), so either lever below
is safe and fully reversible. Pick by how long the pause needs to last:

- **Quick / temporary — disable the ESM (no deploy).** The fastest lever:
  flip the SQS trigger off and the Lambda stops parsing (takes effect in
  under a minute). Console: Lambda → the function → **Configuration →
  Triggers** → the SQS trigger → **Disable**. CLI:

  ```bash
  aws lambda list-event-source-mappings \
    --function-name production-soroban-explorer-indexer \
    --query 'EventSourceMappings[].UUID' --output text
  aws lambda update-event-source-mapping --uuid <uuid> --no-enabled   # resume: --enabled
  ```

  (enrichment worker: `production-soroban-explorer-enrichment-worker`.)

  > ⚠️ **Not durable.** `production.json` still says `concurrency: 1`, so
  > the **next `make deploy-production-compute` re-enables the ESM** (CDK
  > reconciles it back to `enabled`). Use this for a short, watched pause —
  > not to park a worker across deploys.

- **Durable — set concurrency to `0` + redeploy Compute.** Puts the paused
  state in code so it survives future deploys. The ESM is created **only
  when concurrency > 0** (`compute-stack.ts`), so:

  - `indexerLambdaConcurrency: 0` → indexer ESM not created at all.
  - `enrichmentWorkerLambdaConcurrency: 0` → same for the enrichment worker.

  Set back to `1` + redeploy to resume. **A redeploy alone does not toggle
  this** — the `production.json` value is the switch.

### Galexie (live ingestion) — Ingestion stack

The Galexie version is **pinned by ECR image digest** in
`infra/envs/production.json → galexieImageTag`. CDK resolves it through
`ContainerImage.fromEcrRepository(repo, galexieImageTag)`, which calls
`repositoryUriForTagOrDigest` — a `sha256:…` value is therefore treated as a
**digest** (`repoUri@sha256:…`), not a tag. The image must already be in the
`production-galexie` ECR repo.

Bump procedure — **pull → tag → push → sha**:

1. **Mirror the image Docker Hub → ECR:**

   ```bash
   REPO=$(aws ssm get-parameter --region eu-central-1 \
     --name /soroban-explorer/production/ecr-galexie-repo-uri \
     --query Parameter.Value --output text)
   aws ecr get-login-password --region eu-central-1 | \
     docker login --username AWS --password-stdin "${REPO%%/*}"

   docker pull stellar/stellar-galexie:<version>          # or @sha256:<hub-digest>
   docker tag  stellar/stellar-galexie:<version> "$REPO:<version>"
   docker push "$REPO:<version>"        # ← note the digest ECR prints back
   ```

2. **Put the ECR digest in `production.json → galexieImageTag`.** If you missed
   what `docker push` printed:

   ```bash
   aws ecr describe-images --region eu-central-1 \
     --repository-name production-galexie --image-ids imageTag=<version> \
     --query 'imageDetails[0].imageDigest' --output text
   ```

   > ⚠️ **This is NOT the Docker Hub digest.** Docker Hub serves a multi-arch
   > manifest list; pushing to ECR rewrites the manifest, so the two digests
   > differ. The 27.0.0 pin is Hub `sha256:81a9e829…` but ECR
   > `sha256:91eae7af…` — and it is the **ECR** one that belongs in
   > `production.json`. Copying the Hub digest across yields an image ECS
   > cannot pull.

3. **Roll the ECS task:**

   ```bash
   make -C infra deploy-production-ingestion
   ```

4. **Verify:** ECS service healthy + the Galexie ingestion-lag alarm quiet
   (a stalled Galexie = an ingestion outage; the lag alarm is your signal).

> **`GALEXIE_IMAGE_DIGEST` (GitHub `staging` Environment) is not part of this.**
> It is read **only** by `.github/workflows/deploy-staging.yml`, which is dead
> (see [§ No staging, no CI](#no-staging-no-ci)) — so updating it has **no effect
> on a manual deploy**. `production.json` is the pin. Task 0367 updated the
> variable as bookkeeping; treat it as a record, not a lever.

> Do **not** flip `assignPublicIp` on the Galexie task — it is the only
> egress path (no NAT GW post-0239). There is a CODEOWNERS-flagged inline
> comment in `ingestion-stack.ts` guarding this.

### Frontend SPA — Delivery stack + `deploy-production-web`

`Delivery` provisions the CloudFront distribution and the SPA bucket. The
**content** is a separate step that builds the SPA (baking
`VITE_API_BASE_URL` from `cloudflareApiDomainName`), syncs to S3, and
invalidates CloudFront:

```bash
make -C infra deploy-production-web
```

### CloudWatch alarms / API Gateway / DNS

- Alarms & dashboards: `make -C infra deploy-production-cloudwatch`
  (use the `--exclusively` raw form if other stacks have pending changes).
- API Gateway cache / throttle / edge-lock toggles live in
  `production.json` (`apiGatewayCache*`, `apiGatewayThrottle*`,
  `enableApiMtls`, `enable*Lock`): `make -C infra deploy-production-apigateway`.
- CH box IP changed (box replacement): update the SSM parameter, then
  `make -C infra deploy-production-hetzner-dns` (see
  [`infra/README.md` § HetznerDnsStack](../infra/README.md#hetznerdnsstack--route-53-record-for-the-production-clickhouse-box)).

---

## The machine (Hetzner ClickHouse)

The production data plane runs on the Hetzner dedicated server
`ch-prod-01`: ClickHouse + Caddy (mTLS) + Borg backups, all via
`docker-compose.prod.yml`. **Deploy is Ansible from a laptop — not CI.**

```bash
source ~/.config/soroban-prod.env                   # secrets from password manager
cd infra-hetzner/ansible
ansible-playbook -i inventory.ini site.yml          # full run (idempotent)
ansible-playbook -i inventory.ini site.yml --tags app   # after a docker-compose.prod.yml / .env change
```

`--check --diff` for a dry run; `--tags {app,security,hetzner,storagebox,users}`
to scope a re-run.

Everything else about the box — **first-time setup, disaster recovery
(box loss / restore from Borg), CH & Borg password rotation, adding/removing
a developer, the `--force-recreate clickhouse` log-rotation gotcha, and
post-deploy verification** — is fully documented and **not duplicated
here**:

➡️ **[`infra-hetzner/README.md`](../infra-hetzner/README.md)**

Machine access = your personal SSH key + mTLS client cert, both distributed
via the team password manager (see that README).

---

## Cloudflare edge

DNS record + WAF + API-origin mTLS lock for `api-sorobanscan.rumblefishdev.com`.
Terraform, run from a laptop, gated behind `terraform plan` flags:

➡️ **[`infra/cloudflare/README.md`](../infra/cloudflare/README.md)**

---

## Verify after deploy

| Surface      | Check                                                                                                           |
| ------------ | --------------------------------------------------------------------------------------------------------------- |
| Frontend     | https://sorobanscan.rumblefish.dev loads (HTTP 200, `text/html`)                                                |
| API (legacy) | `curl -f https://api.sorobanscan.rumblefish.dev/health`                                                         |
| API (via CF) | `curl -f https://api-sorobanscan.rumblefishdev.com/health`                                                      |
| API docs     | https://api.sorobanscan.rumblefish.dev/api-docs (Swagger UI)                                                    |
| CH box       | see [`infra-hetzner/README.md` § Post-deploy verification](../infra-hetzner/README.md#post-deploy-verification) |
| Alarms       | CloudWatch → `Explorer-production-CloudWatch` dashboards; no alarms in ALARM                                    |

---

## Rollback

- **AWS stack:** check out the last-good commit, `make -C infra build`,
  redeploy the affected stack. CDK has no app-level rollback; a _failed_
  deploy auto-rolls-back at the CloudFormation layer, but a _successful_
  bad deploy is undone by redeploying the previous code.
- **Frontend:** `make -C infra deploy-production-web` from the good commit.
- **Galexie:** set `galexieImageTag` back to the previous digest +
  `deploy-production-ingestion`.
- **Machine:** `ansible-playbook … --tags app` from a good checkout; data
  restore is Borg (see infra-hetzner DR).

---

## Config vs secrets

- **Non-secret config** — `infra/envs/production.json` (domains, sizes,
  thresholds, Galexie digest, concurrency knobs). Committed; changing it +
  redeploying is how you tune prod.
- **Secrets** — AWS Secrets Manager (per-service mTLS bundles) + the team
  password manager (box env). **Never** in the repo, CDK context, or
  workflow YAML.
