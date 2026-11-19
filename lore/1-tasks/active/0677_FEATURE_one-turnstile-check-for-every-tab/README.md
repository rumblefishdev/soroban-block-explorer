---
id: '0677'
title: 'FEATURE: one Turnstile check for every tab (HttpOnly session cookie)'
type: FEATURE
status: active
related_adr: ['0048']
related_tasks: ['0289', '0277', '0346', '0678']
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
- Next: the one PR of the PR split, on a feature branch, test first.
- In force: rows 1–11 of Decisions so far.

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

| #   | What is done                                                                                                                                                                              | From                                           | vs the task | Who does the same                                               | Code | Test             |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- | ----------- | --------------------------------------------------------------- | ---- | ---------------- |
| 1   | The gate stays on every read and the session stays 15 min: the x402 grant (Tranche #3) gates the explorer API, so a free read path would undercut it                                      | owner, team daily 2026-11-17                   | matches     | —                                                               | none | none (no change) |
| 2   | One passed check serves every tab and reload: the session lives in an HttpOnly cookie set by the API, per the OWASP Session Management Cheat Sheet                                        | owner, team daily 2026-11-17                   | matches     | Cloudflare pre-clearance, Blockscout, Prices portal (see Peers) | —    | to write         |
| 3   | The site stays on CloudFront, not behind Cloudflare: it holds no data, the API that does is already behind Cloudflare, and 30 days showed 43,006 requests with no attack                  | owner, team daily 2026-11-17 (keeps task 0302) | matches     | —                                                               | none | none (no change) |
| 4   | Build the HttpOnly cookie now, not a JWT in `localStorage` and not a measurement first (premise review: frequency doubtful)                                                               | owner, grilling W3 A, 2026-11-19               | matches     | Prices portal, Blockscout, Cloudflare pre-clearance             | —    | to write         |
| 5   | A read goes out at once; on a 401 the SPA runs the Turnstile check, mints, and repeats that read once, so no page shows the 401 (replaces 0289 decision 1: the SPA cannot see the cookie) | owner, grilling W4 A, 2026-11-19               | beyond it   | ky, axios-auth-refresh, Cloudflare pre-clearance wrapper        | —    | to write         |
| 6   | One cut-over: the API sets only the cookie and stops returning the JWT and accepting `Bearer` in the same change; a tab still on the old bundle reloads or shows a brief error            | owner, grilling W10 B, 2026-11-19              | beyond it   | Vite `vite:preloadError` (reload)                               | —    | to write         |
| 7   | Each tab handles its own 401; no lock shared between tabs                                                                                                                                 | owner, grilling W5 A, 2026-11-19               | beyond it   | supabase-js (dropped Web Locks in v2.107)                       | —    | none (no code)   |
| 8   | First real proof of the cookie is testnet: deploy, the owner checks, then mainnet (no local path crosses origins)                                                                         | owner, grilling W13 A, 2026-11-19              | beyond it   | —                                                               | —    | testnet run      |
| 9   | The cookie has `SameSite=Strict` (with `__Host-`, `HttpOnly`, `Secure`, `Path=/`, `Max-Age=900`, no `Domain`)                                                                             | owner, grilling W11 A, 2026-11-19              | beyond it   | IETF BFF §6.1.3.2, Vercel BotID                                 | —    | to write         |
| 10  | Authenticated Origin Pulls is not part of this task: a separate backlog task 0678 for later                                                                                               | owner, grilling W6 B, 2026-11-19               | beyond it   | —                                                               | none | none             |
| 11  | No extra custom header on reads: one request per read; a page on a taken-over subdomain still cannot read the answer (CORS list)                                                          | owner, grilling W12 A, 2026-11-19              | beyond it   | Vercel BotID, Blockscout                                        | —    | none (no code)   |
| 12  | A 401 for a read sent before the last mint finished repeats the read without a new check                                                                                                  | owner, W21 A, 2026-11-19                       | beyond it   | axios-auth-refresh                                              | —    | to write         |
| 13  | The API logs one line per mint and per 401, saying whether a cookie came                                                                                                                  | owner, W23 A, 2026-11-19                       | beyond it   | —                                                               | —    | to write         |

## Not yet decided

- Foundations review (notes/S-foundations-review.md) questions decision 1:
  is the gate on every read needed for x402 at all.
- Browsers that block all cookies (B8): cookie only, or a Bearer fallback.
- Own cookie parsing vs `axum-extra` (R7): benefit being measured.

## PR split

One PR (decision 6 leaves no step between API and SPA): cookie on a `204`
mint, gate reads the cookie, `Bearer`, the body JWT and the `bearer_jwt`
OpenAPI scheme removed, `allowCredentials` on both CORS layers, SPA
`credentials: 'include'` (client and mint) and the repeat after 401, tests,
docs, API types regenerated. Deploy per stack: `ApiGateway` → `Compute` →
`web`, testnet first with the same order, then mainnet (decision 8).
Plan review and testnet checklist: notes/S-plan-review.md.

## Out of scope

- `Cache-Control: public` on gated 200s (F6): no shared cache today; the
  same gap exists with `Bearer`. Whoever adds a Cloudflare cache rule must
  make gated responses `private` or key the cache on the session.
- x402 payment on the explorer API (F7): its order slot is after the session
  check, before the 401; nothing is built here. The x402 task must decide how
  the SPA treats a 402 from an expired session (R10).
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
