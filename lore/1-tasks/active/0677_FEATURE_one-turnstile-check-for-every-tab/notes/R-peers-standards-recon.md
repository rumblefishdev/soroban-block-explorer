---
title: 'Peers, standards and code recon for the shared session cookie'
type: research
status: developing
spawns: []
tags: [turnstile, auth, cookie]
links: []
history:
  - date: 2026-11-19
    status: developing
    who: karolkow
    note: 'Second research pass, asked for by the owner before the grilling round.'
---

# Peers, standards and code recon for the shared session cookie

## Standards

| Who                                             | What it says                                                                                                                                | Source                              |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------- |
| IETF OAuth for Browser-Based Apps, BFF §6.1.3.2 | Cookie MUST be `Secure`, `HttpOnly`; SHOULD be `SameSite=Strict`, `Path=/`, no `Domain`, prefix `__Host-Http-`                              | draft-ietf-oauth-browser-based-apps |
| same, §6.1.3.3                                  | SameSite suffices only if no other app shares the site; else a static custom header on every request (which brings back the CORS preflight) | same                                |
| same, §6.1.2.1                                  | The frontend learns the session state from a "check session" endpoint                                                                       | same                                |
| OWASP ASVS 5.0 V3.3.1–3.3.4                     | `Secure` + `__Host-` prefix; SameSite fits the purpose; session cookies `HttpOnly`                                                          | ASVS v5.0.0 `0x12-V3`               |
| OWASP ASVS 5.0 V7.1.1, 7.3.2                    | Document the idle and absolute lifetime; enforce the absolute one                                                                           | ASVS v5.0.0 `0x16-V7`               |

## Peers

| Topic                     | Who · how                                                                                                                                                                                                                                                                                                                         | Source                                                                                                            |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| Same design               | **Vercel BotID "verify on page load"**: browser POSTs `/api/verify`, server sets `__Host-botid-verification` (HttpOnly, Secure, `SameSite=Strict`, 5-min JWT); later endpoints answer 401 when it is missing; the sample UI says "Reload to try again", no replay                                                                 | vercel.com/docs/botid/verify-on-page-load                                                                         |
| 401 → replay once         | ky `afterResponse` (retry once on 401 after refresh); axios-auth-refresh (one shared refresh, queued requests resent, `maxRetries`); Cloudflare Turnstile pre-clearance wrapper (`cf-mitigated: challenge` → widget → replay once)                                                                                                | ky readme.md:707-760@3541888; axios-auth-refresh src/index.ts:50-80@985a91a; Cloudflare blog                      |
| Probe first / expiry hint | Duende BFF `/bff/user` (401 when anonymous) + `bff:session_expires_in`; BotID page-load verify                                                                                                                                                                                                                                    | docs.duendesoftware.com/bff                                                                                       |
| Explorer with a gate      | Blockscout: optimistic reads, 429 → "I'm not a robot" → `api-v2-temp-token` (+ TTL header) kept in a JS cookie, then `location.reload()`; Solscan: Cloudflare managed challenge, exposes `cf-mitigated` to JS; Etherscan site `ASP.NET_SessionId` `SameSite=Lax`, API by key; stellar.expert, StellarChain, Horizon: no gate seen | blockscout/frontend AppErrorTooManyRequests.tsx:36-85@13456e2; measured 2026-11-19 (one GET each)                 |
| Several tabs              | auth0-spa-js: Web Locks; supabase auth-js: dropped Web Locks (v2.107) for in-tab single-flight + server-side race handling; Firebase: IndexedDB poll. All hold tokens in JS, so only the refresh coordination compares                                                                                                            | auth0-spa-js src/lock.ts:29-39@eb994ca; supabase-js lockless-coordination.md@02d2e8f                              |
| Old tabs after a deploy   | Vercel Skew Protection pins old tabs to the old backend (12 h–1 day); Vite `vite:preloadError` reloads on a missing chunk. No peer found that accepts Bearer and cookie for one release                                                                                                                                           | vercel.com/docs/skew-protection; vite docs/guide/build.md:73-85                                                   |
| SameSite                  | Strict: IETF BFF, Vercel BotID, Vercel `__vdpl`. Lax: ASP.NET Core cookie auth default, supabase/ssr, Etherscan, our Prices portal. Safari reports of missing same-site cookies: unconfirmed                                                                                                                                      | aspnetcore CookieAuthenticationOptions.cs:19; supabase/ssr constants.ts:5-9                                       |
| AOP before API Gateway    | Lychee ltbase and truvity/grun trust Cloudflare's **global** origin-pull CA (proves "from Cloudflare", not "from our account"); no peer with an own cert found; whether a handshake-rejected request counts against the throttle or the bill: not established                                                                     | ltbase docs/superpowers/specs/2026-04-08-cloudflare-apigw-mtls-design.md@f750857; grun cdk/test/stack.test.ts:494 |

## Code recon (develop)

- Retry point: a hey-api **response** interceptor runs before the `ok`
  check (`libs/api-types/src/generated/client/client.gen.ts:116-129`), so
  it can return a repeated response; `client.setConfig({credentials:
'include'})` reaches every request (`types.gen.ts:15-17`).
- All SPA reads are GET (30 `.get`, 1 `.post` = `mintSession`, unused by
  the SPA). `web/src/search/federation.ts:109` calls third-party servers
  and must not send credentials.
- Old bundle against an API without the body JWT (`session.ts:94-98`):
  `token` becomes `undefined`, every query solves, mints and still gets
  401 — the page is broken until reload. With the body JWT and `Bearer`
  kept, the old bundle works unchanged.
- Deploy units (`docs/deployment.md`): Lambda + axum CORS =
  `Explorer-<env>-Compute`; preflight = `Explorer-<env>-ApiGateway`; SPA =
  `deploy-<env>-web`.
- Tests to rewrite: `web/src/api/__tests__/client.test.ts:93-132` (Bearer,
  no in-call retry), `session.test.ts` (token-returning `ensureToken`),
  `web/e2e/turnstile.spec.ts:79,105-149` (Bearer asserted; API mocked
  same-origin). Rust gate tests `crates/api/src/auth/mod.rs:186-327`. No
  infra test covers the preflight headers.
- No local path proves a cross-origin cookie: dev is same-origin through
  the Vite proxy, `bin/local` has no auth layer, Playwright mocks the API,
  Safari refuses `Secure` cookies on `http://localhost` (WebKit 218980).
  The first real proof is testnet.
- Cargo: no cookie crate. `axum-extra` 0.12.5 (feature `cookie`) matches
  the locked axum 0.8.8; 0.12.6 bumps axum to 0.8.9. New crates: `axum-extra`,
  `cookie`; `time`, `percent-encoding` already locked.
- Docs to update: `docs/architecture/backend/backend-overview.md:127-131,867`,
  `docs/architecture/frontend/frontend-overview.md:123-130,241-245`, ADR
  0048:30-31; four comments cite the missing
  `docs/paid-api/plan-platne-api.md`.
