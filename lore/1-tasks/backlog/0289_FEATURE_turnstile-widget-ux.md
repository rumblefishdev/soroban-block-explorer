---
id: '0289'
title: 'FEATURE: Turnstile widget UX (clean gate, do not block all reads)'
type: FEATURE
status: backlog
related_adr: ['0048']
related_tasks: ['0277']
tags: [phase-future, effort-medium, priority-low, frontend, ux, turnstile]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: claude
    note: 'Spawned from 0277 future work.'
---

# Turnstile widget UX

## Summary
Replace the minimal floating-overlay Turnstile render with a clean entry gate, and reconsider gating
ALL reads behind a session (currently every API call awaits a Turnstile solve).

## Context
0277 ships a functional but rough UX: the widget renders as a centered fixed overlay over content,
and the request interceptor blocks every call until a session exists (`web/src/api/session.ts`).

## Implementation
- Dedicated verifying/gate screen (or fixed corner) instead of the floating kafelek.
- Mint the session in the background; don't block first paint / non-data nav on the solve.

## Acceptance Criteria
- [ ] Clean widget placement; data loads with at most one solve per session; no overlay over content
