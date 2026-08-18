---
id: '0475'
title: 'FEATURE: /release skill — collect what actually ships and cut the production tag'
type: FEATURE
status: completed
related_adr: ['0009', '0052']
related_tasks: ['0390']
tags: [ci, cd, tooling, priority-low, effort-small, layer-infra]
links:
  - .github/workflows/deploy-production.yml
  - .claude/skills/issues/SKILL.md
history:
  - date: '2026-08-12'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0390, and deliberately scoped DOWN from the original idea
      ("collect the changes AND decide which stacks to deploy"). Stack selection
      is already answered exactly by the `cdk diff` the workflow runs and prints,
      and the tag path deploys a fixed set anyway — a path→stack heuristic would
      be a second, drift-prone source of truth that is wrong precisely when it
      matters. What is genuinely unserved is "what is actually shipping".
      Deferred until after the first real `production-*` tag so the skill is
      written from what hurt, not from a guessed pipeline.
  - date: '2026-08-17'
    status: completed
    who: stkrolikiewicz
    note: >
      Single-phase — promoted from backlog and completed in one PR.
      The deferral paid off. Written the same day as
      production-2026.08.17-1 and -2, so every caution in the skill is a
      thing that actually went wrong rather than a guess: a `Refs #405` that
      was a docs-only commit, a `Refs #371` on a disk-headroom fix, a
      backfill-runner change that merged but does not deploy, a green tag whose
      SPA smoke can pass against the stale bundle, a verification query that
      could not measure the thing it claimed to, and a cold Rust cache that
      makes every tag cost ~15 min forever. A skill guessed on 08-12 would have
      had none of them. `.claude/skills/release/SKILL.md` plus the `/issues`
      Step 4 handoff.
---

# /release — collect what ships, then cut the tag

## Summary

A skill that answers **"what is in this release?"** for the tag-driven
production deploy added in 0390, and hands back the exact `git tag` command.
It does **not** choose stacks.

## Context

Since [0390](../active/0390_REFACTOR_deploy-workflow-cleanup-prod-template.md),
a release is `git tag production-YYYY.MM.DD-N` (see
[`docs/deployment.md` § Releases](../../../docs/deployment.md#releases-and-the-ci-deploy-path)).
Nothing assembles the release's contents today: `/issues` Step 4 literally asks
the operator what they just deployed, and during the 0437 incident pinning the
live commit needed manual bundle-hash detective work.

## Implementation

- Range = last `production-*` tag (`git describe --tags --match 'production-*'`)
  → `master`. First run has no previous tag: fall back to a caller-supplied ref
  and say so rather than dumping all history.
- From that range collect, per merged PR: number + title, `lore-NNNN` scopes
  from the commit trailers, and `Refs #NNN` issue references.
- Emit: a short release note, the list of issues now shippable (input for
  `/issues` Step 4 — the two skills chain, they do not overlap), and the tag
  command with `-N` resolved against tags already pushed for that date.
- Drafts only, never pushes the tag. Same rule as `/issues`: a human sends.
- Update `.claude/skills/issues/SKILL.md` Step 4 — its forward-looking note
  ("when a tag becomes the deploy trigger, replace this question with reading
  the release") is exactly this handoff.

## Out of scope

- **Deciding which stacks to deploy.** `cdk diff` runs inside the workflow and
  is printed as the log record of what changed; tag runs deploy the standard
  set (Compute + SPA), and surgical runs are a human choice on
  `workflow_dispatch`. Re-deriving that from changed paths adds a heuristic
  that must chase the CDK app.
- Build-SHA stamping (`VITE_COMMIT_SHA` + `/version`) — related, tracked in
  0390's hardening list, useful input for this skill but a separate change.

## Acceptance Criteria

- [x] `/release` prints the PR / task / issue list for the range and the exact
      tag command.
- [x] ~~no GitHub write of any kind~~ — **narrowed, deliberately.** The skill
      does create the release PR (`gh pr create`), because that is the step
      the operator wanted automated and it is reversible. What stays
      human-only is the pair that actually matters: **pushing the tag**, which
      is the release decision and the pipeline's only approval gate, and
      **posting issue comments**, which `/issues` already forbids. A blanket
      no-writes rule would have banned the PR while permitting nothing safer.
- [x] First-release case (no previous `production-*` tag) handled explicitly —
      stops and asks for `--since` rather than dumping all history.
- [x] `/issues` Step 4 updated to read the release instead of asking, and
      carries the two cautions below.
- [x] Written **after** at least one real tag deploy — after two, on
      2026-08-17, against what those runs actually needed.
