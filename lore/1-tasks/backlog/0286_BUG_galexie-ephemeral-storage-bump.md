---
id: '0286'
title: 'BUG: Galexie disk-full — bump ephemeral storage 30->100'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0277']
tags:
  [
    phase-future,
    effort-small,
    priority-high,
    infra,
    ingestion,
    galexie,
    reliability,
  ]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: claude
    note: 'Spawned from 0277 (unrelated incident hit mid-task).'
---

# Galexie disk-full — bump ephemeral storage

## Summary

Galexie captive-core crashed mid-task with `No space left on device` (15 GB pubnet state on a 30 GB
Fargate ephemeral disk). Recovered on its own (no data loss), but **will recur**.

## Context

Incident during 0277 (unrelated to the Cloudflare work). Root cause = `galexieEphemeralStorage: 30`
too tight. `infra/envs/production.json`.

## Implementation

- `galexieEphemeralStorage` 30 → ~100; `make deploy-production-ingestion` (restarts Galexie →
  catch-up; coordinate, low-impact window).
- Optional: CloudWatch alarm on ECS task ephemeral storage / restart rate.

## Acceptance Criteria

- [ ] Ephemeral storage raised; Galexie stable; no recurrence after deploy
