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

- [ ] Clean widget placement; data loads with at most one solve per session

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

| #   | Decision                                                                                                                                                   | From                                           | Test                                               |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- | -------------------------------------------------- |
| 1   | Armed, `ensureToken` never resolves `null`: requests wait for a session, none go out bare                                                                  | owner (W25 C)                                  | solver: stall → panel → retry → JWT                |
| 2   | A failed solve or a refused mint shows a panel with "Try again"; nothing re-solves until it is clicked                                                     | owner (W25 C)                                  | solver: retry panel, HTTP status                   |
| 3   | Cloudflare asks for a click → its widget stays alone at the centre, with no card or text of ours, for as long as the visitor takes                         | owner (W25 C; 2026-11-18: widget alone)        | solver: no text, stays; e2e: widget stays 25 s     |
| 4   | No time limit of our own: every end of a check comes from a Cloudflare callback (success, error, expired, unsupported → panel)                             | owner (2026-11-18; replaces the 20 s watchdog) | solver: no time limit, unsupported → panel         |
| 5   | `refresh-timeout: 'manual'`, `retry: 'never'`: Cloudflare must not restart challenges by itself                                                            | plan review (2)                                | solver: render options                             |
| 6   | The Turnstile error code or HTTP status goes to `console.warn` only; the panel shows fixed plain text                                                      | owner (2026-11-17)                             | solver: error code out of the panel                |
| 7   | A script that loads without `window.turnstile` can be retried                                                                                              | plan review (3)                                | solver: retry after missing global                 |
| 8   | The API logs every refused or failed siteverify, with Cloudflare's `error-codes` or HTTP status, never the token                                           | owner (W25 C)                                  | none (log line)                                    |
| 9   | A 404 from the mint is a failure like any other: the panel (the gate is on in both environments; a gate-off fallback was built, then dropped)              | owner (2026-11-18)                             | session: 404 → panel                               |
| 10  | Panel colours come from `colorsDark`                                                                                                                       | code review                                    | none                                               |
| 11  | No silent retry: every failure shows the panel at once (dropped after 7 server errors in 31,452 API requests over 30 days)                                 | owner (2026-11-18)                             | solver: panel at once                              |
| 12  | The retry panel is labelled (`aria-labelledby`) and its button takes focus                                                                                 | devil's advocate (3)                           | solver: labels and focus                           |
| 13  | The failure card covers the centre of the page by design; it and the widget fill a phone screen less 16 px a side (card up to 360 px, widget up to 300 px) | owner (2026-11-17), screenshots                | solver: phone width; e2e: widget width on a phone  |
| 14  | The retry panel says only "We couldn't verify your browser" and "Try again", no explanatory sentence                                                       | owner (2026-11-19)                             | solver: title and button only; e2e: no explanation |

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
- A widget that never calls back (none of Cloudflare's documented callbacks
  fires) leaves the page loading with no card; there is no timer of our own.
  Owner, 2026-11-18: like a hung connection, not handled here.
- A connection that hangs while the Turnstile script loads or the session is
  minted (no answer, no error) keeps the page loading with no card. Owner,
  2026-11-18: the visitor's network,
  not handled here.
- `session.ts` now holds the session, the widget and the panel: split in a
  `[structure only]` PR; `session-widget.test.ts` then follows its source file.
- One passed check shared by all tabs and reloads (an HttpOnly cookie set by
  the API, per OWASP): proposed to the owner; the reporter may be opening new
  tabs, which this PR does not change.
- Before mainnet: leave one testnet tab open overnight; nothing tested an
  unattended interactive challenge for hours.
