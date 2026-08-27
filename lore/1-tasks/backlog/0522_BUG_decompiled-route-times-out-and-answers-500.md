---
id: '0522'
title: 'BUG: the decompiled-contract route times out and answers a bare 500'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0465', '0455']
tags: [api, contracts, timeout]
links: []
history:
  - date: 2026-08-27
    status: backlog
    who: karolkow
    note: 'Task created from the 0455 mute investigation'
---

# BUG: the decompiled-contract route times out and answers a bare 500

## Summary

`GET /v1/contracts/{contract_id}/decompiled` computed for 10.244 s and
returned HTTP 500 to a browser. The API logged
`decompilation timed out for <contract_id>` at WARN in the same request. The
user got no explanation, and the route offers no partial answer, no retry
hint and no "this contract is too large" state — only a server error.

## Status: Backlog

Found 2026-08-27 while investigating why nine days of alarm actions never
reached the channel (task 0455). The alarm that fired for this 500 was one of
the eighteen that delivered nowhere, so the defect had already been live for a
day before anyone saw it.

## Context

Measured, not inferred:

- One X-Ray fault in the window: `GET /v1/contracts/{id}/decompiled`,
  HTTP 500, 10.244 s, trace `1-6a8e8e93-...`.
- The `decompilation timed out` WARN carries the **same trace id**, so the
  warning and the 500 are one request, not two neighbouring events.
- Frequency across four sampled days of X-Ray fault queries: 2026-08-20 none,
  08-22 none, 08-24 none, 08-26 one. Rare, and so far tied to one contract.
- The stage has `TracingEnabled: true`, which is why this was diagnosable at
  all: `accessLogSettings` is null and `metricsEnabled` is false, so neither
  the request nor a per-route error count exists outside X-Ray.

Two things are wrong and they are separable:

1. **The timeout itself.** Whether a contract of this size can be decompiled
   inside the budget at all, or whether the budget is simply too small.
2. **What the user is shown.** A bare 500 tells the reader the site is broken.
   A timeout on an expensive, optional view is not the same thing as the site
   being broken, and it should not read as such.

The decompiled tab shipped under task 0465 (archived).

## Implementation Plan

### Step 1: Reproduce and measure

Call the route for the contract from the incident and for a size spread of
others. Record wall time against contract size, and find where the budget
sits relative to the distribution.

### Step 2: Decide the honest failure shape

A timeout is a known, expected outcome for this route, so it deserves a
status and a message of its own rather than a 500. Options to weigh: a 504
with a body naming the cause, a 200 carrying an explicit "not decompiled -
too large" state the frontend renders, or a partial result. Whatever is
chosen, the rule from lore-0455 applies: **never a plausible-but-wrong
display** - the reader must be able to tell "we could not do this" from
"there is nothing here".

### Step 3: Decide whether the timeout can be avoided

Only after step 1. Raising a budget without knowing the distribution moves the
cliff rather than removing it.

### Step 4: Make the failure countable

X-Ray answered the single-request question. It does not answer "how often does
this route time out in a week". Per-method metrics on the stage
(`metricsEnabled`) would; that decision is tracked in 0455 and this task
should record whichever way it goes rather than restate it.

## Acceptance Criteria

- [ ] Wall time measured against contract size across a spread, and the
      budget's position in that distribution stated
- [ ] A timeout no longer answers a bare 500; the response distinguishes
      "could not decompile" from "nothing to show"
- [ ] The frontend renders that state explicitly rather than as an error page
- [ ] A recurrence is countable without opening X-Ray, or the reason it is not
      is written down
- [ ] **Docs updated** — `docs/runbooks/api-5xx.md` gains this error class if
      the fix changes what an operator sees; `N/A` with a reason otherwise
- [ ] **API types regenerated** — required if the response shape changes
      (`npx nx run @rumblefish/api-types:generate`); `N/A` with a reason
      otherwise
