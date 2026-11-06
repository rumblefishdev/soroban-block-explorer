---
id: '0658'
title: 'Move the Cloudflare TF-state bucket stack to soroban-infrastructure'
type: OPS
status: completed
related_adr: ['0048']
related_tasks: ['0277', '0312']
tags: [infra, cloudflare, cdk, priority-medium, effort-small]
links:
  - 'https://github.com/rumblefishdev/soroban-infrastructure/pull/3'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/695'
history:
  - date: '2026-10-29'
    status: active
    who: stkrolikiewicz
    note: >
      Created and started: soroban-infrastructure#3 takes the stack over as a
      CloudFormation template; this task takes it out of the CDK app.
  - date: '2026-11-04'
    status: completed
    who: stkrolikiewicz
    note: >
      #695 merged, then soroban-infrastructure#3. Change set take-over-0658
      executed on the live stack at 10:19 UTC: CDKMetadata removed, bucket
      tags changed, the same TLS-only policy, no replacement. All four state
      files unchanged.
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

- Done: #695 and soroban-infrastructure#3 merged (2026-11-04); the live stack
  now runs soroban-infrastructure's `cloudflare/state-bucket.yaml`.
- In force: the stack keeps its name and logical ids; the bucket and its
  name stay, and `infra/cloudflare/` keeps its state there.
- In force until a release carries #695 to `master`: no `--all` deploy from
  `master`. It still defines the stack and would put the CDK template back.

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

- [x] `cdk synth` of production lists no `CloudflareBootstrap` stack, and the
      live stack is unchanged by it
- [x] Nothing in `infra/` or the docs tells a reader to deploy the bucket from
      this repo
- [x] soroban-infrastructure#3's change set executed: `UPDATE_COMPLETE`, tags
      `ManagedBy=cloudformation`, `Project=soroban-infrastructure`, policy and
      state files unchanged
- [x] **Docs updated** — N/A: no file under `docs/architecture/**` names the
      stack.
- [x] **API types regenerated** — N/A: nothing under `crates/api/**`.

## Design Decisions

### From Plan

1. **Same stack name and logical ids** (`CloudflareTfState6AD14780` is CDK's
   path hash): CloudFormation updates in place; a rename means orphaning and
   importing the bucket, production steps for a cosmetic gain.

### Emerged

2. **`Project` tag moved to `soroban-infrastructure`**, `ManagedBy` to
   `cloudformation`: the repo that deploys the stack now.
3. **Change set created by name** (`create-change-set take-over-0658`) instead
   of `deploy --no-execute-changeset`, so it was read before an explicit
   `execute-change-set`.
