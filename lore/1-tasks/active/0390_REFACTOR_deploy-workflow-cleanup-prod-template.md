---
id: '0390'
title: 'CI: retire dead staging-deploy workflow + add tag-driven production deploy'
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
  - date: 2026-08-10
    status: active
    who: stkrolikiewicz
    note: >
      Design change (decided while planning the 0465 release): dropped the
      environment approval gate — pushing a `production-YYYY.MM.DD-N` tag IS
      the human decision, same date-tag convention the staging pipeline used.
      Workflow collapsed to one job (the two-job split only existed for the
      gate boundary); tag runs deploy Compute + SPA content, dispatch stays
      for surgical per-stack deploys. Also corrected the Summary: staging
      tags WERE pushed (4× in 2026-04, before the env teardown).
---

# CI: retire dead staging-deploy workflow + tag-driven prod deploy

## Summary

`.github/workflows/deploy-staging.yml` targeted a **us-east-1** staging
environment (`staging.json` / `Explorer-staging-*` / `staging.sorobanscan…` with
basic-auth) that **no longer exists** — verified `0` `Explorer-staging-*` stacks
in us-east-1. It was `staging-*`-tag-triggered (last pushed 2026-04.14, before
the env teardown) → dead and misleading. Production actually runs in
**eu-central-1** (10 `Explorer-production-*` stacks), deployed **manually** via
`make deploy-production-*`, with no automated prod-deploy path. This task
retires the fossil and adds a **tag-driven production deploy**: pushing a
`production-YYYY.MM.DD-N` tag (the convention the staging pipeline established)
runs diff → deploy (Compute + SPA) → smoke. The tag is the human decision —
no separate approval gate.

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
- **Added** `deploy-production.yml`, tag-driven (2026-08-10 revision):
  `push: tags: production-*` (standard release set: Compute + SPA content sync
  via `make -C infra deploy-production-web`) OR `workflow_dispatch` (surgical:
  `stacks` input, `deploy_web` opt-in) → build → `cdk diff` (printed as the log
  record) → `cdk deploy --exclusively` → smoke (`/health` + public frontend).
  Single job — the earlier two-job split existed only to host an environment
  approval gate, dropped because **the tag push is the approval**.

### Open — enablement

- Provision secrets: `AWS_DEPLOY_ROLE_ARN` (OIDC deploy role), `AWS_ACCOUNT_ID`.
  Workflow is inert until they exist.

## Acceptance Criteria

- [x] Dead `deploy-staging.yml` removed; us-east-1 staging confirmed absent (0 stacks).
- [x] `deploy-production.yml` added — tag-driven (`production-*`) + dispatch,
      diff→per-stack deploy→SPA sync→smoke; header documents prerequisites.
- [ ] Secrets provisioned; first release tag deployed via the workflow
      validated end-to-end.
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
