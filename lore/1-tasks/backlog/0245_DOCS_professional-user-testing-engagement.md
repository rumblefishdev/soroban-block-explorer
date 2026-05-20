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
      Spawned z M1-M3 sequencing planu (2026-05-20). Realizuje D3 spec
      "Professional user testing completed" (tech design §7.4 Deliverable 3).
      Wymagane przed M3 prod launch — capture critical findings jako bug tasks.
---

# Professional user testing engagement

## Summary

D3 spec wymaga "Professional user testing completed" przed mainnet launch.
Zaangażować zewnętrzny user testing service lub recruit pre-selected testerów,
przeprowadzić full user journey, capture findings jako follow-up bug tasks
przed prod deploy.

## Context

Per `docs/architecture/technical-design-general-overview.md` §7.4 Deliverable 3:

> "Professional user testing completed"

To explicit AC dla D3 budget allocation. Cel: catch UX issues, accessibility
problems, missing features, confusing flows przed publiczny launch — tańsze
naprawiać przed niż po prod incident.

## Implementation Plan

### Step 1: Test plan

Spisać `docs/post-launch/user-testing-plan.md`:

- **Personas**: typical Stellar user (developer / power user), casual explorer
  user (first-time visitor)
- **Tasks** (user journey):
  1. Search for known transaction hash → land on tx detail page
  2. Search for known account ID → land on account detail
  3. Search for known contract ID → land on contract detail with interface
  4. Browse latest ledgers → drill into specific ledger
  5. Browse asset list → drill into specific asset → see holders / total supply
  6. Find NFT collection → view single NFT detail with metadata
  7. Find liquidity pool → view chart + participants
  8. Use network indicator to verify mainnet vs testnet awareness
- **Success criteria per task**: time-to-completion, error rate, user-reported confusion
- **A11y checks**: keyboard navigation, screen reader, color contrast

### Step 2: Recruit testers

Opcje:

- (a) Zewnętrzny user testing service (UserTesting.com, Maze, etc.) — ~$200-500/user
- (b) Recruit Stellar community via Twitter / Discord — free ale slower
- (c) Internal RumbleFish team test (only as supplement, not primary)

Min N=5 testerów (rule-of-thumb dla user testing — 80% issues catchable z 5).

### Step 3: Wykonanie

- Każdy tester wykonuje pełny user journey (zaobserwowany via screen + audio
  recording lub written report)
- Captured: time per task, errors, points of confusion, feature requests,
  a11y issues

### Step 4: Synthesis + action items

Spisać `docs/audits/post-launch-user-testing-YYYY-MM-DD.md`:

- **Findings**: kategorie (UX, A11y, Performance, Feature gap, Bug)
- **Severity**: blocker / major / minor / nice-to-have
- **Action items**: spawned jako backlog bug tasks (blockerzy = M3 prerequisite,
  minor = post-launch)

### Step 5: Critical findings → spawned bug tasks

Per każdy "blocker" lub "major" finding: spawn backlog task with:

- `priority-high` jeśli blocker
- `priority-medium` jeśli major
- `related_tasks: ['0245']`
- History note: "Spawned from 0245 user testing findings"

## Acceptance Criteria

- [ ] `docs/post-launch/user-testing-plan.md` napisany + reviewed by team
- [ ] N ≥ 5 testerów wykonało full user journey
- [ ] Findings captured w `docs/audits/post-launch-user-testing-YYYY-MM-DD.md`
- [ ] Critical findings (blocker + major) spawned jako bug tasks
- [ ] Blocker bug tasks zaadresowane przed M3 prod deploy
- [ ] Minor findings logged jako post-launch backlog (nie blokują launch)
- [ ] **Docs updated** — N/A — task creates docs, doesn't modify architecture docs
- [ ] **API types regenerated** — N/A — task does not touch API code

## Notes

- Budget consideration: zewnętrzny service ~$1k-2.5k dla N=5 testerów. Wlicza się
  w M3 budget (40% project total).
- Timing: po M2 done (staging stable), przed M3 prod deploy. Min 2-tygodniowy
  buffer na findings remediation.
- Team alignment przed zaangażowaniem — fmazur signoff wymagany.
