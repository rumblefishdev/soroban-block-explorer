---
id: '0245'
title: 'DOCS+OPS: Professional user testing engagement before mainnet launch'
type: DOCS
status: done
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
  - date: '2026-08-19'
    status: done
    who: karolkow
    note: >
      Closed without running the engagement. Launch happened 2026-07-17, so the
      pre-launch premise expired. Post-launch community feedback stood in for it
      and is what the Milestone 3 package reports (milestone-3-evidence.md § 6).
      Verified against the approved submission: professional user testing is in
      the Deliverable 3 prose, not among the six numbered acceptance criteria.
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

- (a) External user testing service — paid, per-tester pricing
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

- [ ] `docs/post-launch/user-testing-plan.md` authored and reviewed by the team —
      not authored; no moderated engagement was run
- [ ] N ≥ 5 testers completed the full user journey — not run
- [ ] Findings captured in `docs/audits/post-launch-user-testing-YYYY-MM-DD.md` —
      superseded by the public issue tracker as the findings record
- [x] Critical findings (blocker + major) spawned as bug tasks — via community
      reports on the public tracker, not via a testing engagement
- [ ] Blocker bug tasks addressed before M3 prod deploy — window expired,
      launch was 2026-07-17
- [x] Minor findings logged in the post-launch backlog (do not block launch)
- [ ] **Docs updated** — N/A — task creates docs, does not modify
      architecture docs
- [ ] **API types regenerated** — N/A — task does not touch API code

## Notes

- The external-service route is a paid engagement, funded from the M3
  allocation.
- Timing: after M2 done (staging stable), before M3 prod deploy. Minimum
  2-week buffer for findings remediation.
- Team alignment before engagement — fmazur signoff required.

## Outcome

Closed without running the engagement. The task was written 2026-05-20 as a
pre-launch gate — test plan, N ≥ 5 moderated testers, blocker findings fixed
before prod deploy, two-week remediation buffer. Launch happened 2026-07-17.
The window the plan depends on is gone, and moderated testing now answers a
question the live system already answered.

**What stood in for it.** `docs/scf/milestone-3-evidence.md` § 6 reports
post-launch community feedback in this slot: real users on live mainnet data
since launch, improvement reports filed on the public issue tracker by the team
on the reporters' behalf, and roughly half of them already resolved and shipped
(operation readability, failed-transaction cause, issuer home domain). Findings
came from real usage rather than a scripted journey, and they produced merged
fixes — which is what step 5 of the plan was for.

**Status against the approved submission.** Checked against the approved
proposal rather than our own tech-design doc: "Professional user testing
completed" appears in the Deliverable 3 prose and in its budget line, but is
**not** one of the six numbered acceptance criteria (those are public access,
public repo + reproducible deploy, monitoring dashboard, load test, security
checklist, 7-day report). So this is a prose-level deliverable that was
substituted, not a gating criterion that was missed.

**Undeclared substitution.** As with [[0129]], § Scope Refinement in the
evidence package lists three deviations — the p95 miss, the RDS-specific
data-at-rest wording, and the one raised alarm — and does not list this one.
Community feedback in place of a moderated engagement is a further deviation the
package presents as equivalent without saying so. If the package is ever
revised, one Scope Refinement point should cover this and the on-request
monitoring access together.
