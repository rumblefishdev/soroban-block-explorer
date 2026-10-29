---
id: '0658'
title: 'Move the Cloudflare TF-state bucket stack to soroban-infrastructure'
type: OPS
status: active
related_adr: ['0048']
related_tasks: ['0277', '0312']
tags: [infra, cloudflare, cdk, priority-medium, effort-small]
links:
  - 'https://github.com/rumblefishdev/soroban-infrastructure/pull/3'
history:
  - date: '2026-10-29'
    status: active
    who: stkrolikiewicz
    note: >
      Created and started: soroban-infrastructure#3 takes the stack over as a
      CloudFormation template; this task takes it out of the CDK app.
---

# Move the Cloudflare TF-state bucket stack to soroban-infrastructure

## Summary

`Explorer-production-CloudflareBootstrap` (0277) holds one S3 bucket,
`production-soroban-explorer-cf-tfstate`: the Terraform state of
`infra/cloudflare/` and, since 2026-10-26, of soroban-infrastructure's
`cloudflare/` (the `sorobanscan.com` zone, stellar-x402's tunnel). Shared
Soroban infrastructure lives in soroban-infrastructure, so the stack moves
there as a plain CloudFormation template (soroban-infrastructure#3). This task
stops the explorer's CDK app from defining it, so two repos never deploy the
same stack.

## Stan teraz

- Done: soroban-infrastructure#3 (draft), with a preview change set on the live
  stack: no replacement, tags and the same TLS-only policy only.
- Next: this PR merges, then #3's change set is executed.
- In force: the stack keeps its name and logical ids; the bucket and its
  name stay, and `infra/cloudflare/` keeps its state there.

## Implementation Plan

### Step 1: Take the stack out of the CDK app

`cloudflare-bootstrap-stack.ts`, its use in `app.ts`, the
`provisionCloudflareBootstrap` flag in `types.ts` and `envs/*.json`, and the
Makefile's deploy and destroy targets. Removing a stack from a CDK app never
deletes it.

### Step 2: Point the docs at soroban-infrastructure

`infra/README.md`, `infra/cloudflare/{README.md,backend.hcl.example,versions.tf}`,
`docs/deployment.md`, the selector list in `deploy-production.yml` and the
deploy-scope self-check. Accounts of past deploys stay as written.

## Acceptance Criteria

- [ ] `cdk synth` of production lists no `CloudflareBootstrap` stack, and the
      live stack is unchanged by it
- [ ] Nothing in `infra/` or the docs tells a reader to deploy the bucket from
      this repo
- [ ] **Docs updated** — N/A: no file under `docs/architecture/**` names the
      stack.
- [ ] **API types regenerated** — N/A: nothing under `crates/api/**`.
