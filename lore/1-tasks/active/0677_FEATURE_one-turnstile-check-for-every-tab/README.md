---
id: '0677'
title: 'FEATURE: one Turnstile check for every tab (HttpOnly session cookie)'
type: FEATURE
status: active
related_adr: ['0048']
related_tasks: ['0289', '0277', '0346']
tags:
  [
    effort-medium,
    priority-medium,
    layer-api,
    layer-frontend,
    layer-infra,
    turnstile,
    auth,
  ]
links: []
history:
  - date: 2026-11-19
    status: active
    who: karolkow
    note: >
      Created after task 0289 closed. Carries the three decisions of the team
      daily of 2026-11-17, recorded only on the unmerged branch
      feat/0289-session-cookie (d4c019f78). Grilling before any code.
---

# FEATURE: one Turnstile check for every tab (HttpOnly session cookie)

## Summary

Today every tab and every reload runs its own Turnstile check: the session
JWT lives in the memory of one tab for 15 min (`web/src/api/session.ts`) and
travels as `Authorization: Bearer`. A visitor with three tabs passes three
checks, and again after each reload. The API should set the session as an
HttpOnly cookie, so one passed check serves every tab and reload of the same
browser until it expires.

## Stan teraz

- Done: first research (cookie attributes, CORS, CSRF, caching, x402, AOP);
  measurement F12; row 4.
- Next: grilling round 2 (Not yet decided), after the deeper research and
  code recon the owner asked for (notes/R-peers-standards-recon.md).
- In force: rows 1–4 of Decisions so far.

## Context

- SPA `https://sorobanscan.com` (CloudFront), API `https://api.sorobanscan.com`
  (API Gateway REST + axum Lambda, proxied by Cloudflare); testnet
  `testnet.sorobanscan.com` / `api-testnet.sorobanscan.com`
  (source: `infra/envs/{production,testnet}.json`). The legacy SPA host
  `sorobanscan.rumblefish.dev` only answers `301` (source:
  `infra/src/lib/stacks/delivery-stack.ts:54`).
- The gate (`crates/api/src/auth/mod.rs`, `require_auth`): exempt paths, then a
  paid `X-API-Key`, then the session JWT, else `401`. It runs inside the
  edge-secret lock (`crates/api/src/common/edge_lock.rs`).
- Mint: `POST /auth/session` verifies the Turnstile token and returns
  `{token, expires_in: 900}` with `no-store`.

## Premise

Fresh opus review of the goal alone (2026-11-19):

- **doubtful** — the problem is frequent: not measured (mints per visitor in
  15 min; interactive share in Turnstile analytics). The 0289 report was an
  overnight tab, caused by the 15-min TTL, which a cookie does not change.
- **wrong** — a cookie alone gives one check: at expiry each visible tab gets
  a 401 and checks on its own, unless the tabs share the check (F9 narrows it
  to visible tabs).
- **wrong** — a reload needs no check with a cookie alone: the SPA learns of a
  missing session only from a 401, and `QueryProvider` does not retry 4xx; the
  interceptor must repeat the request after the check.
- **holds** — a cookie can carry the session (same-site, F1; the Prices portal
  does it).
- **doubtful** — HttpOnly protects much here: the session grants 15 min of
  public reads. A `localStorage` JWT + Web Locks (frontend only) is cheaper.
- **wrong** — pre-clearance as a replacement: a Cloudflare challenge page
  would hide the origin's x402 `402` (derived, untested).

## Facts

F1–F11 (same-site, cookie attributes, CORS, CSRF, caching, x402, AOP) and the
peer comparison: [notes/R-facts-and-peers.md](notes/R-facts-and-peers.md).

## Decisions so far

| #   | What is done                                                                                                                                                             | From                                           | vs the task | Who does the same                                               | Code | Test             |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------- | ----------- | --------------------------------------------------------------- | ---- | ---------------- |
| 1   | The gate stays on every read and the session stays 15 min: the x402 grant (Tranche #3) gates the explorer API, so a free read path would undercut it                     | owner, team daily 2026-11-17                   | matches     | —                                                               | none | none (no change) |
| 2   | One passed check serves every tab and reload: the session lives in an HttpOnly cookie set by the API, per the OWASP Session Management Cheat Sheet                       | owner, team daily 2026-11-17                   | matches     | Cloudflare pre-clearance, Blockscout, Prices portal (see Peers) | —    | to write         |
| 3   | The site stays on CloudFront, not behind Cloudflare: it holds no data, the API that does is already behind Cloudflare, and 30 days showed 43,006 requests with no attack | owner, team daily 2026-11-17 (keeps task 0302) | matches     | —                                                               | none | none (no change) |
| 4   | Build the HttpOnly cookie now, not a JWT in `localStorage` and not a measurement first (premise review: frequency doubtful)                                              | owner, grilling W3 A, 2026-11-19               | matches     | Prices portal, Blockscout, Cloudflare pre-clearance             | —    | to write         |

## Not yet decided

Round 2 of the grilling (research: notes/R-peers-standards-recon.md):

- How a tab learns its session is missing: 401 and one repeated request,
  a check-session probe, or an expiry hint. Touches 0289 decision 1.
- Old tabs after the deploy: one release where the API accepts both the
  cookie and `Bearer`, or the cookie only at once.
- `SameSite=Strict` or `Lax`.
- A static custom header on every read (IETF BFF §6.1.3.3), or none.
- Several visible tabs at one expiry: Web Locks, or each tab on its own.
- PR split and where the cookie is first proven (no local cross-origin path).
- Authenticated Origin Pulls: in this task, a separate task, or not now.

## Out of scope

- `Cache-Control: public` on gated 200s (F6): no shared cache today; the
  same gap exists with `Bearer`. Whoever adds a Cloudflare cache rule must
  make gated responses `private` or key the cache on the session.
- x402 payment on the explorer API (F7): its order slot is after the session
  check, before the 401; nothing is built here.
- Server-side logout or revocation: a stateless 15-min JWT has none (OWASP
  "Logout Button"); a free read gate has no logout.

## Acceptance Criteria

- [ ] After one passed check, a second tab and a reload read data with no new
      check until the session expires.
- [ ] No page shows a 401 or a broken section when the session expires.
- [ ] Mainnet and testnet sessions never mix.
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md`,
      `docs/architecture/frontend/frontend-overview.md` (auth flow).
- [ ] **API types regenerated** — `crates/api/**` changes (mint response).

## Notes

- `crates/api/src/auth/mod.rs` and `edge_lock.rs` cite
  `docs/paid-api/plan-platne-api.md`, which is not on `develop`; fix the
  comment when the file is touched.
- The task 0317 comment on the preflight `maxAge` (`api-gateway-stack.ts`)
  becomes true only for the mint once `Authorization` is gone (F4).
