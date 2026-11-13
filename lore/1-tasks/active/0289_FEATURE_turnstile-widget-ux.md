---
id: '0289'
title: 'FEATURE: Turnstile widget UX (clean gate, do not block all reads)'
type: FEATURE
status: active
related_adr: ['0048']
related_tasks: ['0277']
tags: [phase-future, effort-medium, priority-low, frontend, ux, turnstile]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/675']
history:
  - date: '2026-06-10'
    status: backlog
    who: fmazur
    note: 'Spawned from 0277 future work.'
  - date: '2026-10-21'
    status: backlog
    who: karolkow
    note: >
      Linked issue #675 (explorer blocked behind Cloudflare WARP). Its second
      acceptance criterion - a failed verification shows a retry state, not a
      crash - is this task's 2026-09-24 section. Its first (WARP passes the
      edge) has no reproduction yet; the issue waits for the reporter's setup.
  - date: '2026-11-13'
    status: active
    who: karolkow
    note: >
      Activated after a third report (a testnet user, night of 2026-11-13: a
      run of unsolved Turnstile re-checks, then every read 401). Scope chosen:
      keep the gate; a failed or interactive check shows a persistent verify
      panel instead of the render-error state, retries stop until it is
      solved, and the API logs each failed verification.
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

## 2026-09-24 — a failed solve reads as a crash (decision 93 A)

Observed on production in an automated browser: Turnstile never resolved,
the `TURNSTILE_TIMEOUT_MS = 20_000` watchdog in `web/src/api/session.ts` fired
after 20 s, the request went out without a Bearer, the API answered 401, and
`/liquidity-pools` showed **"Something went wrong — an unexpected error
occurred while rendering this section"**. The user waits 20 s and is then
told the page is broken, when the cause is a verification that did not pass.

- [ ] A 401 after a failed or timed-out solve renders "could not verify your
      browser — try again" (with a retry that re-solves), not the section's
      render-error boundary.

Seen again 2026-09-24 on `/search?q=<hash>` in the in-app browser: the same
"Something went wrong" section, with `Uncaught TurnstileError: [Cloudflare
Turnstile] Nothing to reset found for provided container` in the console —
a `reset()` called on a widget that was never rendered (or already removed),
so the failure path itself throws. Guard the reset on a live widget id.

## 2026-11-13 — verify panel instead of a 401 (this PR)

Third report: a testnet user left the home page open overnight. A 15-min
re-mint needed a click, the 20 s watchdog removed the widget, every read went
out without a Bearer and got 401, and each poll started a new challenge
(Turnstile analytics: ~85 unsolved in two hours). Owner chose to keep the gate
and fix what the visitor sees; opening reads or a challenge-after-limit
(Blockscout's model) stay out of scope.

## Premise

Fresh review of the goal alone (2026-11-13):

- **holds** — the client causes the failure: a failed solve resolved `null`,
  the bare request got 401, `QueryProvider` does not retry 4xx, and the
  4-8 s home poll started a new solve each tick.
- **holds** — the cheapest root fix is the one built here: stop the 20 s
  watchdog once Cloudflare asks for a click, and stop solving after a failure.
- **wrong as evidence of harm** — "124 of 158 unsolved" does not mean most
  visitors fail; one stuck tab re-rendering every ~25 s explains it (derived,
  not measured; both networks share one site key).
- **doubtful** — the 15-min TTL makes an idle tab re-check ~4 times an hour;
  kept by the owner's scope, listed under Not yet decided.
- **doubtful** — a WARP block at the edge is unproven: the 2026-10-05 launch
  note says Managed Challenge stays off, while checklist row 2 says it is on.

## Decisions so far

| #   | Decision                                                                                                              | From                    | Test                                    |
| --- | --------------------------------------------------------------------------------------------------------------------- | ----------------------- | --------------------------------------- |
| 1   | Armed, `ensureToken` never resolves `null`: requests wait for a session, none go out bare                             | owner (W25 C)           | solver: stall → panel → retry → JWT     |
| 2   | A failed solve or a refused mint shows a panel with the reason and "Try again"; nothing re-solves until it is clicked | owner (W25 C)           | solver: retry panel, HTTP status        |
| 3   | Cloudflare asks for a click → the widget stays, with a heading; the watchdog stops counting                           | owner (W25 C)           | solver: interactive heading, no timeout |
| 4   | No callback within 20 s and no interaction → failure → panel (2), widget removed                                      | plan review (blocker 1) | solver: stall shows the retry panel     |
| 5   | `refresh-timeout: 'manual'`, `retry: 'never'`: Cloudflare must not restart challenges by itself                       | plan review (2)         | solver: render options                  |
| 6   | The Turnstile error code reaches the panel and `console.warn`                                                         | plan review (6)         | solver: error code in the panel         |
| 7   | A script that loads without `window.turnstile` can be retried                                                         | plan review (3)         | solver: retry after missing global      |
| 8   | The API logs every refused or failed siteverify, with Cloudflare's `error-codes` or HTTP status, never the token      | owner (W25 C)           | none (log line)                         |
| 9   | A 404 from the mint means the API gate is off: resolve `null`, the request goes out bare as before                    | code review (standards) | session: 404 → null                     |
| 10  | Panel colours come from `colorsDark`; the interactive heading is added once                                           | code review             | solver: one heading                     |

The 2026-09-24 `reset()` error has no source in our code (`git log -S "reset("`
on `session.ts` is empty): Turnstile's own retry or refresh called it after
`remove()`. `retry: 'never'` and `refresh-timeout: 'manual'` should stop it;
not reproduced, so not proven.

## Not yet decided

- A 401 on a JWT the client still thinks is fresh shows the section error and
  re-mints on the next poll. No retry rule added: no measured case (review 7).
- Session TTL (15 min): a longer one would cut re-checks with the gate kept;
  out of this PR's scope.
- Turnstile analytics split by hostname and interactive vs non-interactive,
  and whether Bot Fight Mode is on for the zone: owner's dashboard, not yet read.
- After the click, a check that stalls keeps the card with no "Try again": the
  watchdog stops at interaction and is not re-armed. No measured case.
- `session.ts` now holds the session, the widget and the panel: split in a
  `[structure only]` PR; `session-widget.test.ts` then follows its source file.
