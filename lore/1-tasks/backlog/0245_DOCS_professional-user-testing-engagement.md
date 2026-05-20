---
id: '0245'
title: 'DOCS+OPS: Professional user testing engagement before mainnet launch'
type: DOCS
status: backlog
related_adr: []
related_tasks: ['0127']
tags:
  [
    priority-medium,
    effort-small,
    layer-ops,
    user-testing,
    pre-launch,
    quality-assurance,
  ]
milestone: 3
links:
  - docs/architecture/technical-design-general-overview.md
history:
  - date: '2026-05-20'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from M1-M3 sequencing plan (2026-05-20). Implements the D3 spec
      "Professional user testing completed" (tech design §7.4 Deliverable 3).
      Required before M3 prod launch — captures critical findings as bug tasks.
---

# Professional user testing engagement

## Summary

D3 spec requires "Professional user testing completed" before mainnet
launch. Engage an external user testing service or recruit pre-selected
testers, execute the full user journey, and capture findings as follow-up
bug tasks before prod deploy.

## Context

Per `docs/architecture/technical-design-general-overview.md` §7.4
Deliverable 3:

> "Professional user testing completed"

This is an explicit AC for the D3 budget allocation. Goal: catch UX issues,
accessibility problems, missing features, and confusing flows before public
launch — they are cheaper to fix before a prod incident than after.

## Implementation Plan

### Step 1: Test plan

Author `docs/post-launch/user-testing-plan.md`:

- **Personas**: typical Stellar user (developer / power user), casual
  explorer user (first-time visitor)
- **Tasks** (user journey):
  1. Search for known transaction hash → land on tx detail page
  2. Search for known account ID → land on account detail
  3. Search for known contract ID → land on contract detail with interface
  4. Browse latest ledgers → drill into a specific ledger
  5. Browse asset list → drill into a specific asset → see holders / total supply
  6. Find an NFT collection → view a single NFT detail with metadata
  7. Find a liquidity pool → view chart + participants
  8. Use the network indicator to verify mainnet vs testnet awareness
- **Success criteria per task**: time-to-completion, error rate,
  user-reported confusion
- **A11y checks**: keyboard navigation, screen reader, color contrast

### Step 2: Recruit testers

Options:

- (a) External user testing service (UserTesting.com, Maze, etc.) — ~$200-500/user
- (b) Recruit from the Stellar community via Twitter / Discord — free but
  slower
- (c) Internal RumbleFish team test — supplemental only, not primary

Minimum N=5 testers (rule-of-thumb for user testing — ~80% of issues are
catchable with 5 participants).

### Step 3: Execution

- Each tester completes the full user journey, observed via screen + audio
  recording or via written report.
- Capture: time per task, errors, points of confusion, feature requests,
  a11y issues.

### Step 4: Synthesis + action items

Author `docs/audits/post-launch-user-testing-YYYY-MM-DD.md`:

- **Findings**: categorized (UX, A11y, Performance, Feature gap, Bug)
- **Severity**: blocker / major / minor / nice-to-have
- **Action items**: spawned as backlog bug tasks (blockers = M3
  prerequisite, minor = post-launch)

### Step 5: Critical findings → spawned bug tasks

For each "blocker" or "major" finding, spawn a backlog task with:

- `priority-high` for blockers
- `priority-medium` for major findings
- `related_tasks: ['0245']`
- History note: "Spawned from 0245 user testing findings"

## Acceptance Criteria

- [ ] `docs/post-launch/user-testing-plan.md` authored and reviewed by the team
- [ ] N ≥ 5 testers completed the full user journey
- [ ] Findings captured in `docs/audits/post-launch-user-testing-YYYY-MM-DD.md`
- [ ] Critical findings (blocker + major) spawned as bug tasks
- [ ] Blocker bug tasks addressed before M3 prod deploy
- [ ] Minor findings logged in the post-launch backlog (do not block launch)
- [ ] **Docs updated** — N/A — task creates docs, does not modify
      architecture docs
- [ ] **API types regenerated** — N/A — task does not touch API code

## Notes

- Budget consideration: external service ~$1k-2.5k for N=5 testers. Folded
  into the M3 budget (40% of project total).
- Timing: after M2 done (staging stable), before M3 prod deploy. Minimum
  2-week buffer for findings remediation.
- Team alignment before engagement — fmazur signoff required.
