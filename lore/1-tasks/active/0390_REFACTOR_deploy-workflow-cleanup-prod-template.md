---
id: '0390'
title: 'CI: retire dead staging-deploy workflow + add dispatch-only production deploy template'
type: REFACTOR
status: active
related_adr: []
related_tasks: []
tags: [priority-low, effort-small, layer-infra, milestone-3, phase-launch]
milestone: 3
links:
  - .github/workflows/deploy-production.yml
history:
  - date: 2026-07-14
    status: active
    who: stkrolikiewicz
    note: >
      Spawned to back the CI deploy-workflow cleanup (PR #338) — retroactively,
      per the task-gate. `deploy-staging.yml` was a fossil pointing at a
      us-east-1 staging env that no longer exists (verified: 0 Explorer-staging-*
      stacks). Replaced with a dispatch-only prod-deploy template. Fossil-removal
      + template are done in #338; the post-launch enablement is the open work.
  - date: 2026-07-14
    status: active
    who: stkrolikiewicz
    note: >
      Renumbered 0388 → 0390: id 0388 collided with the concurrently-created
      `0388_BUG_repair-tier1-soroban-contracts-name-mismatch` on develop (max id
      was 0389). Content unchanged — only id + filename moved. Earlier PR #338
      commits still reference `lore-0388` (historical, not rewritten).
---

# CI: retire dead staging-deploy workflow + prod deploy template

## Summary

`.github/workflows/deploy-staging.yml` targeted a **us-east-1** staging
environment (`staging.json` / `Explorer-staging-*` / `staging.sorobanscan…` with
basic-auth) that **no longer exists** — verified `0` `Explorer-staging-*` stacks
in us-east-1. It was `staging-*`-tag-triggered (never pushed) → dead and
misleading. Production actually runs in **eu-central-1** (10 `Explorer-production-*`
stacks), deployed **manually** via `make deploy-production-*`, with no automated
prod-deploy path. This task retires the fossil and adds a **safe, dispatch-only
production-deploy template** — not wired in — for deliberate post-launch adoption.

## Context

- Prod = eu-central-1, single env (prod = the "staging"-named GitHub env; see
  the no-separate-staging reality). Deploys are operator-machine manual today.
- The manual path is what gave control during the 2026-07 read-path perf work
  (per-stack `--exclusively` + `cdk diff` avoided shipping indexer/enrichment
  drift). Any automated path must preserve that (no blind `--all`).

## Implementation

### Done (PR #338)

- **Removed** `deploy-staging.yml` + **`scripts/staging-deploy.sh`** (the dead
  `staging-*` tag-trigger that fired it) — both → `.trash/`. The staging CDK app
  and its config (`infra/src/bin/staging.ts`, `infra/envs/staging.json`) were
  **already gone**, so the workflow was doubly dead — a dead trigger and a
  missing deploy target (`node dist/bin/staging.js`).
- **Added** `deploy-production.yml` as a POST-LAUNCH TEMPLATE:
  `workflow_dispatch` only → build → `cdk diff` (prints) → **manual approval gate**
  (`production` environment, required reviewers) → deploy a **chosen stack** (input,
  `--exclusively` default, not `--all`) → smoke (`/health` + public frontend).

### Open — post-launch enablement (deliberate, do NOT do pre-launch)

- Create the GitHub `production` environment with **required reviewers** (the
  human gate between diff and deploy).
- Provision secrets: `AWS_DEPLOY_ROLE_ARN`, `AWS_ACCOUNT_ID`.
- Decide release cadence (drives whether auto-triggers are ever added — default:
  keep dispatch-only).
- Frontend SPA content sync stays a separate step (`make deploy-production-web`).

## Acceptance Criteria

- [x] Dead `deploy-staging.yml` removed; us-east-1 staging confirmed absent (0 stacks).
- [x] `deploy-production.yml` added — dispatch-only, diff→approval-gate→per-stack
      deploy→smoke; header documents prerequisites.
- [x] Template is inert (won't run until the `production` environment + secrets
      exist and someone dispatches it).
- [ ] Post-launch: `production` environment + reviewers + secrets provisioned;
      first deliberate deploy via the workflow validated.
- [ ] **Docs updated** — N/A (CI tooling; does not change the architecture's shape).
- [ ] **API types regenerated** — N/A (no API surface change).

## Notes

- **Leftover staging references** (deliberately NOT removed here):
  - `lore/2-adrs/0009_staging-deploy-trigger-strategy.md` — kept as historical
    record (ADR convention); now **superseded** by this task.
  - `lore/1-tasks/backlog/0115_FEATURE_cdk-diff-early-exit-staging-deploy.md` —
    now **moot** (optimizes a deleted pipeline); candidate to drop/close.
  - `crates/backfill-runner/src/ch_staging.rs` — **unrelated** (ClickHouse
    backfill _staging tables_, not the deploy env); left alone.
- Same lesson as the branch-model discussion: the manual path works and gives
  control; automate/rebuild **post-launch, deliberately** — not as pre-launch churn.
- `master = production + deliberate release` is a separate, optional post-launch
  maturity step; if adopted, this workflow's `workflow_dispatch` slots naturally
  into a `develop→master` release gate.
