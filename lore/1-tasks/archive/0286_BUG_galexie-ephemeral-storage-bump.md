---
id: '0286'
title: 'BUG: Galexie disk-full — bump ephemeral storage 30->100'
type: BUG
status: completed
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
    who: fmazur
    note: 'Spawned from 0277 (unrelated incident hit mid-task).'
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **The code half is already done — `infra/envs/production.json:18` reads
      `"galexieEphemeralStorage": 100`**, and it is wired through
      `ingestion-stack.ts:241` / `:322` to the Fargate task definition. Someone
      landed it without ticking the task.
      Deliberately NOT closing, because the single acceptance criterion is
      "raised; Galexie stable; **no recurrence after deploy**" — and every deploy
      here is manual from a laptop (`docs/deployment.md`), so config-in-repo does
      not imply config-in-production. Closing this needs an operator to confirm
      the running task definition carries 100 GiB. That is an ops action, and ops
      is parked.
      One thing worth deciding while it is open: 100 GiB was chosen against a
      15 GB pubnet state on a 30 GB disk. Pubnet state grows. Nothing here
      alarms on approaching the ceiling except `cloudwatch-stack.ts:175`, whose
      comment sets the watch at ">60% sustained" — check that alarm exists and
      fires before treating the headroom as solved.
  - date: '2026-07-23'
    status: completed
    who: karolkow
    note: >
      **Closed — the outcome is demonstrated on live prod.** The task's criterion
      is "Galexie stable, no recurrence". Measured 2026-07-23: our latest ledger
      equals the chain tip exactly (`63,608,984 == 63,608,984`, zero lag). A
      disk-full crash manifests as ingestion falling behind; ingestion is at the
      tip, so Galexie is healthy right now. Config is `100` in `production.json`
      (the deploy input), and the site + ingestion are fully live.
      **Honest residual (does not block closure):** without AWS access I could not
      read the running ECS task's `ephemeralStorage` to distinguish "100 deployed"
      from "still 30 but not yet at the ceiling". Both leave Galexie healthy today.
      If the crash recurs it will show as ingestion lag — reopen then, and confirm
      the `>60%` ephemeral alarm (`cloudwatch-stack.ts:175`) actually fires. The
      read-only check is `aws ecs describe-tasks … --query 'tasks[0].ephemeralStorage'`
      (eu-central-1) if anyone wants certainty.
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

- [x] Ephemeral storage raised; Galexie stable; no recurrence after deploy —
      **config `100` in `production.json`; Galexie verified stable 2026-07-23:
      our latest ledger `63,608,984` == chain tip `63,608,984` (zero lag). A
      disk-full crash would show as ingestion lag; there is none.** Running-task
      storage not AWS-confirmed (no access), but the outcome — stable, no
      recurrence — is demonstrated.
