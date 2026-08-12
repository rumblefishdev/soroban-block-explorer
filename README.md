# Soroban Block Explorer

[**Backlog Board**](https://rumblefishdev.github.io/soroban-block-explorer/)

Nx + TypeScript monorepo bootstrap for a Soroban-first Stellar block explorer.

This repository starts from the official `nrwl/typescript-template` foundation and adapts
it to the planned product architecture:

- `web` for the frontend explorer UI
- `crates/api` for the public REST API (Rust/axum)
- `crates/indexer` for ledger ingestion entrypoints (Rust)
- `infra` for infrastructure as code (AWS CDK)
- `libs/ui` for shared frontend components

## Quick Start

```bash
nvm use
npm install
npm run lint
npm run build
npm run typecheck
```

## Workspace Layout

```text
web/
crates/
  api/
  indexer/
  xdr-parser/
infra/
libs/
  ui/
docs/
  architecture/
```

## Current Status

The workspace contains:

- root Nx / TypeScript / ESLint / Prettier bootstrap
- `web` — React 19 + Vite SPA with MUI, React Router, and TanStack Query
- `libs/ui` — shared React component library (Vite lib mode)
- `infra` — AWS CDK infrastructure stacks
- `crates/` — Rust backend (api, indexer, xdr-parser)
- architecture docs aligned with the reviewed technical design

Backend: Rust (axum + utoipa + sqlx), deployed as Lambda via cargo-lambda (per ADR 0005).
They will be introduced as dedicated follow-up steps.

## Infrastructure

AWS infrastructure is managed with CDK (TypeScript) in `infra/`.

### Prerequisites

- AWS CLI configured with a named profile:
  ```bash
  aws configure --profile soroban-explorer
  ```
- Set the profile in your shell:
  ```bash
  export AWS_PROFILE=soroban-explorer
  ```

### First-time setup

Bootstrap CDK on the AWS account (once per account + region):

```bash
npm run infra:bootstrap
```

### Deploying

**See [`docs/deployment.md`](docs/deployment.md) — the single source of truth**
for what command ships what. Production is the only environment.

A release is a git tag: pushing `production-YYYY.MM.DD-N` to `master` runs
[`.github/workflows/deploy-production.yml`](.github/workflows/deploy-production.yml)
— `cdk diff` → deploy Compute → sync the SPA → smoke tests.

```bash
git tag production-$(date +%Y.%m.%d)-1 master && git push origin --tags
```

> ⚠️ The CI path is **armed but not yet exercised** — the GitHub `production`
> environment, the OIDC deploy role and its secrets were provisioned
> 2026-08-12, but no tag has run through it (task 0390). The first release is
> also its validation. The manual laptop path stays available:

```bash
make -C infra diff-production              # preview
make -C infra deploy-production-compute    # ship the API / indexer / enrichment Lambdas
make -C infra deploy-production-web        # ship the frontend SPA
make -C infra deploy-production            # deploy ALL stacks (see gotchas in the guide)
```

> There is **no staging environment**. AWS staging was retired by task 0249,
> and the `deploy-staging.yml` / `scripts/staging-deploy.sh` fossils by 0390.
> `npm run infra:*:staging` and any `make deploy-staging*` target are **dead**
> — they reference targets that no longer exist. Do not use them.
